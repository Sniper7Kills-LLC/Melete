//! Background fetchers for fetch-backed template widgets.
//!
//! Per the project memory, every fetch widget has a freshness policy
//! (see `WidgetKind` definitions in `journal-core`):
//!
//! - `Once` widgets (Quote, BibleVerse, Sunrise, MoonPhase, OnThisDay,
//!   WordOfDay, Astronomy) fetch a single time at the page's first
//!   open and freeze forever.
//! - `UntilDate` widgets (Weather, RssHeadline) refetch as long as the
//!   page is opened on its bound calendar date — once the date is in
//!   the past, the entry is frozen and never touched again.
//!
//! Fetches run on a worker thread (sync ureq); results are pushed to
//! a `Mutex<Vec<PendingUpdate>>` queue. A main-thread
//! `glib::timeout_add_local` poller drains the queue, merges results
//! into `state.current_page_widget_data`, persists with `update_page`,
//! and triggers a `queue_draw` on the canvas. The poller approach
//! avoids GTK's "objects aren't `Send`" constraint that prevents the
//! worker thread from holding the canvas / state directly.
//!
//! Every endpoint here is a free public API with no auth. APIs that
//! need authentication (Gmail, Google Calendar, Todoist, NewsAPI,
//! authenticated stocks feeds, etc.) are documented but intentionally
//! NOT wired into the UI — see `auth_required_widgets` below.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::DrawingArea;
use melete_core::{
    Freshness, OnThisDayEvent, PageId, RssItem, TemplateWidget, WeatherDay, WidgetData, WidgetKind,
    WidgetPayload,
};
use uuid::Uuid;

use crate::state::SharedState;

/// One result coming back from the worker thread; drained by the main
/// thread poller.
struct PendingUpdate {
    page_id: PageId,
    widget_id: Uuid,
    data: WidgetData,
}

fn pending_queue() -> &'static Mutex<Vec<PendingUpdate>> {
    static Q: OnceLock<Mutex<Vec<PendingUpdate>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(Vec::new()))
}

/// Install the main-thread poller that drains fetched results onto the
/// current page. Call once at app startup, after the canvas exists.
/// The poller runs every 250ms — slow enough not to burn CPU, fast
/// enough that fetched widgets appear within half a second of their
/// HTTP response landing.
pub fn install_poller(state: SharedState, canvas: DrawingArea) {
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let drained: Vec<PendingUpdate> = {
            let mut q = pending_queue().lock().unwrap();
            std::mem::take(&mut *q)
        };
        if drained.is_empty() {
            return glib::ControlFlow::Continue;
        }
        let mut redraw = false;
        for upd in drained {
            let applied_to_current = {
                let mut s = state.borrow_mut();
                if s.current_page_id == Some(upd.page_id) {
                    s.current_page_widget_data
                        .insert(upd.widget_id, upd.data.clone());
                    true
                } else {
                    false
                }
            };
            persist_single(state.clone(), upd.page_id, upd.widget_id, upd.data);
            if applied_to_current {
                redraw = true;
            }
        }
        if redraw {
            canvas.queue_draw();
        }
        glib::ControlFlow::Continue
    });
}

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const WEATHER_REFRESH: Duration = Duration::from_secs(60 * 60); // 1 hour
const RSS_REFRESH: Duration = Duration::from_secs(60 * 30); // 30 min

/// Returns the cache policy for a widget kind. `None` means the widget
/// does not pull external data (so the fetcher skips it entirely).
pub fn freshness_for(kind: &WidgetKind) -> Option<Freshness> {
    match kind {
        WidgetKind::Weather { .. } => Some(Freshness::UntilDate),
        WidgetKind::Quote { .. } => Some(Freshness::Once),
        WidgetKind::BibleVerse { .. } => Some(Freshness::Once),
        WidgetKind::Sunrise { .. } => Some(Freshness::Once),
        WidgetKind::MoonPhase => Some(Freshness::Once),
        WidgetKind::OnThisDay { .. } => Some(Freshness::Once),
        WidgetKind::WordOfDay { .. } => Some(Freshness::Once),
        WidgetKind::RssHeadline { .. } => Some(Freshness::UntilDate),
        WidgetKind::Astronomy { .. } => Some(Freshness::Once),
        _ => None,
    }
}

