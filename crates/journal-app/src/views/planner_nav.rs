//! Planner navigation: ensure year + month/week section wrappers exist for a
//! given date, then ensure each daily/wrapper page exists, and load the
//! resulting "landing" page on the canvas.

use std::cell::Cell;
use std::rc::Rc;

use chrono::{Datelike, Duration, NaiveDate, Utc};
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Calendar, DrawingArea, Label, MenuButton, Orientation, Popover};
use uuid::Uuid;

use journal_core::{
    DailySlot, NotebookId, NotebookKind, NotebookTemplate, Page, PageId, PlannerGrouping,
    PlannerPageAddress, SectionId, TemplateId,
};
use journal_storage::JournalBackend;
use journal_templates::{render_title, TitleContext};

use crate::state::{self, SharedState};

/// Compute the addresses (in render order) for a date under the given
/// notebook template + grouping. `template_index` is just an enumeration
/// within the slot.
///
/// Multi-day slots (e.g. a weekend spread covering both Saturday and
/// Sunday on one page) collapse to a SINGLE shared page record:
/// every weekday in the slot resolves to the same `PlannerPageAddress`,
/// keyed by the EARLIEST weekday's date in the same calendar week.
/// Navigating to either day lands on that one page.
fn addresses_for_date(template: &NotebookTemplate, date: NaiveDate) -> Vec<PlannerPageAddress> {
    let mut out = Vec::new();
    if let Some(slot) = matching_slot(template, date) {
        let key_date = canonical_slot_date(slot, date);
        for (i, _tid) in slot.templates.iter().enumerate() {
            out.push(PlannerPageAddress::Day {
                date: key_date,
                template_index: i as u32,
            });
        }
    }
    out
}

/// For a multi-day slot, return the date of the earliest weekday in
/// `slot.days` that falls within the same ISO week as `date`. So when a
/// slot covers `[Sat, Sun]` and the user navigates to Sunday, this
/// returns the Saturday of the same week. Single-day slots return
/// `date` unchanged.
fn canonical_slot_date(slot: &DailySlot, date: NaiveDate) -> NaiveDate {
    if slot.days.len() <= 1 {
        return date;
    }
    let mon0 = date.weekday().num_days_from_monday() as i64;
    let week_start = date - chrono::Duration::days(mon0); // Monday of `date`'s week
                                                          // Earliest weekday (by Monday-relative index) wins.
    let mut best: Option<NaiveDate> = None;
    for wd in &slot.days {
        let off = wd.num_days_from_monday() as i64;
        let candidate = week_start + chrono::Duration::days(off);
        match best {
            None => best = Some(candidate),
            Some(prev) if candidate < prev => best = Some(candidate),
            _ => {}
        }
    }
    best.unwrap_or(date)
}

fn matching_slot(template: &NotebookTemplate, date: NaiveDate) -> Option<&DailySlot> {
    let wd = date.weekday();
    template.daily_slots.iter().find(|s| s.days.contains(&wd))
}

fn template_for_address(
    template: &NotebookTemplate,
    address: &PlannerPageAddress,
) -> Option<TemplateId> {
    match address {
        PlannerPageAddress::Day {
            date,
            template_index,
        } => {
            let slot = matching_slot(template, *date)?;
            slot.templates.get(*template_index as usize).copied()
        }
        _ => None,
    }
}

