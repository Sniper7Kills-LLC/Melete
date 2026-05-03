//! Full-screen drag-and-drop notebook template editor.
//!
//! Mirrors the pattern established by `template_creator::build_editor_view`:
//! a `GtkBox` root is returned, placed into the app `Stack` under
//! `NOTEBOOK_TEMPLATE_EDITOR_NAME`, and closed via an `on_done` callback.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::Weekday;
use gtk4::gdk::DragAction;
use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box as GtkBox, Button, DropDown, DropTarget, Entry, Label,
    Orientation, Paned, ScrolledWindow, Separator, StringList, Switch, ToggleButton,
};
use journal_core::{
    DailySlot, NotebookTemplate, PageTemplate, PlannerGrouping, SectionTitleFormats,
    TemplateId,
};
use uuid::Uuid;

use crate::state::SharedState;

// ─── key string helpers ──────────────────────────────────────────────────────

fn key_year_start(n: usize) -> String {
    format!("year_start:{}", n)
}
fn key_before_quarter(n: usize) -> String {
    format!("before_quarter:{}", n)
}
fn key_before_month(n: usize) -> String {
    format!("before_month:{}", n)
}
fn key_before_week(n: usize) -> String {
    format!("before_week:{}", n)
}
fn key_daily(slot: usize, n: usize) -> String {
    format!("daily:{}:{}", slot, n)
}

// ─── EditorState ─────────────────────────────────────────────────────────────

/// Mutable working state for the notebook template editor.
struct EditorState {
    template: NotebookTemplate,
    /// Currently selected chip (slot_key).
    selected_key: Option<String>,
}

impl EditorState {
    fn new(template: NotebookTemplate) -> Self {
        Self { template, selected_key: None }
    }

    /// Remove a template from a flat slot Vec and renumber `entry_options` keys
    /// so they stay aligned with the new Vec indices.
    fn remove_from_flat_slot(
        &mut self,
        slot: FlatSlot,
        idx: usize,
    ) {
        let vec = self.flat_slot_mut(slot);
        if idx < vec.len() {
            vec.remove(idx);
        }
        // Renumber: shift all keys above `idx` down by 1.
        let prefix = slot.prefix();
        self.renumber_flat_keys(prefix, idx);
    }

    /// Remove a template from a daily slot and renumber keys.
    fn remove_from_daily_slot(&mut self, slot_idx: usize, tmpl_idx: usize) {
        if let Some(ds) = self.template.daily_slots.get_mut(slot_idx) {
            if tmpl_idx < ds.templates.len() {
                ds.templates.remove(tmpl_idx);
            }
        }
        self.renumber_daily_keys(slot_idx, tmpl_idx);
    }

    fn flat_slot_mut(&mut self, slot: FlatSlot) -> &mut Vec<TemplateId> {
        match slot {
            FlatSlot::YearStart => &mut self.template.year_start,
            FlatSlot::BeforeQuarter => &mut self.template.before_quarter,
            FlatSlot::BeforeMonth => &mut self.template.before_month,
            FlatSlot::BeforeWeek => &mut self.template.before_week,
        }
    }

    fn renumber_flat_keys(&mut self, prefix: &str, removed_idx: usize) {
        let old_map = std::mem::take(&mut self.template.entry_options);
        let mut new_map = HashMap::new();
        for (k, v) in old_map {
            if let Some(rest) = k.strip_prefix(&format!("{}:", prefix)) {
                if let Ok(n) = rest.parse::<usize>() {
                    if n < removed_idx {
                        new_map.insert(k, v);
                    } else if n > removed_idx {
                        new_map.insert(format!("{}:{}", prefix, n - 1), v);
                    }
                    // n == removed_idx: drop it
                } else {
                    new_map.insert(k, v);
                }
            } else {
                new_map.insert(k, v);
            }
        }
        self.template.entry_options = new_map;
    }

    fn renumber_daily_keys(&mut self, slot_idx: usize, removed_tmpl: usize) {
        let old_map = std::mem::take(&mut self.template.entry_options);
        let mut new_map = HashMap::new();
        for (k, v) in old_map {
            if let Some(rest) = k.strip_prefix("daily:") {
                let parts: Vec<&str> = rest.splitn(2, ':').collect();
                if parts.len() == 2 {
                    if let (Ok(s), Ok(n)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                        if s != slot_idx {
                            new_map.insert(k, v);
                        } else if n < removed_tmpl {
                            new_map.insert(k, v);
                        } else if n > removed_tmpl {
                            new_map.insert(key_daily(s, n - 1), v);
                        }
                        // n == removed_tmpl: drop it
                        continue;
                    }
                }
            }
            new_map.insert(k, v);
        }
        self.template.entry_options = new_map;
    }
}