fn refresh_threshold(kind: &WidgetKind) -> Duration {
    match kind {
        WidgetKind::Weather { .. } => WEATHER_REFRESH,
        WidgetKind::RssHeadline { .. } => RSS_REFRESH,
        _ => Duration::from_secs(60 * 60 * 24),
    }
}

/// Decide whether `widget` needs a fresh fetch given the current cache
/// entry and the page's bound date (if any).
fn needs_refetch(
    kind: &WidgetKind,
    cached: Option<&WidgetData>,
    page_date: Option<NaiveDate>,
    now: DateTime<Utc>,
) -> bool {
    let Some(policy) = freshness_for(kind) else {
        return false;
    };

    // First fetch — always go.
    let Some(data) = cached else {
        return true;
    };

    // Frozen entries never refetch, even if the policy would otherwise
    // say to refresh. Frozen flips on at the moment a UntilDate page's
    // date passes (handled in `mark_stale_frozen` below).
    if data.frozen {
        return false;
    }

    match policy {
        Freshness::Once => false,
        Freshness::UntilDate => {
            // Only refresh when the page is bound to today and the
            // cache is older than the per-widget threshold.
            let today = chrono::Local::now().date_naive();
            match page_date {
                Some(d) if d == today => {
                    now.signed_duration_since(data.fetched_at).to_std().ok()
                        > Some(refresh_threshold(kind))
                }
                _ => false,
            }
        }
    }
}

/// Walk the page's UntilDate widgets and freeze any entries whose
/// bound date has passed. Run before evaluating refresh need so that a
/// page opened weeks later doesn't refetch yesterday's weather.
fn mark_stale_frozen(
    widgets: &[TemplateWidget],
    page_date: Option<NaiveDate>,
    cache: &mut HashMap<Uuid, WidgetData>,
) {
    let today = chrono::Local::now().date_naive();
    for w in widgets {
        let Some(Freshness::UntilDate) = freshness_for(&w.kind) else {
            continue;
        };
        if let Some(entry) = cache.get_mut(&w.id) {
            if let Some(d) = page_date {
                if d < today {
                    entry.frozen = true;
                }
            }
        }
    }
}

/// Public entry point — invoked by `set_current_page` after a page's
/// strokes / overrides / cache have been loaded. Walks the template's
/// fetch widgets, decides which need a refresh, and dispatches a
/// background worker thread that drops `PendingUpdate`s onto the
/// shared queue (drained by `install_poller` on the main thread).
pub fn schedule_fetches_for_current_page(state: SharedState) {
    let (page_id, widgets, cache, page_date, persist_freeze) = {
        let mut s = state.borrow_mut();
        let Some(page_id) = s.current_page_id else {
            return;
        };
        let Some(template) = s.current_template.clone() else {
            return;
        };
        let widgets = template.widgets.clone();
        let page_date = s.current_page_date;
        // Apply the freeze rule on UntilDate widgets first so we don't
        // refetch a page bound to a past date.
        let mut owned_cache = s.current_page_widget_data.clone();
        let before = owned_cache.clone();
        mark_stale_frozen(&widgets, page_date, &mut owned_cache);
        let persist_freeze = before != owned_cache;
        s.current_page_widget_data = owned_cache.clone();
        (page_id, widgets, owned_cache, page_date, persist_freeze)
    };

    let now = Utc::now();
    let to_fetch: Vec<TemplateWidget> = widgets
        .iter()
        .filter(|w| needs_refetch(&w.kind, cache.get(&w.id), page_date, now))
        .cloned()
        .collect();

    if persist_freeze {
        // Persist the freeze flips so future opens don't re-walk them.
        persist_full_cache(state.clone(), page_id, cache.clone());
    }
    if to_fetch.is_empty() {
        return;
    }

    std::thread::spawn(move || {
        for widget in to_fetch {
            let payload = match fetch_widget_blocking(&widget, page_date) {
                Ok(p) => p,
                Err(e) => WidgetPayload::Error { message: e },
            };
            let data = WidgetData {
                payload,
                fetched_at: Utc::now(),
                frozen: matches!(freshness_for(&widget.kind), Some(Freshness::Once)),
            };
            pending_queue().lock().unwrap().push(PendingUpdate {
                page_id,
                widget_id: widget.id,
                data,
            });
        }
    });
}