/// Ensure pages exist in `section_id` for each `(address, template_id, title)`
/// triple, in the given order. Returns the resolved pages in the same order.
///
/// Newly-inserted Day-addressed pages are slotted into chronological order
/// within the section — so if the user jumps to a future date and creates a
/// page, then later "today" rolls around, the new today page lands BEFORE
/// the future page rather than appended after it.
pub fn ensure_planner_pages(
    db: &mut dyn JournalBackend,
    section_id: SectionId,
    entries: &[(PlannerPageAddress, Option<TemplateId>, String)],
) -> Vec<Page> {
    let mut out = Vec::with_capacity(entries.len());
    for (addr, tid, title) in entries {
        match db.find_page_by_address(section_id, addr) {
            Ok(Some(page)) => out.push(page),
            Ok(None) => {
                let end_position = db
                    .list_pages(section_id)
                    .map(|v| v.len() as u32)
                    .unwrap_or(0);
                let now = chrono::Utc::now();
                let page = Page {
                    id: PageId(Uuid::new_v4()),
                    section_id,
                    position: end_position,
                    template_id: *tid,
                    planner_address: Some(*addr),
                    created_at: now,
                    modified_at: now,
                    name: title.clone(),
                    widget_overrides: Default::default(),
                    widget_data: Default::default(),
                    flagged: false,
                    bookmark_position: 0,
                };
                if let Err(e) = db.insert_page(&page) {
                    tracing::error!("failed to insert planner page: {}", e);
                    continue;
                }

                if let PlannerPageAddress::Day {
                    date: new_date,
                    template_index,
                } = addr
                {
                    let target = chronological_target_position(
                        db,
                        section_id,
                        *new_date,
                        *template_index,
                        page.id,
                    );
                    if target != end_position {
                        if let Err(e) = db.reorder_page(page.id, target) {
                            tracing::warn!("reorder planner page chronologically: {}", e);
                        }
                    }
                }

                out.push(page);
            }
            Err(e) => tracing::error!("find_page_by_address failed: {}", e),
        }
    }
    out
}

/// Find the earliest `Day`-address date among all pages in `section_id` and
/// (recursively) its descendant sections. Returns `None` for sections that
/// contain no dated pages — they sort to the bottom in chronological reorder.
fn min_day_date_in_section(
    db: &mut dyn JournalBackend,
    section_id: SectionId,
) -> Option<NaiveDate> {
    let mut min_d: Option<NaiveDate> = None;
    if let Ok(pages) = db.list_pages(section_id) {
        for p in &pages {
            if let Some(PlannerPageAddress::Day { date, .. }) = p.planner_address {
                min_d = Some(min_d.map_or(date, |m| m.min(date)));
            }
        }
    }
    if let Ok(children) = db.list_child_sections(section_id) {
        for c in children {
            if let Some(d) = min_day_date_in_section(db, c.id) {
                min_d = Some(min_d.map_or(d, |m| m.min(d)));
            }
        }
    }
    min_d
}

/// Reorder the sibling sections under `parent` (or all root sections of the
/// notebook when `parent` is `None`) so they appear in chronological order
/// by the earliest Day-addressed page they contain. Sections with no dated
/// pages keep relative order at the bottom.
fn reorder_sections_chronologically(
    db: &mut dyn JournalBackend,
    notebook_id: NotebookId,
    parent: Option<SectionId>,
) {
    let siblings = match parent {
        None => db.list_root_sections(notebook_id),
        Some(pid) => db.list_child_sections(pid),
    };
    let siblings = match siblings {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("list siblings for chrono reorder: {}", e);
            return;
        }
    };

    let mut keyed: Vec<(SectionId, u32, Option<NaiveDate>)> = siblings
        .iter()
        .map(|s| (s.id, s.position, min_day_date_in_section(db, s.id)))
        .collect();
    keyed.sort_by(|a, b| match (a.2, b.2) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.1.cmp(&b.1)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.1.cmp(&b.1),
    });

    for (i, (id, _, _)) in keyed.iter().enumerate() {
        if let Err(e) = db.reorder_section(*id, i as u32) {
            tracing::warn!("reorder_section {:?}: {}", id, e);
        }
    }
}