// ─── FlatSlot helper ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum FlatSlot {
    YearStart,
    BeforeQuarter,
    BeforeMonth,
    BeforeWeek,
}

impl FlatSlot {
    fn prefix(self) -> &'static str {
        match self {
            FlatSlot::YearStart => "year_start",
            FlatSlot::BeforeQuarter => "before_quarter",
            FlatSlot::BeforeMonth => "before_month",
            FlatSlot::BeforeWeek => "before_week",
        }
    }
    fn label(self) -> &'static str {
        match self {
            FlatSlot::YearStart => "Year start",
            FlatSlot::BeforeQuarter => "Before each quarter",
            FlatSlot::BeforeMonth => "Before each month",
            FlatSlot::BeforeWeek => "Before each week",
        }
    }
    fn make_key(self, n: usize) -> String {
        match self {
            FlatSlot::YearStart => key_year_start(n),
            FlatSlot::BeforeQuarter => key_before_quarter(n),
            FlatSlot::BeforeMonth => key_before_month(n),
            FlatSlot::BeforeWeek => key_before_week(n),
        }
    }
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Build the full-screen notebook template editor view.
///
/// `edit` — `Some(t)` edits an existing template, `None` creates a new one.
/// `on_done` — called when the editor is closed (save or back).
pub fn build_editor_view(
    _parent: &ApplicationWindow,
    state: SharedState,
    edit: Option<NotebookTemplate>,
    on_done: Rc<dyn Fn()>,
) -> GtkBox {
    let page_templates: Vec<PageTemplate> = {
        let s = state.borrow();
        let reg = s.templates.borrow();
        let mut v: Vec<PageTemplate> = reg.list().iter().map(|t| (*t).clone()).collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        v
    };

    let template = edit.unwrap_or_else(|| NotebookTemplate {
        id: TemplateId(Uuid::new_v4()),
        name: String::new(),
        description: String::new(),
        year_start: Vec::new(),
        before_quarter: Vec::new(),
        before_month: Vec::new(),
        before_week: Vec::new(),
        daily_slots: Vec::new(),
        grouping: PlannerGrouping::Month,
        page_title_format: "{weekday} {month_name} {day}".into(),
        section_title_formats: SectionTitleFormats::default(),
        entry_options: HashMap::new(),
    });

    let es = Rc::new(RefCell::new(EditorState::new(template)));
    let page_templates = Rc::new(page_templates);

    // ── Root ─────────────────────────────────────────────────────────────────
    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    // ── Top action row ───────────────────────────────────────────────────────
    let action_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();
    let back_btn = Button::from_icon_name("go-previous-symbolic");
    back_btn.set_tooltip_text(Some("Back (cancel)"));
    let title_lbl = Label::builder()
        .label("Notebook Template Editor")
        .halign(Align::Start)
        .hexpand(true)
        .build();
    title_lbl.add_css_class("title-3");
    let saved_indicator = Label::builder().label("").halign(Align::End).build();
    saved_indicator.add_css_class("dim-label");
    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    action_row.append(&back_btn);
    action_row.append(&title_lbl);
    action_row.append(&saved_indicator);
    action_row.append(&save_btn);
    root.append(&action_row);

    // ── Meta row ─────────────────────────────────────────────────────────────
    let meta_row = build_meta_row(&es);
    root.append(&meta_row);

    root.append(&Separator::new(Orientation::Horizontal));

    // ── Three-pane layout ────────────────────────────────────────────────────
    let options_panel = build_options_panel(&es);
    let slots_pane = build_slots_pane(&es, &page_templates, &options_panel);
    let palette = build_palette(&page_templates);

    let right_scroll = ScrolledWindow::builder()
        .width_request(260)
        .vexpand(true)
        .build();
    right_scroll.set_child(Some(&options_panel));

    let inner_paned = Paned::new(Orientation::Horizontal);
    inner_paned.set_start_child(Some(&slots_pane));
    inner_paned.set_end_child(Some(&right_scroll));
    inner_paned.set_position(600);

    let outer_paned = Paned::new(Orientation::Horizontal);
    outer_paned.set_start_child(Some(&palette));
    outer_paned.set_end_child(Some(&inner_paned));
    outer_paned.set_position(200);

    root.append(&outer_paned);

    // ── Back ─────────────────────────────────────────────────────────────────
    {
        let on_done = on_done.clone();
        back_btn.connect_clicked(move |_| (on_done)());
    }

    // ── Save ─────────────────────────────────────────────────────────────────
    {
        let es = es.clone();
        let state = state.clone();
        let on_done = on_done.clone();
        let indicator = saved_indicator.clone();
        save_btn.connect_clicked(move |_| {
            let t = es.borrow().template.clone();
            crate::dialogs::persist_notebook_template(&t);
            state.borrow().notebook_templates.borrow_mut().insert(t);
            indicator.set_label("Saved \u{2713}");
            let on_done = on_done.clone();
            gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(450),
                move || (on_done)(),
            );
        });
    }

    root
}