fn persist_single(state: SharedState, page_id: PageId, widget_id: Uuid, data: WidgetData) {
    let backend = state.borrow().backend.clone();
    let mut b = backend.borrow_mut();
    let mut page = match b.get_page(page_id) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("fetcher: get_page({:?}) failed: {}", page_id, e);
            return;
        }
    };
    page.widget_data.insert(widget_id, data);
    if let Err(e) = b.update_page(&page) {
        tracing::warn!("fetcher: update_page({:?}) failed: {}", page_id, e);
    }
}

fn persist_full_cache(
    state: SharedState,
    page_id: PageId,
    cache: HashMap<Uuid, WidgetData>,
) {
    let backend = state.borrow().backend.clone();
    let mut b = backend.borrow_mut();
    let mut page = match b.get_page(page_id) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("fetcher: get_page({:?}) failed: {}", page_id, e);
            return;
        }
    };
    page.widget_data = cache;
    if let Err(e) = b.update_page(&page) {
        tracing::warn!("fetcher: update_page({:?}) failed: {}", page_id, e);
    }
}

// ---------------------------------------------------------------------------
// Per-widget blocking fetch — runs on the worker thread.
// ---------------------------------------------------------------------------

pub fn fetch_widget_blocking(
    widget: &TemplateWidget,
    page_date: Option<NaiveDate>,
) -> Result<WidgetPayload, String> {
    let date = page_date.unwrap_or_else(|| chrono::Local::now().date_naive());
    match &widget.kind {
        WidgetKind::Weather {
            lat,
            lon,
            location_label,
            days,
        } => fetch_weather(*lat, *lon, location_label, *days),
        WidgetKind::Quote { source } => fetch_quote(source),
        WidgetKind::BibleVerse {
            reference,
            translation,
        } => fetch_bible_verse(reference, translation),
        WidgetKind::Sunrise { lat, lon } => fetch_sunrise(*lat, *lon, date),
        WidgetKind::MoonPhase => Ok(compute_moon_phase(date)),
        WidgetKind::OnThisDay { lang, max_events } => fetch_on_this_day(lang, *max_events, date),
        WidgetKind::WordOfDay { lang } => fetch_word_of_day(lang),
        WidgetKind::RssHeadline { url, count } => fetch_rss(url, *count),
        WidgetKind::Astronomy { lat, lon } => fetch_astronomy(*lat, *lon, date),
        _ => Err("not a fetch widget".into()),
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(HTTP_TIMEOUT)
        .user_agent("journal-app/0.1 (+https://github.com/)")
        .build()
}

fn http_get_json(url: &str) -> Result<serde_json::Value, String> {
    let agent = agent();
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| format!("http {}", e))?;
    let body = resp
        .into_string()
        .map_err(|e| format!("decode body: {}", e))?;
    serde_json::from_str(&body).map_err(|e| format!("json parse: {}", e))
}

fn http_get_text(url: &str) -> Result<String, String> {
    let agent = agent();
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| format!("http {}", e))?;
    resp.into_string().map_err(|e| format!("decode: {}", e))
}

// ---- Weather (Open-Meteo) -------------------------------------------------

fn fetch_weather(
    lat: f64,
    lon: f64,
    label: &str,
    days: u32,
) -> Result<WidgetPayload, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}\
         &current=temperature_2m,weather_code\
         &daily=weather_code,temperature_2m_max,temperature_2m_min\
         &timezone=auto&forecast_days={}",
        lat,
        lon,
        days.clamp(1, 7),
    );
    let body = http_get_json(&url)?;
    let current = body.get("current").ok_or("missing current")?;
    let current_c = current
        .get("temperature_2m")
        .and_then(|v| v.as_f64())
        .ok_or("missing temperature")?;
    let current_code = current
        .get("weather_code")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let daily = body.get("daily").ok_or("missing daily")?;
    let dates = daily
        .get("time")
        .and_then(|v| v.as_array())
        .ok_or("no daily.time")?;
    let codes = daily
        .get("weather_code")
        .and_then(|v| v.as_array())
        .ok_or("no daily.weather_code")?;
    let his = daily
        .get("temperature_2m_max")
        .and_then(|v| v.as_array())
        .ok_or("no daily.max")?;
    let los = daily
        .get("temperature_2m_min")
        .and_then(|v| v.as_array())
        .ok_or("no daily.min")?;
    let mut out_days = Vec::new();
    let n = dates.len().min(codes.len()).min(his.len()).min(los.len());
    for i in 0..n {
        out_days.push(WeatherDay {
            date: dates[i].as_str().unwrap_or("").to_string(),
            hi_c: his[i].as_f64().unwrap_or(0.0),
            lo_c: los[i].as_f64().unwrap_or(0.0),
            code: codes[i].as_u64().unwrap_or(0) as u32,
        });
    }
    Ok(WidgetPayload::Weather {
        location_label: label.to_string(),
        current_c,
        current_code,
        days: out_days,
    })
}