/// Compute the position the new Day-addressed page should sit at within
/// `section_id` so that all Day pages stay in (date, template_index) order.
/// Non-Day pages keep their relative order at the front.
fn chronological_target_position(
    db: &mut dyn JournalBackend,
    section_id: SectionId,
    new_date: NaiveDate,
    new_template_index: u32,
    new_page_id: PageId,
) -> u32 {
    let pages = match db.list_pages(section_id) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("list_pages for chronological insert: {}", e);
            return u32::MAX;
        }
    };
    // Pages are returned in `position` order. Walk in order and find the
    // index of the first existing Day page whose (date, idx) sorts AFTER us.
    // Skip ourselves.
    let mut idx: u32 = 0;
    for p in &pages {
        if p.id == new_page_id {
            continue;
        }
        match p.planner_address {
            Some(PlannerPageAddress::Day {
                date,
                template_index,
            }) if (date, template_index) > (new_date, new_template_index) => {
                return idx;
            }
            _ => {
                // Non-Day pages (e.g. before-month wrappers if any) keep
                // their leading slot — our new Day page goes after them.
            }
        }
        idx += 1;
    }
    idx
}

/// Navigate the planner notebook to `date`: build any missing year/month/week
/// section wrappers, ensure daily pages exist under them, then load the first
/// daily page on the canvas. Returns the page id we landed on (if any).
pub fn goto_date(
    state: &SharedState,
    canvas: &DrawingArea,
    notebook_id: journal_core::NotebookId,
    template: &NotebookTemplate,
    date: NaiveDate,
) -> Option<PageId> {
    let backend_rc = state.borrow().backend.clone();

    let year_title = render_title(
        &template.section_title_formats.year,
        &TitleContext::new(date),
    );
    let year_section = match backend_rc
        .borrow_mut()
        .ensure_section(notebook_id, None, &year_title)
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to ensure year section: {}", e);
            return None;
        }
    };

    let wrapper_title = match template.grouping {
        PlannerGrouping::Month => render_title(
            &template.section_title_formats.month,
            &TitleContext::new(date),
        ),
        PlannerGrouping::Week => render_title(
            &template.section_title_formats.week,
            &TitleContext::new(date),
        ),
    };
    let wrapper_section = match backend_rc.borrow_mut().ensure_section(
        notebook_id,
        Some(year_section.id),
        &wrapper_title,
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to ensure wrapper section: {}", e);
            return None;
        }
    };

    let addresses = addresses_for_date(template, date);
    let entries: Vec<_> = addresses
        .into_iter()
        .map(|addr| {
            let title = render_title(&template.page_title_format, &TitleContext::new(date));
            (addr, template_for_address(template, &addr), title)
        })
        .collect();

    let pages = {
        let mut b = backend_rc.borrow_mut();
        let pages = ensure_planner_pages(&mut *b, wrapper_section.id, &entries);
        // Keep year + wrapper sections sorted chronologically every time
        // we land — ensure_section appends to the end.
        reorder_sections_chronologically(&mut *b, notebook_id, None);
        reorder_sections_chronologically(&mut *b, notebook_id, Some(year_section.id));
        pages
    };

    // Land on first daily page; fall back to wrapper section's first existing
    // page if no daily pages were generated.
    let landing = pages.into_iter().next().or_else(|| {
        backend_rc
            .borrow_mut()
            .list_pages(wrapper_section.id)
            .ok()
            .and_then(|v| v.into_iter().next())
    });

    if let Some(page) = landing {
        let template_for_canvas = page
            .template_id
            .and_then(|tid| state.borrow().templates.borrow().get(tid).cloned());
        state::set_current_template(state, template_for_canvas);
        state::set_current_page(state, page.id);
        canvas.queue_draw();
        return Some(page.id);
    }
    None
}

fn fmt_date(d: NaiveDate) -> String {
    d.format("%a, %b %-d, %Y").to_string()
}

/// Compute what fraction of the year [0.0, 1.0) a given date represents.
/// Uses day_of_year - 1 so Jan 1 == 0.0, Dec 31 ≈ 1.0.
fn year_fraction(d: NaiveDate) -> f64 {
    use chrono::Datelike;
    let days_in_year = if d.leap_year() { 366.0 } else { 365.0 };
    (d.ordinal() as f64 - 1.0) / days_in_year
}