// ─── Meta row ────────────────────────────────────────────────────────────────

fn build_meta_row(es: &Rc<RefCell<EditorState>>) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(12)
        .margin_top(4)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();

    // Name
    {
        let col = GtkBox::builder().orientation(Orientation::Vertical).spacing(2).build();
        col.append(&Label::builder().label("Name").halign(Align::Start).build());
        let entry = Entry::builder()
            .placeholder_text("My Planner Template")
            .hexpand(true)
            .text(&es.borrow().template.name)
            .build();
        {
            let es = es.clone();
            entry.connect_changed(move |e| {
                es.borrow_mut().template.name = e.text().to_string();
            });
        }
        col.append(&entry);
        row.append(&col);
    }

    // Description
    {
        let col = GtkBox::builder().orientation(Orientation::Vertical).spacing(2).build();
        col.append(&Label::builder().label("Description").halign(Align::Start).build());
        let entry = Entry::builder()
            .hexpand(true)
            .text(&es.borrow().template.description)
            .build();
        {
            let es = es.clone();
            entry.connect_changed(move |e| {
                es.borrow_mut().template.description = e.text().to_string();
            });
        }
        col.append(&entry);
        row.append(&col);
    }

    // Grouping
    {
        let col = GtkBox::builder().orientation(Orientation::Vertical).spacing(2).build();
        col.append(&Label::builder().label("Group by").halign(Align::Start).build());
        let model = StringList::new(&["Month", "Week"]);
        let sel = match es.borrow().template.grouping {
            PlannerGrouping::Week => 1,
            _ => 0,
        };
        let dd = DropDown::builder().model(&model).selected(sel).build();
        {
            let es = es.clone();
            dd.connect_selected_notify(move |d| {
                es.borrow_mut().template.grouping = match d.selected() {
                    1 => PlannerGrouping::Week,
                    _ => PlannerGrouping::Month,
                };
            });
        }
        col.append(&dd);
        row.append(&col);
    }

    // Page title format
    {
        let col = GtkBox::builder().orientation(Orientation::Vertical).spacing(2).build();
        col.append(&Label::builder().label("Page title format").halign(Align::Start).build());
        let entry = Entry::builder()
            .hexpand(true)
            .text(&es.borrow().template.page_title_format)
            .tooltip_text("vars: {year} {month} {month_name} {week} {day} {weekday} {date}")
            .build();
        {
            let es = es.clone();
            entry.connect_changed(move |e| {
                es.borrow_mut().template.page_title_format = e.text().to_string();
            });
        }
        col.append(&entry);
        row.append(&col);
    }

    // Section title formats
    {
        let col = GtkBox::builder().orientation(Orientation::Vertical).spacing(2).build();
        col.append(&Label::builder().label("Year / Month / Week section titles").halign(Align::Start).build());

        let fmts_row = GtkBox::builder().orientation(Orientation::Horizontal).spacing(4).build();

        let year_e = Entry::builder()
            .placeholder_text("Year")
            .hexpand(true)
            .text(&es.borrow().template.section_title_formats.year)
            .build();
        {
            let es = es.clone();
            year_e.connect_changed(move |e| {
                es.borrow_mut().template.section_title_formats.year = e.text().to_string();
            });
        }
        fmts_row.append(&year_e);

        let month_e = Entry::builder()
            .placeholder_text("Month")
            .hexpand(true)
            .text(&es.borrow().template.section_title_formats.month)
            .build();
        {
            let es = es.clone();
            month_e.connect_changed(move |e| {
                es.borrow_mut().template.section_title_formats.month = e.text().to_string();
            });
        }
        fmts_row.append(&month_e);

        let week_e = Entry::builder()
            .placeholder_text("Week")
            .hexpand(true)
            .text(&es.borrow().template.section_title_formats.week)
            .build();
        {
            let es = es.clone();
            week_e.connect_changed(move |e| {
                es.borrow_mut().template.section_title_formats.week = e.text().to_string();
            });
        }
        fmts_row.append(&week_e);

        col.append(&fmts_row);
        row.append(&col);
    }

    row
}