// ---- Quote (zenquotes.io / quotable.io / local) ---------------------------

fn fetch_quote(source: &str) -> Result<WidgetPayload, String> {
    match source {
        "quotable" => {
            let body = http_get_json("https://api.quotable.io/random")?;
            let text = body
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let author = body
                .get("author")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            Ok(WidgetPayload::Quote { text, author })
        }
        "local" => {
            // Read from ~/.config/journal/quotes.toml — array of
            // {text, author} entries. Picks a deterministic entry per
            // calendar date so the same day always shows the same
            // quote (avoids surprise changes within a single day).
            let path = dirs::config_dir()
                .ok_or("no config dir")?
                .join("melete")
                .join("quotes.toml");
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("read {}: {}", path.display(), e))?;
            let parsed: toml::Value =
                toml::from_str(&text).map_err(|e| format!("parse quotes.toml: {}", e))?;
            let arr = parsed
                .get("quotes")
                .and_then(|v| v.as_array())
                .ok_or("expected [[quotes]] table array")?;
            if arr.is_empty() {
                return Err("quotes.toml is empty".into());
            }
            let today = chrono::Local::now().date_naive();
            let idx = (today.num_days_from_ce() as usize) % arr.len();
            let q = &arr[idx];
            let text = q
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let author = q
                .get("author")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            Ok(WidgetPayload::Quote { text, author })
        }
        _ => {
            // Default = zenquotes.io
            let body = http_get_json("https://zenquotes.io/api/today")?;
            let arr = body.as_array().ok_or("expected array")?;
            let item = arr.first().ok_or("empty zenquotes response")?;
            let text = item
                .get("q")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let author = item
                .get("a")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            Ok(WidgetPayload::Quote { text, author })
        }
    }
}

// ---- Bible verse (bible-api.com) ------------------------------------------

fn fetch_bible_verse(
    reference: &str,
    translation: &str,
) -> Result<WidgetPayload, String> {
    let endpoint_ref = if reference.eq_ignore_ascii_case("random") {
        "?random=verse".to_string()
    } else {
        urlencode(reference)
    };
    let url = format!(
        "https://bible-api.com/{}{}translation={}",
        endpoint_ref,
        if endpoint_ref.starts_with('?') {
            "&"
        } else {
            "?"
        },
        translation,
    );
    let body = http_get_json(&url)?;
    let text = body
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let reference_out = body
        .get("reference")
        .and_then(|v| v.as_str())
        .unwrap_or(reference)
        .to_string();
    let translation_out = body
        .get("translation_id")
        .and_then(|v| v.as_str())
        .unwrap_or(translation)
        .to_string();
    Ok(WidgetPayload::BibleVerse {
        reference: reference_out,
        text,
        translation: translation_out,
    })
}

// ---- Sunrise / sunset (sunrise-sunset.org) --------------------------------

fn fetch_sunrise(lat: f64, lon: f64, date: NaiveDate) -> Result<WidgetPayload, String> {
    let url = format!(
        "https://api.sunrise-sunset.org/json?lat={:.4}&lng={:.4}&date={}&formatted=0",
        lat, lon, date
    );
    let body = http_get_json(&url)?;
    let results = body.get("results").ok_or("no results")?;
    let sunrise = results
        .get("sunrise")
        .and_then(|v| v.as_str())
        .ok_or("no sunrise")?;
    let sunset = results
        .get("sunset")
        .and_then(|v| v.as_str())
        .ok_or("no sunset")?;
    let day_seconds = results
        .get("day_length")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // sunrise/sunset come back as ISO-8601 UTC; convert to local.
    let sunrise_local = iso_to_local_hm(sunrise);
    let sunset_local = iso_to_local_hm(sunset);
    let h = day_seconds / 3600;
    let m = (day_seconds % 3600) / 60;
    let daylight_hms = format!("{}h {:02}m", h, m);

    Ok(WidgetPayload::Sunrise {
        sunrise_local,
        sunset_local,
        daylight_hms,
    })
}