/// Draw the year-progress bar: dim background rounded rect, accent foreground,
/// 12 month-tick lines, and optionally highlight where `frac` sits. Adapts to
/// dark mode so the indigo fill stays visible against a dark window.
fn draw_year_bar(ctx: &gtk4::cairo::Context, w: f64, h: f64, frac: f64) {
    let r = h / 2.0;
    let dark = crate::is_dark_mode();

    // Dim background track.
    if dark {
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.12);
    } else {
        ctx.set_source_rgba(0.5, 0.5, 0.5, 0.15);
    }
    rounded_rect(ctx, 0.0, 0.0, w, h, r);
    let _ = ctx.fill();

    // Filled progress: indigo on light, brighter periwinkle on dark.
    let progress_w = (w * frac).max(0.0).min(w);
    if progress_w > 0.0 {
        if dark {
            ctx.set_source_rgba(0.62, 0.66, 0.92, 0.95);
        } else {
            ctx.set_source_rgba(0.227, 0.239, 0.431, 0.85);
        }
        rounded_rect(ctx, 0.0, 0.0, progress_w, h, r);
        let _ = ctx.fill();
    }

    // Month tick marks at each 1/12 boundary.
    if dark {
        ctx.set_source_rgba(0.0, 0.0, 0.0, 0.35);
    } else {
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.25);
    }
    ctx.set_line_width(0.75);
    for m in 1..12 {
        let x = w * (m as f64 / 12.0);
        ctx.move_to(x, 0.0);
        ctx.line_to(x, h);
        let _ = ctx.stroke();
    }
}

fn rounded_rect(ctx: &gtk4::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    ctx.new_sub_path();
    ctx.arc(
        x + r,
        y + r,
        r,
        std::f64::consts::PI,
        3.0 * std::f64::consts::PI / 2.0,
    );
    ctx.arc(x + w - r, y + r, r, 3.0 * std::f64::consts::PI / 2.0, 0.0);
    ctx.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::PI / 2.0);
    ctx.arc(
        x + r,
        y + h - r,
        r,
        std::f64::consts::PI / 2.0,
        std::f64::consts::PI,
    );
    ctx.close_path();
}

