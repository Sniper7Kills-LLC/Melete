//! Cached payloads for fetch-backed template widgets.
//!
//! Widgets that pull from a network or local data source (Weather, Quote,
//! BibleVerse, RSS headline, etc.) store their last-fetched body here on
//! the `Page` itself — keyed by the `TemplateWidget.id` they target.
//! Renderers only ever read from this cache; the actual fetching lives
//! in the app-layer fetcher and writes back through `journal-storage`.
//!
//! Refresh policy is driven by `Freshness`: `Once` widgets fetch a
//! single time and are then frozen forever; `UntilDate` widgets keep
//! refetching while the bound page-date matches today, then freeze
//! once the date passes.
//!
//! Each widget kind that fetches has its own typed `WidgetPayload`
//! variant — no JSON-string-in-string roundtripping at render time.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How often a fetch-backed widget should refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// Fetch once at first open, freeze forever.
    Once,
    /// Refetch whenever the page is opened *on its bound date*. Once
    /// the bound date is in the past, freeze and never refetch.
    UntilDate,
}

/// Cached fetch result for a single widget instance, stored on the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetData {
    pub payload: WidgetPayload,
    pub fetched_at: DateTime<Utc>,
    /// When true, the fetcher will not refresh this entry again — either
    /// because the policy is `Once` or because the page's bound date has
    /// passed.
    #[serde(default)]
    pub frozen: bool,
}

/// One forecast day in a Weather payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherDay {
    pub date: String,
    pub hi_c: f64,
    pub lo_c: f64,
    pub code: u32,
}

/// One headline in an RSS payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RssItem {
    pub title: String,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub published: String,
}

/// One event in an OnThisDay payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnThisDayEvent {
    pub year: i32,
    pub text: String,
}

/// Typed body for a fetched widget. Each variant matches a `WidgetKind`
/// fetch variant — the renderer dispatches on whichever it pulls from
/// the page's `widget_data` map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WidgetPayload {
    Weather {
        location_label: String,
        current_c: f64,
        current_code: u32,
        days: Vec<WeatherDay>,
    },
    Quote {
        text: String,
        author: String,
    },
    BibleVerse {
        reference: String,
        text: String,
        translation: String,
    },
    Sunrise {
        sunrise_local: String,
        sunset_local: String,
        daylight_hms: String,
    },
    MoonPhase {
        name: String,
        illumination_pct: f64,
        emoji: String,
    },
    OnThisDay {
        events: Vec<OnThisDayEvent>,
    },
    WordOfDay {
        word: String,
        definition: String,
    },
    RssHeadline {
        feed_title: String,
        items: Vec<RssItem>,
    },
    Astronomy {
        lines: Vec<String>,
    },
    /// Fetch attempt failed — keep the most recent error so the
    /// renderer can show "offline / stale" without losing whatever is
    /// already in `payload` from a prior good fetch (handled
    /// out-of-band; this variant is for the *first* fetch failing
    /// before any good payload exists).
    Error {
        message: String,
    },
}