fn iso_to_local_hm(s: &str) -> String {
    use chrono::Local;
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|_| s.to_string())
}

// ---- Moon phase (local computation) ---------------------------------------

fn compute_moon_phase(date: NaiveDate) -> WidgetPayload {
    // Conway's algorithm — synodic month is ~29.53 days; phase 0..7 maps
    // to a named phase. Accurate to within a day, fine for journal use.
    let r = date.year() % 100;
    let mut r = r % 19;
    if r > 9 {
        r -= 19;
    }
    let r_b = (r * 11) % 30
        + date.month() as i32
        + date.day() as i32;
    let r_b = if date.month() < 3 {
        r_b + 2
    } else {
        r_b
    };
    let r_b = r_b - if date.year() < 2000 { 4 } else { 8 };
    let phase_day = ((r_b % 30) + 30) % 30;
    // Map phase_day ∈ [0,29] → label + emoji + illumination percent.
    let phase_frac = phase_day as f64 / 29.5306;
    let illumination_pct = (1.0 - (1.0 - 2.0 * phase_frac).abs()) * 100.0;
    let (name, emoji) = match phase_day {
        0..=1 => ("New Moon", "\u{1F311}"),
        2..=6 => ("Waxing Crescent", "\u{1F312}"),
        7..=8 => ("First Quarter", "\u{1F313}"),
        9..=13 => ("Waxing Gibbous", "\u{1F314}"),
        14..=16 => ("Full Moon", "\u{1F315}"),
        17..=21 => ("Waning Gibbous", "\u{1F316}"),
        22..=23 => ("Last Quarter", "\u{1F317}"),
        _ => ("Waning Crescent", "\u{1F318}"),
    };
    WidgetPayload::MoonPhase {
        name: name.to_string(),
        illumination_pct,
        emoji: emoji.to_string(),
    }
}

// ---- On this day (Wikipedia REST) -----------------------------------------

fn fetch_on_this_day(
    lang: &str,
    max_events: u32,
    date: NaiveDate,
) -> Result<WidgetPayload, String> {
    let url = format!(
        "https://{}.wikipedia.org/api/rest_v1/feed/onthisday/events/{:02}/{:02}",
        lang,
        date.month(),
        date.day(),
    );
    let body = http_get_json(&url)?;
    let arr = body
        .get("events")
        .and_then(|v| v.as_array())
        .ok_or("no events")?;
    let mut events = Vec::new();
    for e in arr.iter().take(max_events.max(1) as usize) {
        let year = e
            .get("year")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let text = e
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        events.push(OnThisDayEvent { year, text });
    }
    Ok(WidgetPayload::OnThisDay { events })
}

// ---- Word of the day (Wiktionary random page summary) ---------------------

fn fetch_word_of_day(lang: &str) -> Result<WidgetPayload, String> {
    let url = format!(
        "https://{}.wiktionary.org/api/rest_v1/page/random/summary",
        lang
    );
    let body = http_get_json(&url)?;
    let word = body
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let definition = body
        .get("extract")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(WidgetPayload::WordOfDay { word, definition })
}

// ---- RSS / Atom -----------------------------------------------------------

fn fetch_rss(url: &str, count: u32) -> Result<WidgetPayload, String> {
    let body = http_get_text(url)?;
    let doc = roxmltree::Document::parse(&body).map_err(|e| format!("xml: {}", e))?;
    let root = doc.root_element();
    let (feed_title, items) = if root.tag_name().name() == "rss" {
        parse_rss(&root, count)
    } else if root.tag_name().name() == "feed" {
        parse_atom(&root, count)
    } else {
        return Err(format!("unsupported feed root: {}", root.tag_name().name()));
    };
    Ok(WidgetPayload::RssHeadline { feed_title, items })
}

fn parse_rss(root: &roxmltree::Node, count: u32) -> (String, Vec<RssItem>) {
    let channel = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "channel");
    let mut items = Vec::new();
    let mut feed_title = String::new();
    if let Some(ch) = channel {
        feed_title = ch
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "title")
            .and_then(|n| n.text())
            .unwrap_or("")
            .to_string();
        for it in ch
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "item")
            .take(count.max(1) as usize)
        {
            let title = child_text(&it, "title");
            let link = child_text(&it, "link");
            let published = child_text(&it, "pubDate");
            items.push(RssItem {
                title,
                link,
                published,
            });
        }
    }
    (feed_title, items)
}