/// Build the planner navigation strip: Prev / Today / [date label menu] / Next,
/// with a thin year-progress bar below the buttons.
/// Returns the outer container widget. Auto-loads today's page on construction.
pub fn build_nav_strip(
    state: SharedState,
    canvas: DrawingArea,
    notebook_id: NotebookId,
    template: NotebookTemplate,
    on_refresh: Rc<dyn Fn()>,
) -> GtkBox {
    let outer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(4)
        .margin_bottom(4)
        .margin_start(8)
        .margin_end(8)
        .build();

    let strip = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();

    let current: Rc<Cell<NaiveDate>> = Rc::new(Cell::new(Utc::now().date_naive()));
    // Set true while we update the GtkCalendar programmatically so the
    // intermediate set_year/set_month/set_day don't fire day_selected with
    // a half-applied date (which clobbers `current` and prevents month
    // rollover at end-of-month / end-of-year boundaries).
    let suppress_day_selected: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    let prev_btn = Button::from_icon_name("go-previous-symbolic");
    prev_btn.set_tooltip_text(Some("Previous day"));
    let today_btn = Button::with_label("Today");
    let next_btn = Button::from_icon_name("go-next-symbolic");
    next_btn.set_tooltip_text(Some("Next day"));

    let date_btn = MenuButton::new();
    let date_label = Label::new(Some(&fmt_date(current.get())));
    date_btn.set_child(Some(&date_label));

    let popover = Popover::new();
    let cal = Calendar::new();
    cal.set_year(current.get().year());
    cal.set_month(current.get().month0() as i32);
    cal.set_day(current.get().day() as i32);
    popover.set_child(Some(&cal));
    date_btn.set_popover(Some(&popover));

    strip.append(&prev_btn);
    strip.append(&today_btn);
    strip.append(&date_btn);
    strip.append(&next_btn);

    // Year-progress bar — redraws whenever `current` changes.
    let progress_bar = DrawingArea::new();
    progress_bar.set_height_request(6);
    progress_bar.set_hexpand(true);
    progress_bar.set_margin_top(2);
    {
        let current = current.clone();
        progress_bar.set_draw_func(move |_area, ctx, w, h| {
            if w <= 0 || h <= 0 {
                return;
            }
            let frac = year_fraction(current.get());
            draw_year_bar(ctx, w as f64, h as f64, frac);
        });
    }
    // Click on the bar → jump to that date in the year.
    {
        let current_for_click = current.clone();
        let progress_for_click = progress_bar.clone();
        let state_for_click = state.clone();
        let canvas_for_click = canvas.clone();
        let template_for_click = template.clone();
        let on_refresh_for_click = on_refresh.clone();
        let date_label_for_click = date_label.clone();
        let cal_for_click = cal.clone();
        let suppress_for_click = suppress_day_selected.clone();
        let click = gtk4::GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        click.connect_pressed(move |_g, _n, x, _y| {
            let width = progress_for_click.width() as f64;
            if width <= 0.0 {
                return;
            }
            let frac = (x / width).clamp(0.0, 1.0 - 1e-9);
            let year = current_for_click.get().year();
            let is_leap = chrono::NaiveDate::from_ymd_opt(year, 2, 29).is_some();
            let days_in_year = if is_leap { 366.0 } else { 365.0 };
            let ordinal = (frac * days_in_year) as u32 + 1;
            let d = chrono::NaiveDate::from_yo_opt(year, ordinal.clamp(1, days_in_year as u32));
            if let Some(d) = d {
                current_for_click.set(d);
                date_label_for_click.set_text(&fmt_date(d));
                suppress_for_click.set(true);
                cal_for_click.set_year(d.year());
                cal_for_click.set_month(d.month0() as i32);
                cal_for_click.set_day(d.day() as i32);
                suppress_for_click.set(false);
                progress_for_click.queue_draw();
                goto_date(
                    &state_for_click,
                    &canvas_for_click,
                    notebook_id,
                    &template_for_click,
                    d,
                );
                (on_refresh_for_click)();
            }
        });
        progress_bar.add_controller(click);
    }

    outer.append(&strip);
    outer.append(&progress_bar);

    let nav = |state: &SharedState,
               canvas: &DrawingArea,
               template: &NotebookTemplate,
               current: &Rc<Cell<NaiveDate>>,
               label: &Label,
               cal: &Calendar,
               bar: &DrawingArea,
               date: NaiveDate,
               notebook_id: NotebookId,
               on_refresh: &Rc<dyn Fn()>,
               suppress: &Rc<Cell<bool>>| {
        current.set(date);
        label.set_text(&fmt_date(date));
        // Guard against day-selected reentrancy: GtkCalendar fires that
        // signal on each of set_year/set_month/set_day, and the
        // intermediate states (e.g. day=31 while month=Feb) cause the
        // handler to navigate to the wrong date.
        suppress.set(true);
        cal.set_year(date.year());
        cal.set_month(date.month0() as i32);
        cal.set_day(date.day() as i32);
        suppress.set(false);
        bar.queue_draw();
        goto_date(state, canvas, notebook_id, template, date);
        (on_refresh)();
    };

    {
        let state = state.clone();
        let canvas = canvas.clone();
        let template = template.clone();
        let current = current.clone();
        let label = date_label.clone();
        let cal = cal.clone();
        let bar = progress_bar.clone();
        let on_refresh_clone = on_refresh.clone();
        let suppress = suppress_day_selected.clone();
        prev_btn.connect_clicked(move |_| {
            let d = current.get() - Duration::days(1);
            nav(
                &state,
                &canvas,
                &template,
                &current,
                &label,
                &cal,
                &bar,
                d,
                notebook_id,
                &on_refresh_clone,
                &suppress,
            );
        });
    }
    {
        let state = state.clone();
        let canvas = canvas.clone();
        let template = template.clone();
        let current = current.clone();
        let label = date_label.clone();
        let cal = cal.clone();
        let bar = progress_bar.clone();
        let on_refresh_clone = on_refresh.clone();
        let suppress = suppress_day_selected.clone();
        next_btn.connect_clicked(move |_| {
            let d = current.get() + Duration::days(1);
            nav(
                &state,
                &canvas,
                &template,
                &current,
                &label,
                &cal,
                &bar,
                d,
                notebook_id,
                &on_refresh_clone,
                &suppress,
            );
        });
    }
    {
        let state = state.clone();
        let canvas = canvas.clone();
        let template = template.clone();
        let current = current.clone();
        let label = date_label.clone();
        let cal_clone = cal.clone();
        let bar = progress_bar.clone();
        let on_refresh_clone = on_refresh.clone();
        let suppress = suppress_day_selected.clone();
        today_btn.connect_clicked(move |_| {
            let d = Utc::now().date_naive();
            nav(
                &state,
                &canvas,
                &template,
                &current,
                &label,
                &cal_clone,
                &bar,
                d,
                notebook_id,
                &on_refresh_clone,
                &suppress,
            );
        });
    }
    {
        let state = state.clone();
        let canvas = canvas.clone();
        let template = template.clone();
        let current = current.clone();
        let label = date_label.clone();
        let popover = popover.clone();
        let bar = progress_bar.clone();
        let on_refresh_clone = on_refresh.clone();
        let suppress = suppress_day_selected.clone();
        cal.connect_day_selected(move |c| {
            // Ignore the volley of day_selected signals fired while we
            // programmatically reposition the calendar from a button click.
            if suppress.get() {
                return;
            }
            let d = NaiveDate::from_ymd_opt(c.year(), (c.month() + 1) as u32, c.day() as u32);
            if let Some(d) = d {
                current.set(d);
                label.set_text(&fmt_date(d));
                bar.queue_draw();
                goto_date(&state, &canvas, notebook_id, &template, d);
                (on_refresh_clone)();
                popover.popdown();
            }
        });
    }

    // Install a "sync date" callback so that clicking a planner page in the
    // sidebar updates this strip's notion of "current date" — making prev /
    // next walk from the clicked page instead of from today.
    {
        let current = current.clone();
        let label = date_label.clone();
        let cal = cal.clone();
        let bar = progress_bar.clone();
        let suppress = suppress_day_selected.clone();
        let sync: Rc<dyn Fn(NaiveDate)> = Rc::new(move |d: NaiveDate| {
            if current.get() == d {
                return;
            }
            current.set(d);
            label.set_text(&fmt_date(d));
            bar.queue_draw();
            suppress.set(true);
            cal.set_year(d.year());
            cal.set_month(d.month0() as i32);
            cal.set_day(d.day() as i32);
            suppress.set(false);
        });
        state.borrow_mut().planner_nav_sync_date = Some(sync);
    }

    // Auto-load today on open.
    goto_date(&state, &canvas, notebook_id, &template, current.get());
    (on_refresh)();

    outer
}

/// Resolve the active `NotebookTemplate` for a planner notebook from the
/// per-app registry, falling back to None for non-planner notebooks.
pub fn resolve_planner_template(
    state: &SharedState,
    notebook_id: journal_core::NotebookId,
) -> Option<NotebookTemplate> {
    let s = state.borrow();
    let nb = s.backend.borrow_mut().get_notebook(notebook_id).ok()?;
    match nb.kind {
        NotebookKind::Planner { template_id, .. } => {
            s.notebook_templates.borrow().get(template_id).cloned()
        }
        NotebookKind::Standard => None,
    }
}