// ─── Palette (left pane) ──────────────────────────────────────────────────────

fn build_palette(page_templates: &Rc<Vec<PageTemplate>>) -> ScrolledWindow {
    let scroll = ScrolledWindow::builder()
        .width_request(200)
        .vexpand(true)
        .build();

    let inner = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let header = Label::builder()
        .label("Page Templates")
        .halign(Align::Start)
        .build();
    header.add_css_class("title-4");
    inner.append(&header);

    let hint = Label::builder()
        .label("Drag to a slot →")
        .halign(Align::Start)
        .wrap(true)
        .build();
    hint.add_css_class("dim-label");
    inner.append(&hint);

    inner.append(&Separator::new(Orientation::Horizontal));

    for t in page_templates.iter() {
        let chip = build_palette_chip(t);
        inner.append(&chip);
    }

    scroll.set_child(Some(&inner));
    scroll
}

fn build_palette_chip(t: &PageTemplate) -> GtkBox {
    let chip = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .margin_top(2)
        .margin_bottom(2)
        .margin_start(2)
        .margin_end(2)
        .build();
    chip.add_css_class("notebook-card");

    // Colour swatch
    let swatch = gtk4::DrawingArea::builder()
        .width_request(14)
        .height_request(14)
        .valign(Align::Center)
        .build();
    let bg = t.background.clone();
    swatch.set_draw_func(move |_, ctx, _w, _h| {
        // A simple coloured rectangle as a tiny swatch.
        let (r, g, b) = swatch_color(&bg);
        ctx.set_source_rgb(r, g, b);
        ctx.rectangle(0.0, 0.0, 14.0, 14.0);
        let _ = ctx.fill();
    });
    chip.append(&swatch);

    let name_lbl = Label::builder()
        .label(&t.name)
        .halign(Align::Start)
        .hexpand(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    chip.append(&name_lbl);

    // Drag source
    let drag_src = gtk4::DragSource::new();
    drag_src.set_actions(DragAction::COPY);
    let payload = format!("page-template:{}", t.id.0);
    drag_src.connect_prepare(move |_src, _x, _y| {
        let val = payload.clone().to_value();
        Some(gtk4::gdk::ContentProvider::for_value(&val))
    });
    chip.add_controller(drag_src);

    chip
}

fn swatch_color(bg: &journal_core::BackgroundType) -> (f64, f64, f64) {
    use journal_core::BackgroundType as BT;
    match bg {
        BT::Blank => (0.95, 0.95, 0.95),
        BT::Dots { .. } => (0.80, 0.85, 0.95),
        BT::Lines { .. } => (0.75, 0.80, 0.90),
        BT::Grid { .. } => (0.65, 0.75, 0.88),
        BT::Image { .. } => (0.88, 0.78, 0.65),
        BT::Pdf { .. } => (0.75, 0.65, 0.85),
    }
}

// ─── Slots pane (middle) ─────────────────────────────────────────────────────

fn build_slots_pane(
    es: &Rc<RefCell<EditorState>>,
    page_templates: &Rc<Vec<PageTemplate>>,
    options_panel: &GtkBox,
) -> ScrolledWindow {
    let scroll = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();

    let inner = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    // Four flat slots.
    for flat in [
        FlatSlot::YearStart,
        FlatSlot::BeforeQuarter,
        FlatSlot::BeforeMonth,
        FlatSlot::BeforeWeek,
    ] {
        let section = build_flat_slot_section(flat, es, page_templates, options_panel);
        inner.append(&section);
        inner.append(&Separator::new(Orientation::Horizontal));
    }

    // Daily slots header + add button.
    {
        let daily_header_row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .build();
        let daily_lbl = Label::builder()
            .label("Daily slots")
            .halign(Align::Start)
            .hexpand(true)
            .build();
        daily_lbl.add_css_class("title-4");
        let add_slot_btn = Button::with_label("+ Add daily slot");
        daily_header_row.append(&daily_lbl);
        daily_header_row.append(&add_slot_btn);
        inner.append(&daily_header_row);

        let daily_container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .build();

        // Pre-populate existing daily slots.
        {
            let s = es.borrow();
            for (slot_idx, ds) in s.template.daily_slots.iter().enumerate() {
                let slot_widget = build_daily_slot_widget(
                    slot_idx,
                    ds,
                    es,
                    page_templates,
                    options_panel,
                    &daily_container,
                );
                daily_container.append(&slot_widget);
            }
        }

        {
            let es = es.clone();
            let pts = page_templates.clone();
            let opts = options_panel.clone();
            let container = daily_container.clone();
            add_slot_btn.connect_clicked(move |_| {
                let new_slot = DailySlot {
                    days: Vec::new(),
                    templates: Vec::new(),
                };
                let slot_idx = {
                    let mut s = es.borrow_mut();
                    s.template.daily_slots.push(new_slot.clone());
                    s.template.daily_slots.len() - 1
                };
                let widget = build_daily_slot_widget(
                    slot_idx,
                    &new_slot,
                    &es,
                    &pts,
                    &opts,
                    &container,
                );
                container.append(&widget);
            });
        }

        inner.append(&daily_container);
    }

    scroll.set_child(Some(&inner));
    scroll
}

/// Build the section for a flat slot (year_start / before_quarter / before_month / before_week).
fn build_flat_slot_section(
    slot: FlatSlot,
    es: &Rc<RefCell<EditorState>>,
    page_templates: &Rc<Vec<PageTemplate>>,
    options_panel: &GtkBox,
) -> GtkBox {
    let section = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .build();

    let header = Label::builder()
        .label(slot.label())
        .halign(Align::Start)
        .build();
    header.add_css_class("title-4");
    section.append(&header);

    // Flow box of chips.
    let flow = gtk4::FlowBox::builder()
        .orientation(Orientation::Horizontal)
        .selection_mode(gtk4::SelectionMode::None)
        .column_spacing(4)
        .row_spacing(4)
        .min_children_per_line(1)
        .max_children_per_line(8)
        .build();
    let flow_rc = Rc::new(flow.clone());
    section.append(&flow);

    // Populate from existing template data.
    {
        let s = es.borrow();
        let ids = match slot {
            FlatSlot::YearStart => &s.template.year_start,
            FlatSlot::BeforeQuarter => &s.template.before_quarter,
            FlatSlot::BeforeMonth => &s.template.before_month,
            FlatSlot::BeforeWeek => &s.template.before_week,
        };
        for (n, tid) in ids.iter().enumerate() {
            if let Some(pt) = page_templates.iter().find(|t| t.id == *tid) {
                let chip_key = slot.make_key(n);
                let chip = build_slot_chip(
                    &pt.name,
                    &chip_key,
                    es,
                    options_panel,
                    {
                        let es2 = es.clone();
                        let flow2 = flow_rc.clone();
                        let page_templates2 = page_templates.clone();
                        let opts2 = options_panel.clone();
                        let n_captured = n;
                        Box::new(move || {
                            es2.borrow_mut().remove_from_flat_slot(slot, n_captured);
                            rebuild_flat_slot_flow(
                                &flow2, slot, &es2, &page_templates2, &opts2,
                            );
                        })
                    },
                );
                flow.append(&chip);
            }
        }
    }

    // Drop target
    let drop = DropTarget::new(gtk4::glib::types::Type::STRING, DragAction::COPY);
    {
        let es2 = es.clone();
        let flow2 = flow_rc.clone();
        let pts2 = page_templates.clone();
        let opts2 = options_panel.clone();
        drop.connect_drop(move |_target, val, _x, _y| {
            let s = match val.get::<String>() {
                Ok(s) => s,
                Err(_) => return false,
            };
            if let Some(uuid_str) = s.strip_prefix("page-template:") {
                if let Ok(uuid) = uuid_str.parse::<Uuid>() {
                    let tid = TemplateId(uuid);
                    {
                        let mut state = es2.borrow_mut();
                        let vec = state.flat_slot_mut(slot);
                        vec.push(tid);
                    }
                    rebuild_flat_slot_flow(&flow2, slot, &es2, &pts2, &opts2);
                    return true;
                }
            }
            false
        });
    }
    {
        let flow2 = flow_rc.clone();
        drop.connect_enter(move |_, _, _| {
            flow2.add_css_class("drag-target");
            DragAction::COPY
        });
    }
    {
        let flow2 = flow_rc.clone();
        drop.connect_leave(move |_| {
            flow2.remove_css_class("drag-target");
        });
    }
    flow.add_controller(drop);

    section
}

/// Rebuild the contents of a flat-slot FlowBox from the current `EditorState`.
fn rebuild_flat_slot_flow(
    flow: &Rc<gtk4::FlowBox>,
    slot: FlatSlot,
    es: &Rc<RefCell<EditorState>>,
    page_templates: &Rc<Vec<PageTemplate>>,
    options_panel: &GtkBox,
) {
    while let Some(child) = flow.first_child() {
        flow.remove(&child);
    }
    let ids: Vec<TemplateId> = {
        let s = es.borrow();
        match slot {
            FlatSlot::YearStart => s.template.year_start.clone(),
            FlatSlot::BeforeQuarter => s.template.before_quarter.clone(),
            FlatSlot::BeforeMonth => s.template.before_month.clone(),
            FlatSlot::BeforeWeek => s.template.before_week.clone(),
        }
    };
    for (n, tid) in ids.iter().enumerate() {
        if let Some(pt) = page_templates.iter().find(|t| t.id == *tid) {
            let chip_key = slot.make_key(n);
            let chip = build_slot_chip(
                &pt.name,
                &chip_key,
                es,
                options_panel,
                {
                    let es2 = es.clone();
                    let flow2 = flow.clone();
                    let pts2 = page_templates.clone();
                    let opts2 = options_panel.clone();
                    let n_captured = n;
                    Box::new(move || {
                        es2.borrow_mut().remove_from_flat_slot(slot, n_captured);
                        rebuild_flat_slot_flow(&flow2, slot, &es2, &pts2, &opts2);
                    })
                },
            );
            flow.append(&chip);
        }
    }
}

// ─── Daily slot widget ────────────────────────────────────────────────────────

fn build_daily_slot_widget(
    slot_idx: usize,
    ds: &DailySlot,
    es: &Rc<RefCell<EditorState>>,
    page_templates: &Rc<Vec<PageTemplate>>,
    options_panel: &GtkBox,
    daily_container: &GtkBox,
) -> GtkBox {
    let outer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .build();
    outer.add_css_class("notebook-card");

    // Day-of-week toggles + Remove button
    let day_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .build();

    let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let weekdays = [
        Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu,
        Weekday::Fri, Weekday::Sat, Weekday::Sun,
    ];
    for (i, name) in day_names.iter().enumerate() {
        let active = ds.days.contains(&weekdays[i]);
        let tb = ToggleButton::builder().label(*name).active(active).build();
        let wd = weekdays[i];
        let es2 = es.clone();
        let si = slot_idx;
        tb.connect_toggled(move |b| {
            let mut s = es2.borrow_mut();
            if let Some(slot) = s.template.daily_slots.get_mut(si) {
                if b.is_active() {
                    if !slot.days.contains(&wd) {
                        slot.days.push(wd);
                    }
                } else {
                    slot.days.retain(|&d| d != wd);
                }
            }
        });
        day_row.append(&tb);
    }

    // Spacer
    let spacer = GtkBox::builder().hexpand(true).build();
    day_row.append(&spacer);

    // Remove slot button
    let remove_btn = Button::from_icon_name("edit-delete-symbolic");
    remove_btn.set_tooltip_text(Some("Remove this daily slot"));
    remove_btn.add_css_class("destructive-action");
    {
        let es2 = es.clone();
        let outer_w = outer.clone();
        let container = daily_container.clone();
        remove_btn.connect_clicked(move |_| {
            let mut s = es2.borrow_mut();
            if slot_idx < s.template.daily_slots.len() {
                s.template.daily_slots.remove(slot_idx);
                // Also clean up entry_options for this slot.
                let old = std::mem::take(&mut s.template.entry_options);
                s.template.entry_options = old
                    .into_iter()
                    .filter(|(k, _)| {
                        if let Some(rest) = k.strip_prefix("daily:") {
                            if let Some(s_str) = rest.split(':').next() {
                                if let Ok(s_idx) = s_str.parse::<usize>() {
                                    return s_idx != slot_idx;
                                }
                            }
                        }
                        true
                    })
                    .collect();
            }
            container.remove(&outer_w);
        });
    }
    day_row.append(&remove_btn);
    outer.append(&day_row);

    // Template chip drop zone
    let flow = gtk4::FlowBox::builder()
        .orientation(Orientation::Horizontal)
        .selection_mode(gtk4::SelectionMode::None)
        .column_spacing(4)
        .row_spacing(4)
        .min_children_per_line(1)
        .max_children_per_line(8)
        .build();
    let flow_rc = Rc::new(flow.clone());

    // Populate existing templates for this slot.
    {
        let s = es.borrow();
        if let Some(slot_data) = s.template.daily_slots.get(slot_idx) {
            for (n, tid) in slot_data.templates.iter().enumerate() {
                if let Some(pt) = page_templates.iter().find(|t| t.id == *tid) {
                    let chip_key = key_daily(slot_idx, n);
                    let chip = build_slot_chip(
                        &pt.name,
                        &chip_key,
                        es,
                        options_panel,
                        {
                            let es2 = es.clone();
                            let flow2 = flow_rc.clone();
                            let pts2 = page_templates.clone();
                            let opts2 = options_panel.clone();
                            Box::new(move || {
                                es2.borrow_mut().remove_from_daily_slot(slot_idx, n);
                                rebuild_daily_slot_flow(
                                    &flow2, slot_idx, &es2, &pts2, &opts2,
                                );
                            })
                        },
                    );
                    flow.append(&chip);
                }
            }
        }
    }

    // Drop target for daily slot
    let drop = DropTarget::new(gtk4::glib::types::Type::STRING, DragAction::COPY);
    {
        let es2 = es.clone();
        let flow2 = flow_rc.clone();
        let pts2 = page_templates.clone();
        let opts2 = options_panel.clone();
        let si = slot_idx;
        drop.connect_drop(move |_target, val, _x, _y| {
            let s = match val.get::<String>() {
                Ok(s) => s,
                Err(_) => return false,
            };
            if let Some(uuid_str) = s.strip_prefix("page-template:") {
                if let Ok(uuid) = uuid_str.parse::<Uuid>() {
                    let tid = TemplateId(uuid);
                    {
                        let mut state = es2.borrow_mut();
                        if let Some(slot_data) = state.template.daily_slots.get_mut(si) {
                            slot_data.templates.push(tid);
                        }
                    }
                    rebuild_daily_slot_flow(&flow2, si, &es2, &pts2, &opts2);
                    return true;
                }
            }
            false
        });
    }
    {
        let flow2 = flow_rc.clone();
        drop.connect_enter(move |_, _, _| {
            flow2.add_css_class("drag-target");
            DragAction::COPY
        });
    }
    {
        let flow2 = flow_rc.clone();
        drop.connect_leave(move |_| {
            flow2.remove_css_class("drag-target");
        });
    }
    flow.add_controller(drop);

    outer.append(&flow);
    outer
}

/// Rebuild the FlowBox for a specific daily slot.
fn rebuild_daily_slot_flow(
    flow: &Rc<gtk4::FlowBox>,
    slot_idx: usize,
    es: &Rc<RefCell<EditorState>>,
    page_templates: &Rc<Vec<PageTemplate>>,
    options_panel: &GtkBox,
) {
    while let Some(child) = flow.first_child() {
        flow.remove(&child);
    }
    let ids: Vec<TemplateId> = {
        let s = es.borrow();
        s.template.daily_slots
            .get(slot_idx)
            .map(|ds| ds.templates.clone())
            .unwrap_or_default()
    };
    for (n, tid) in ids.iter().enumerate() {
        if let Some(pt) = page_templates.iter().find(|t| t.id == *tid) {
            let chip_key = key_daily(slot_idx, n);
            let chip = build_slot_chip(
                &pt.name,
                &chip_key,
                es,
                options_panel,
                {
                    let es2 = es.clone();
                    let flow2 = flow.clone();
                    let pts2 = page_templates.clone();
                    let opts2 = options_panel.clone();
                    let n_captured = n;
                    Box::new(move || {
                        es2.borrow_mut().remove_from_daily_slot(slot_idx, n_captured);
                        rebuild_daily_slot_flow(&flow2, slot_idx, &es2, &pts2, &opts2);
                    })
                },
            );
            flow.append(&chip);
        }
    }
}

// ─── Slot chip (chip placed in a slot) ───────────────────────────────────────

fn build_slot_chip(
    name: &str,
    chip_key: &str,
    es: &Rc<RefCell<EditorState>>,
    options_panel: &GtkBox,
    on_remove: Box<dyn Fn()>,
) -> GtkBox {
    let chip = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .margin_top(2)
        .margin_bottom(2)
        .margin_start(2)
        .margin_end(2)
        .build();

    let name_btn = Button::with_label(name);
    name_btn.add_css_class("flat");
    {
        let key = chip_key.to_string();
        let es2 = es.clone();
        let opts = options_panel.clone();
        name_btn.connect_clicked(move |_| {
            es2.borrow_mut().selected_key = Some(key.clone());
            refresh_options_panel(&opts, &es2);
        });
    }
    chip.append(&name_btn);

    let remove_btn = Button::from_icon_name("window-close-symbolic");
    remove_btn.add_css_class("flat");
    remove_btn.set_tooltip_text(Some("Remove from slot"));
    remove_btn.connect_clicked(move |_| on_remove());
    chip.append(&remove_btn);

    chip
}

// ─── Options panel (right pane) ───────────────────────────────────────────────

fn build_options_panel(es: &Rc<RefCell<EditorState>>) -> GtkBox {
    let panel = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let placeholder = Label::builder()
        .label("Click a chip to edit its options.")
        .halign(Align::Start)
        .wrap(true)
        .build();
    placeholder.add_css_class("dim-label");
    panel.append(&placeholder);

    // Populate immediately if there's already a selection.
    refresh_options_panel(&panel, es);

    panel
}

fn refresh_options_panel(panel: &GtkBox, es: &Rc<RefCell<EditorState>>) {
    while let Some(child) = panel.first_child() {
        panel.remove(&child);
    }

    let selected_key = es.borrow().selected_key.clone();
    let Some(key) = selected_key else {
        let lbl = Label::builder()
            .label("Click a chip to edit its options.")
            .halign(Align::Start)
            .wrap(true)
            .build();
        lbl.add_css_class("dim-label");
        panel.append(&lbl);
        return;
    };

    // Slot label + template name
    let slot_lbl = Label::builder()
        .label(&format!("Slot: {}", &key))
        .halign(Align::Start)
        .wrap(true)
        .build();
    slot_lbl.add_css_class("title-4");
    panel.append(&slot_lbl);

    panel.append(&Separator::new(Orientation::Horizontal));

    // Bridge to previous period
    {
        let row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .build();
        let lbl = Label::builder()
            .label("Bridge to previous period")
            .halign(Align::Start)
            .hexpand(true)
            .build();
        let sw = Switch::new();
        let cur_val = es.borrow().template.entry_options
            .get(&key)
            .map(|f| f.bridge_previous)
            .unwrap_or(false);
        sw.set_active(cur_val);
        {
            let es2 = es.clone();
            let k = key.clone();
            sw.connect_active_notify(move |s| {
                let mut state = es2.borrow_mut();
                let flags = state.template.entry_options.entry(k.clone()).or_default();
                flags.bridge_previous = s.is_active();
            });
        }
        row.append(&lbl);
        row.append(&sw);
        panel.append(&row);
    }

    // Bridge to next period
    {
        let row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .build();
        let lbl = Label::builder()
            .label("Bridge to next period")
            .halign(Align::Start)
            .hexpand(true)
            .build();
        let sw = Switch::new();
        let cur_val = es.borrow().template.entry_options
            .get(&key)
            .map(|f| f.bridge_next)
            .unwrap_or(false);
        sw.set_active(cur_val);
        {
            let es2 = es.clone();
            let k = key.clone();
            sw.connect_active_notify(move |s| {
                let mut state = es2.borrow_mut();
                let flags = state.template.entry_options.entry(k.clone()).or_default();
                flags.bridge_next = s.is_active();
            });
        }
        row.append(&lbl);
        row.append(&sw);
        panel.append(&row);
    }

    panel.append(&Separator::new(Orientation::Horizontal));

    // Hint
    let hint = Label::builder()
        .label("Bridge flags are persisted but not yet rendered by the planner — they will be wired in a future phase.")
        .halign(Align::Start)
        .wrap(true)
        .build();
    hint.add_css_class("dim-label");
    panel.append(&hint);
}