fn parse_atom(root: &roxmltree::Node, count: u32) -> (String, Vec<RssItem>) {
    let feed_title = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "title")
        .and_then(|n| n.text())
        .unwrap_or("")
        .to_string();
    let items = root
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "entry")
        .take(count.max(1) as usize)
        .map(|e| {
            let title = child_text(&e, "title");
            let link = e
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "link")
                .and_then(|n| n.attribute("href"))
                .unwrap_or("")
                .to_string();
            let published = child_text(&e, "published");
            RssItem {
                title,
                link,
                published,
            }
        })
        .collect();
    (feed_title, items)
}

fn child_text(node: &roxmltree::Node, name: &str) -> String {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .and_then(|n| n.text())
        .unwrap_or("")
        .to_string()
}

// ---- Astronomy (Open-Meteo daily astronomy fields) ------------------------

fn fetch_astronomy(lat: f64, lon: f64, date: NaiveDate) -> Result<WidgetPayload, String> {
    // Open-Meteo astro endpoint covers sunrise/sunset/sunshine_duration
    // but doesn't ship visible-planet data — for a simple journal
    // background, the daylight + sunshine duration plus moon phase
    // (computed locally above) is plenty.
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}\
         &daily=sunshine_duration,daylight_duration\
         &timezone=auto&start_date={}&end_date={}",
        lat, lon, date, date,
    );
    let body = http_get_json(&url)?;
    let daily = body.get("daily").ok_or("no daily")?;
    let sun = daily
        .get("sunshine_duration")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let day = daily
        .get("daylight_duration")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let lines = vec![
        format!(
            "Daylight {:.1}h",
            day / 3600.0
        ),
        format!("Sunshine {:.1}h", sun / 3600.0),
    ];
    Ok(WidgetPayload::Astronomy { lines })
}

// ---- URL helpers ---------------------------------------------------------

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Auth-required widgets (NOT wired into the UI)
// ---------------------------------------------------------------------------
//
// The widget kinds below would all be valuable additions to a journal
// but require either OAuth flows, API keys, or stored credentials. Per
// the request that started this work, they are NOT enabled in the
// template editor or the WidgetKind enum — they're sketched here as a
// future-work checklist so the same fetcher pattern can plug them in
// later.
//
// - **Email unread count / preview** — IMAP login required (or
//   OAuth2 for Gmail). Endpoint: any IMAP host. Renders as a header
//   strip with the latest N subjects.
// - **Calendar events for the day** — CalDAV login or Google Calendar
//   OAuth (`https://www.googleapis.com/calendar/v3/calendars/primary/events`).
//   Renders the day's events as a vertical list bound to the page's
//   bound date.
// - **News headlines (NewsAPI)** — `https://newsapi.org/v2/top-headlines`,
//   requires an `apiKey` query parameter from a free dev account. (For
//   no-auth news, use a public RSS feed via `WidgetKind::RssHeadline`
//   instead.)
// - **Stock / portfolio snapshot** — Alpha Vantage / Finnhub / IEX
//   Cloud all require a free API key. A snapshot value frozen at page
//   creation matches the "background data" model.
// - **Tasks (Todoist / Microsoft To Do)** — OAuth2; Todoist exposes
//   `https://api.todoist.com/rest/v2/tasks`. List the day's `due`
//   tasks as a checklist scaffold.
// - **GitHub user activity** — public profile activity is keyless
//   (`https://api.github.com/users/:user/events/public`) but rate
//   limited; private/org activity needs a token. Could be a "your
//   commits today" header.
// - **Last.fm scrobbles** — needs an API key plus optional auth for
//   private listens. Could feed a "now-playing / top artist" strip.
//
// All of these would slot into the same fetch flow: a new
// `WidgetKind` variant + a `WidgetPayload` variant + a fetch fn here.
// The reason they're deferred is that the journal is meant to work
// offline-friendly with public-data widgets first; auth-bearing
// widgets belong on the future "dashboard" page (interactive
// lock-screen) rather than as background data on a written page.
