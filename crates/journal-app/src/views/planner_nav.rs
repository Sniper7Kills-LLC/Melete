//! Planner navigation: ensure year + month/week section wrappers exist for a
//! given date, then ensure each daily/wrapper page exists, and load the
//! resulting "landing" page on the canvas.

use std::cell::Cell;
use std::rc::Rc;

use chrono::{Datelike, Duration, NaiveDate, Utc};
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Calendar, DrawingArea, Label, MenuButton, Orientation, Popover,
};
use uuid::Uuid;

use journal_core::{
    DailySlot, NotebookId, NotebookKind, NotebookTemplate, Page, PageId, PlannerGrouping,
    PlannerPageAddress, SectionId, TemplateId,
};
use journal_storage::{page_store, section_store, Db};
use journal_templates::{render_title, TitleContext};

use crate::state::{self, SharedState};

/// Compute the addresses (in render order) for a date under the given
/// notebook template + grouping. `template_index` is just an enumeration
/// within the slot.
fn addresses_for_date(template: &NotebookTemplate, date: NaiveDate) -> Vec<PlannerPageAddress> {
    let mut out = Vec::new();
    if let Some(slot) = matching_slot(template, date) {
        for (i, _tid) in slot.templates.iter().enumerate() {
            out.push(PlannerPageAddress::Day {
                date,
                template_index: i as u32,
            });
        }
    }
    out
}

fn matching_slot<'a>(template: &'a NotebookTemplate, date: NaiveDate) -> Option<&'a DailySlot> {
    let wd = date.weekday();
    template
        .daily_slots
        .iter()
        .find(|s| s.days.iter().any(|d| *d == wd))
}

fn template_for_address<'a>(
    template: &'a NotebookTemplate,
    address: &PlannerPageAddress,
) -> Option<TemplateId> {
    match address {
        PlannerPageAddress::Day { date, template_index } => {
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
    db: &mut Db,
    section_id: SectionId,
    entries: &[(PlannerPageAddress, Option<TemplateId>, String)],
) -> Vec<Page> {
    let mut out = Vec::with_capacity(entries.len());
    for (addr, tid, title) in entries {
        match page_store::find_page_by_address(db.conn(), section_id, addr) {
            Ok(Some(page)) => out.push(page),
            Ok(None) => {
                // Insert at end first; we'll reorder to the correct
                // chronological slot below for Day-addressed pages.
                let end_position = page_store::list_pages(db.conn(), section_id)
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
                };
                if let Err(e) = page_store::insert_page(db.conn(), &page) {
                    tracing::error!("failed to insert planner page: {}", e);
                    continue;
                }

                if let PlannerPageAddress::Day { date: new_date, template_index } = addr {
                    let target = chronological_target_position(
                        db,
                        section_id,
                        *new_date,
                        *template_index,
                        page.id,
                    );
                    if target != end_position {
                        if let Err(e) =
                            page_store::reorder_page(db.conn_mut(), page.id, target)
                        {
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

/// Compute the position the new Day-addressed page should sit at within
/// `section_id` so that all Day pages stay in (date, template_index) order.
/// Non-Day pages keep their relative order at the front.
fn chronological_target_position(
    db: &Db,
    section_id: SectionId,
    new_date: NaiveDate,
    new_template_index: u32,
    new_page_id: PageId,
) -> u32 {
    let pages = match page_store::list_pages(db.conn(), section_id) {
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
            Some(PlannerPageAddress::Day { date, template_index }) => {
                if (date, template_index) > (new_date, new_template_index) {
                    return idx;
                }
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
    let db_rc = state.borrow().db.clone();
    let mut db = db_rc.borrow_mut();

    let year_title = render_title(
        &template.section_title_formats.year,
        &TitleContext::new(date),
    );
    let year_section = match section_store::ensure_section(
        db.conn_mut(),
        notebook_id,
        None,
        &year_title,
    ) {
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
    let wrapper_section = match section_store::ensure_section(
        db.conn_mut(),
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

    let pages = ensure_planner_pages(&mut db, wrapper_section.id, &entries);
    drop(db);

    // Land on first daily page; fall back to wrapper section's first existing
    // page if no daily pages were generated.
    let landing = pages.into_iter().next().or_else(|| {
        let db = db_rc.borrow();
        page_store::list_pages(db.conn(), wrapper_section.id)
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

/// Build the planner navigation strip: Prev / Today / [date label menu] / Next.
/// Returns the strip widget. Auto-loads today's page on construction.
pub fn build_nav_strip(
    state: SharedState,
    canvas: DrawingArea,
    notebook_id: NotebookId,
    template: NotebookTemplate,
    on_refresh: Rc<dyn Fn()>,
) -> GtkBox {
    let strip = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(8)
        .margin_end(8)
        .build();

    let current: Rc<Cell<NaiveDate>> = Rc::new(Cell::new(Utc::now().date_naive()));

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

    let nav = |state: &SharedState, canvas: &DrawingArea, template: &NotebookTemplate, current: &Rc<Cell<NaiveDate>>, label: &Label, cal: &Calendar, date: NaiveDate, notebook_id: NotebookId, on_refresh: &Rc<dyn Fn()>| {
        current.set(date);
        label.set_text(&fmt_date(date));
        cal.set_year(date.year());
        cal.set_month(date.month0() as i32);
        cal.set_day(date.day() as i32);
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
        let on_refresh_clone = on_refresh.clone();
        prev_btn.connect_clicked(move |_| {
            let d = current.get() - Duration::days(1);
            nav(&state, &canvas, &template, &current, &label, &cal, d, notebook_id, &on_refresh_clone);
        });
    }
    {
        let state = state.clone();
        let canvas = canvas.clone();
        let template = template.clone();
        let current = current.clone();
        let label = date_label.clone();
        let cal = cal.clone();
        let on_refresh_clone = on_refresh.clone();
        next_btn.connect_clicked(move |_| {
            let d = current.get() + Duration::days(1);
            nav(&state, &canvas, &template, &current, &label, &cal, d, notebook_id, &on_refresh_clone);
        });
    }
    {
        let state = state.clone();
        let canvas = canvas.clone();
        let template = template.clone();
        let current = current.clone();
        let label = date_label.clone();
        let cal_clone = cal.clone();
        let on_refresh_clone = on_refresh.clone();
        today_btn.connect_clicked(move |_| {
            let d = Utc::now().date_naive();
            nav(&state, &canvas, &template, &current, &label, &cal_clone, d, notebook_id, &on_refresh_clone);
        });
    }
    {
        let state = state.clone();
        let canvas = canvas.clone();
        let template = template.clone();
        let current = current.clone();
        let label = date_label.clone();
        let popover = popover.clone();
        let on_refresh_clone = on_refresh.clone();
        cal.connect_day_selected(move |c| {
            let d = NaiveDate::from_ymd_opt(c.year(), (c.month() + 1) as u32, c.day() as u32);
            if let Some(d) = d {
                current.set(d);
                label.set_text(&fmt_date(d));
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
        let sync: Rc<dyn Fn(NaiveDate)> = Rc::new(move |d: NaiveDate| {
            if current.get() == d {
                return;
            }
            current.set(d);
            label.set_text(&fmt_date(d));
            cal.set_year(d.year());
            cal.set_month(d.month0() as i32);
            cal.set_day(d.day() as i32);
        });
        state.borrow_mut().planner_nav_sync_date = Some(sync);
    }

    // Auto-load today on open.
    goto_date(&state, &canvas, notebook_id, &template, current.get());
    (on_refresh)();

    strip
}

/// Resolve the active `NotebookTemplate` for a planner notebook from the
/// per-app registry, falling back to None for non-planner notebooks.
pub fn resolve_planner_template(
    state: &SharedState,
    notebook_id: journal_core::NotebookId,
) -> Option<NotebookTemplate> {
    let s = state.borrow();
    let db = s.db.borrow();
    let nb = journal_storage::notebook_store::get_notebook(db.conn(), notebook_id).ok()?;
    drop(db);
    match nb.kind {
        NotebookKind::Planner { template_id, .. } => {
            s.notebook_templates.borrow().get(template_id).cloned()
        }
        NotebookKind::Standard => None,
    }
}
