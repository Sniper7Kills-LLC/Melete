//! Full-screen drag-and-drop notebook template editor.
//!
//! Mirrors the pattern established by `template_creator::build_editor_view`:
//! a `GtkBox` root is returned, placed into the app `Stack` under
//! `NOTEBOOK_TEMPLATE_EDITOR_NAME`, and closed via an `on_done` callback.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use std::collections::HashSet as StdHashSet;

use chrono::Weekday;
use gtk4::gdk::DragAction;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box as GtkBox, Button, DrawingArea, DropDown, DropTarget, Entry,
    Frame, Label, Orientation, Paned, PolicyType, ScrolledWindow, Separator, StringList, Switch,
    ToggleButton,
};
use journal_canvas::{paint_with_widgets, ViewportTransform};
use journal_core::{
    DailySlot, NotebookTemplate, PageTemplate, PlannerGrouping, Point, Rect, SectionTitleFormats,
    TemplateId, Viewport,
};
use uuid::Uuid;

use crate::state::SharedState;

// ─── Bottom layout preview ───────────────────────────────────────────────────

/// Opener closure type for clicking a chip in the preview strip.
/// Receives the `PageTemplate` corresponding to the clicked chip; the
/// caller (window.rs) routes the call into the full-screen page-template
/// editor.
pub type OnOpenChipFn = Rc<dyn Fn(PageTemplate)>;

thread_local! {
    /// The active editor's preview body Box. Set on `build_editor_view`,
    /// cleared on Back / Save. Slot-rebuild functions call
    /// `refresh_layout_preview` after every chip add/remove so users can see
    /// what the planner will actually generate.
    static LAYOUT_PREVIEW: RefCell<Option<GtkBox>> = const { RefCell::new(None) };
    /// Active opener closure for clickable preview chips. Set on
    /// `build_editor_view`, cleared on Back / Save. Each chip's click
    /// handler pulls from here.
    static CHIP_OPENER: RefCell<Option<OnOpenChipFn>> = const { RefCell::new(None) };
}

const MINI_W: i32 = 64;
const MINI_H: i32 = 84;

/// Render a single page template as a small Cairo preview wrapped in a
/// thin Frame, suitable for the bottom-of-editor preview strip. Tooltip
/// is set to the page name so users can identify chips on hover. When a
/// chip-opener closure is registered, the chip becomes clickable and
/// opens the matching page-template editor (audit §3 follow-up).
fn mini_page_preview(t: &PageTemplate) -> Frame {
    let area = DrawingArea::builder()
        .width_request(MINI_W)
        .height_request(MINI_H)
        .build();
    let template = t.clone();
    area.set_draw_func(move |_a, ctx, w, h| {
        if w <= 0 || h <= 0 {
            return;
        }
        let page_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: template.size_mm.0,
            height: template.size_mm.1,
        };
        let margin = 0.92;
        let zoom = ((w as f64 / page_rect.width).min(h as f64 / page_rect.height)) * margin;
        let viewport = Viewport {
            center: Point {
                x: page_rect.x + page_rect.width * 0.5,
                y: page_rect.y + page_rect.height * 0.5,
            },
            zoom,
            rotation: 0.0,
        };
        let transform = ViewportTransform::new(viewport, w as f64, h as f64);
        let bg = journal_templates::page_template_to_background_config(&template);
        let empty: StdHashSet<Uuid> = StdHashSet::new();
        paint_with_widgets(
            ctx, &transform, &bg, page_rect,
            &template.widgets, &[], &empty, false,
        );
    });
    let frame = Frame::builder().build();
    frame.add_css_class("nbtc-preview-chip-frame");
    let tooltip = if CHIP_OPENER.with(|c| c.borrow().is_some()) {
        format!("{}\nClick to open in the page-template editor.", t.name)
    } else {
        t.name.clone()
    };
    frame.set_tooltip_text(Some(&tooltip));
    frame.set_child(Some(&area));

    // Click → page-template editor opener (when registered). Routes via
    // the thread-local CHIP_OPENER so the chip-build path stays a pure
    // (PageTemplate) -> Frame fn.
    let click = gtk4::GestureClick::new();
    let template = t.clone();
    click.connect_released(move |_g, _n, _x, _y| {
        let opener = CHIP_OPENER.with(|c| c.borrow().clone());
        if let Some(open) = opener {
            (open)(template.clone());
        }
    });
    frame.add_controller(click);
    frame.add_css_class("nbtc-preview-chip-clickable");

    frame
}

/// A 32×42 dashed-border placeholder shown when a slot has no pages.
fn mini_empty_placeholder() -> Frame {
    let frame = Frame::builder()
        .width_request(MINI_W)
        .height_request(MINI_H)
        .build();
    frame.add_css_class("nbtc-preview-chip-empty");
    frame.set_tooltip_text(Some("(no pages)"));
    frame
}

/// Append a small inline section label like "Year ×1" to the strip.
/// Title is dim, multiplier is amber. Used between sections in the
/// flat horizontal preview strip.
fn append_section_label(strip: &GtkBox, title: &str, mult: &str) {
    let lbl = Label::builder()
        .halign(Align::Center)
        .valign(Align::Center)
        .use_markup(true)
        .build();
    if mult.is_empty() {
        lbl.set_markup(&format!(
            "<span weight=\"600\">{}</span>",
            glib::markup_escape_text(title)
        ));
    } else {
        lbl.set_markup(&format!(
            "<span weight=\"600\">{}</span> <span foreground=\"#d6a83a\" weight=\"700\">{}</span>",
            glib::markup_escape_text(title),
            glib::markup_escape_text(mult),
        ));
    }
    lbl.add_css_class("nbtc-preview-section-label");
    strip.append(&lbl);
}

/// Repopulate the preview as a single horizontal strip ~80px tall. Each
/// page is a mini Cairo render (no inline name; tooltip carries the
/// page name). Section labels (Year ×1 / Quarter ×4 / …) sit inline-
/// left of their chips. Wrapped in a horizontal-only ScrolledWindow so
/// long lists scroll sideways without growing the editor vertically.
fn refresh_layout_preview(
    es: &Rc<RefCell<EditorState>>,
    page_templates: &Rc<Vec<PageTemplate>>,
) {
    LAYOUT_PREVIEW.with(|cell| {
        let body_opt = cell.borrow().clone();
        let Some(body) = body_opt else { return };
        while let Some(c) = body.first_child() {
            body.remove(&c);
        }
        let s = es.borrow();
        let pts = page_templates.as_ref();

        // ── Status line: one-glance summary of what's in the template ──
        // Audit §3 — gives the editor a left-to-right read order so the
        // user sees what they're working with before scanning the strip.
        let total_pages: usize = s.template.year_start.len()
            + s.template.before_quarter.len()
            + s.template.before_month.len()
            + s.template.before_week.len()
            + s.template.daily_slots.iter().map(|d| d.templates.len()).sum::<usize>();
        let daily_slot_count = s.template.daily_slots.len();
        let grouping = match s.template.grouping {
            PlannerGrouping::Week => "Week",
            _ => "Month",
        };
        let name = if s.template.name.is_empty() {
            "(untitled)".to_string()
        } else {
            s.template.name.clone()
        };
        let summary = format!(
            "<span weight=\"600\">{}</span>  ·  {} page{}, {} daily slot{}  ·  groups by {}",
            glib::markup_escape_text(&name),
            total_pages,
            if total_pages == 1 { "" } else { "s" },
            daily_slot_count,
            if daily_slot_count == 1 { "" } else { "s" },
            grouping,
        );
        let status_lbl = Label::builder()
            .use_markup(true)
            .halign(Align::Start)
            .build();
        status_lbl.set_markup(&summary);
        status_lbl.add_css_class("dim-label");
        body.append(&status_lbl);

        // Append page chips (mini Cairo previews) for the given ids,
        // or a single empty placeholder when ids is empty.
        let append_chips = |strip: &GtkBox, ids: &[TemplateId]| {
            if ids.is_empty() {
                strip.append(&mini_empty_placeholder());
                return;
            }
            for tid in ids {
                if let Some(t) = pts.iter().find(|t| t.id == *tid) {
                    strip.append(&mini_page_preview(t));
                } else {
                    strip.append(&mini_empty_placeholder());
                }
            }
        };

        // Group multiple Day slots so weekend spreads (e.g. Sat+Sun on
        // one page) render once with a combined label. Walk daily_slots
        // in order; first slot covering each weekday "owns" that weekday.
        let mut weekday_owner: [Option<usize>; 7] = [None; 7];
        for (i, slot) in s.template.daily_slots.iter().enumerate() {
            for d in &slot.days {
                let idx = d.num_days_from_monday() as usize;
                if weekday_owner[idx].is_none() {
                    weekday_owner[idx] = Some(i);
                }
            }
        }
        // Collapse adjacent weekdays sharing the same slot — so a
        // [Sat, Sun] slot with one template renders as one "Sat–Sun ×1"
        // chip group instead of two duplicate columns.
        let weekday_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let mut collapsed: Vec<(String, &[TemplateId])> = Vec::new();
        let mut wi = 0;
        while wi < 7 {
            let owner = weekday_owner[wi];
            let mut wj = wi + 1;
            while wj < 7 && weekday_owner[wj] == owner {
                wj += 1;
            }
            let label = if wj - wi == 1 {
                weekday_names[wi].to_string()
            } else {
                format!("{}–{}", weekday_names[wi], weekday_names[wj - 1])
            };
            let ids: &[TemplateId] = match owner {
                Some(idx) => s.template.daily_slots[idx].templates.as_slice(),
                None => &[],
            };
            collapsed.push((label, ids));
            wi = wj;
        }

        // ── Build the flat horizontal strip ────────────────────────────
        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(false)
            .build();
        scroll.set_policy(PolicyType::Automatic, PolicyType::Never);
        scroll.add_css_class("nbtc-preview-scroll");

        let strip = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .halign(Align::Start)
            .valign(Align::Center)
            .build();
        strip.add_css_class("nbtc-preview-strip");

        append_section_label(&strip, "Year", "×1");
        append_chips(&strip, &s.template.year_start);
        strip.append(&Separator::new(Orientation::Vertical));

        append_section_label(&strip, "Quarter", "×4");
        append_chips(&strip, &s.template.before_quarter);
        strip.append(&Separator::new(Orientation::Vertical));

        append_section_label(&strip, "Month", "×3");
        append_chips(&strip, &s.template.before_month);
        strip.append(&Separator::new(Orientation::Vertical));

        append_section_label(&strip, "Week", "×4–5");
        append_chips(&strip, &s.template.before_week);
        strip.append(&Separator::new(Orientation::Vertical));

        for (label, ids) in &collapsed {
            append_section_label(&strip, label, "×1");
            append_chips(&strip, ids);
            strip.append(&Separator::new(Orientation::Vertical));
        }

        scroll.set_child(Some(&strip));
        body.append(&scroll);
    });
}

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

/// Install a per-chip DropTarget on a flat-slot chip so dropping another
/// chip onto it reorders the dragged chip to *this* chip's position
/// (insert-before semantics). Cross-slot drops are rejected.
fn install_flat_chip_reorder_drop(
    chip: &GtkBox,
    slot: FlatSlot,
    target_idx: usize,
    es: &Rc<RefCell<EditorState>>,
    flow: &Rc<GtkBox>,
    page_templates: &Rc<Vec<PageTemplate>>,
    options_panel: &GtkBox,
) {
    let drop = DropTarget::new(gtk4::glib::types::Type::STRING, DragAction::MOVE);
    let es2 = es.clone();
    let flow2 = flow.clone();
    let pts2 = page_templates.clone();
    let opts2 = options_panel.clone();
    let chip_for_css = chip.clone();
    let chip_for_leave = chip.clone();
    drop.connect_enter(move |_, _, _| {
        chip_for_css.add_css_class("drag-target");
        DragAction::MOVE
    });
    drop.connect_leave(move |_| {
        chip_for_leave.remove_css_class("drag-target");
    });
    drop.connect_drop(move |_target, val, _x, _y| {
        let s = match val.get::<String>() {
            Ok(s) => s,
            Err(_) => return false,
        };
        if let Some(rest) = s.strip_prefix("slot-chip:") {
            if let Some((src_kind, src_idx)) = parse_flat_chip_key(rest) {
                if src_kind == slot {
                    es2.borrow_mut().move_within_flat_slot(slot, src_idx, target_idx);
                    rebuild_flat_slot_flow(&flow2, slot, &es2, &pts2, &opts2);
                    return true;
                }
            }
        }
        false
    });
    chip.add_controller(drop);
}

/// Same idea for daily-slot chips. Reorder is within the same daily
/// slot only; cross-slot moves rejected.
fn install_daily_chip_reorder_drop(
    chip: &GtkBox,
    slot_idx: usize,
    target_idx: usize,
    es: &Rc<RefCell<EditorState>>,
    flow: &Rc<GtkBox>,
    page_templates: &Rc<Vec<PageTemplate>>,
    options_panel: &GtkBox,
) {
    let drop = DropTarget::new(gtk4::glib::types::Type::STRING, DragAction::MOVE);
    let es2 = es.clone();
    let flow2 = flow.clone();
    let pts2 = page_templates.clone();
    let opts2 = options_panel.clone();
    let chip_for_css = chip.clone();
    let chip_for_leave = chip.clone();
    drop.connect_enter(move |_, _, _| {
        chip_for_css.add_css_class("drag-target");
        DragAction::MOVE
    });
    drop.connect_leave(move |_| {
        chip_for_leave.remove_css_class("drag-target");
    });
    drop.connect_drop(move |_target, val, _x, _y| {
        let s = match val.get::<String>() {
            Ok(s) => s,
            Err(_) => return false,
        };
        if let Some(rest) = s.strip_prefix("slot-chip:") {
            if let Some((src_slot, src_idx)) = parse_daily_chip_key(rest) {
                if src_slot == slot_idx {
                    es2.borrow_mut().move_within_daily_slot(slot_idx, src_idx, target_idx);
                    rebuild_daily_slot_flow(&flow2, slot_idx, &es2, &pts2, &opts2);
                    return true;
                }
            }
        }
        false
    });
    chip.add_controller(drop);
}

/// Parse a `"<flat_prefix>:N"` chip key into the `FlatSlot` it belongs to
/// and its current index. Returns `None` if the key isn't a flat-slot key.
fn parse_flat_chip_key(rest: &str) -> Option<(FlatSlot, usize)> {
    let (prefix, idx_str) = rest.rsplit_once(':')?;
    let idx: usize = idx_str.parse().ok()?;
    let slot = match prefix {
        "year_start" => FlatSlot::YearStart,
        "before_quarter" => FlatSlot::BeforeQuarter,
        "before_month" => FlatSlot::BeforeMonth,
        "before_week" => FlatSlot::BeforeWeek,
        _ => return None,
    };
    Some((slot, idx))
}

/// Parse a `"daily:S:N"` chip key into the slot index and template index.
fn parse_daily_chip_key(rest: &str) -> Option<(usize, usize)> {
    let inner = rest.strip_prefix("daily:")?;
    let (s_str, n_str) = inner.split_once(':')?;
    Some((s_str.parse().ok()?, n_str.parse().ok()?))
}

/// Map an old slot index to its new position after one item moved from
/// `src` to `dst` (post-insert position). Returns the new index for
/// every other element in the slot.
fn remap_index_after_move(n: usize, src: usize, dst: usize) -> usize {
    if n == src {
        return dst;
    }
    // After remove: indices > src shift down by 1.
    let after_remove = if n > src { n - 1 } else { n };
    // After insert at dst: indices >= dst shift up by 1.
    if after_remove >= dst { after_remove + 1 } else { after_remove }
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

    fn flat_slot_len(&self, slot: FlatSlot) -> usize {
        match slot {
            FlatSlot::YearStart => self.template.year_start.len(),
            FlatSlot::BeforeQuarter => self.template.before_quarter.len(),
            FlatSlot::BeforeMonth => self.template.before_month.len(),
            FlatSlot::BeforeWeek => self.template.before_week.len(),
        }
    }

    fn daily_slot_len(&self, slot_idx: usize) -> usize {
        self.template
            .daily_slots
            .get(slot_idx)
            .map(|ds| ds.templates.len())
            .unwrap_or(0)
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

    /// Move a template within a flat slot from `src_idx` to `dst_idx`
    /// (insertion index BEFORE removal of src). Keeps `entry_options`
    /// keys aligned with the new positions.
    fn move_within_flat_slot(&mut self, slot: FlatSlot, src_idx: usize, dst_idx: usize) {
        let vec = self.flat_slot_mut(slot);
        if src_idx >= vec.len() {
            return;
        }
        let item = vec.remove(src_idx);
        // After remove, target shifts left if dst > src.
        let dst_adj = if dst_idx > src_idx { dst_idx - 1 } else { dst_idx };
        let dst_clamped = dst_adj.min(vec.len());
        vec.insert(dst_clamped, item);

        // Rewrite entry_options keys for this slot's prefix.
        let prefix = slot.prefix();
        let old_map = std::mem::take(&mut self.template.entry_options);
        let mut new_map = HashMap::new();
        for (k, v) in old_map {
            if let Some(rest) = k.strip_prefix(&format!("{}:", prefix)) {
                if let Ok(n) = rest.parse::<usize>() {
                    let new_n = remap_index_after_move(n, src_idx, dst_clamped);
                    new_map.insert(format!("{}:{}", prefix, new_n), v);
                    continue;
                }
            }
            new_map.insert(k, v);
        }
        self.template.entry_options = new_map;
    }

    /// Move a template within a daily slot from `src_idx` to `dst_idx`.
    fn move_within_daily_slot(&mut self, slot_idx: usize, src_idx: usize, dst_idx: usize) {
        let Some(ds) = self.template.daily_slots.get_mut(slot_idx) else { return };
        if src_idx >= ds.templates.len() {
            return;
        }
        let item = ds.templates.remove(src_idx);
        let dst_adj = if dst_idx > src_idx { dst_idx - 1 } else { dst_idx };
        let dst_clamped = dst_adj.min(ds.templates.len());
        ds.templates.insert(dst_clamped, item);

        // Rewrite daily entry_options keys for THIS slot.
        let old_map = std::mem::take(&mut self.template.entry_options);
        let mut new_map = HashMap::new();
        for (k, v) in old_map {
            if let Some(rest) = k.strip_prefix("daily:") {
                let parts: Vec<&str> = rest.splitn(2, ':').collect();
                if parts.len() == 2 {
                    if let (Ok(s), Ok(n)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                        if s == slot_idx {
                            let new_n = remap_index_after_move(n, src_idx, dst_clamped);
                            new_map.insert(key_daily(s, new_n), v);
                            continue;
                        }
                    }
                }
            }
            new_map.insert(k, v);
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
    on_open_chip: Option<OnOpenChipFn>,
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
    let meta_row = build_meta_row(&es, &page_templates);
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

    // ── Bottom layout preview ────────────────────────────────────────────────
    // Shows what the planner will actually generate for a typical month, so
    // users can sanity-check their drag-and-drop work without saving.
    let preview = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(4)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(8)
        .build();
    preview.add_css_class("nbtc-preview");
    let preview_header = Label::builder()
        .label("What this generates")
        .halign(Align::Start)
        .build();
    preview_header.add_css_class("title-4");
    preview.append(&preview_header);
    let preview_body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .build();
    preview.append(&preview_body);
    root.append(&Separator::new(Orientation::Horizontal));
    root.append(&preview);

    LAYOUT_PREVIEW.with(|cell| *cell.borrow_mut() = Some(preview_body.clone()));
    CHIP_OPENER.with(|cell| *cell.borrow_mut() = on_open_chip.clone());
    refresh_layout_preview(&es, &page_templates);

    // ── Back ─────────────────────────────────────────────────────────────────
    {
        let on_done = on_done.clone();
        back_btn.connect_clicked(move |_| {
            LAYOUT_PREVIEW.with(|cell| *cell.borrow_mut() = None);
            CHIP_OPENER.with(|cell| *cell.borrow_mut() = None);
            (on_done)();
        });
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
                move || {
                    LAYOUT_PREVIEW.with(|cell| *cell.borrow_mut() = None);
                    CHIP_OPENER.with(|cell| *cell.borrow_mut() = None);
                    (on_done)();
                },
            );
        });
    }

    root
}

// ─── Meta row ────────────────────────────────────────────────────────────────

fn build_meta_row(
    es: &Rc<RefCell<EditorState>>,
    page_templates: &Rc<Vec<PageTemplate>>,
) -> GtkBox {
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
            let pts = page_templates.clone();
            entry.connect_changed(move |e| {
                es.borrow_mut().template.name = e.text().to_string();
                refresh_layout_preview(&es, &pts);
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
            let pts = page_templates.clone();
            dd.connect_selected_notify(move |d| {
                es.borrow_mut().template.grouping = match d.selected() {
                    1 => PlannerGrouping::Week,
                    _ => PlannerGrouping::Month,
                };
                refresh_layout_preview(&es, &pts);
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

    // Group page templates by category, alphabetical by category then by
    // template name within each group. Empty category folds into
    // "Uncategorized" at the bottom.
    let mut by_cat: std::collections::BTreeMap<String, Vec<&PageTemplate>> =
        std::collections::BTreeMap::new();
    for t in page_templates.iter() {
        let key = if t.category.trim().is_empty() {
            "Uncategorized".to_string()
        } else {
            t.category.trim().to_string()
        };
        by_cat.entry(key).or_default().push(t);
    }
    let mut cats: Vec<String> = by_cat.keys().cloned().collect();
    // Force "Uncategorized" to the very end regardless of alphabetical order.
    cats.sort_by(|a, b| match (a.as_str(), b.as_str()) {
        ("Uncategorized", "Uncategorized") => std::cmp::Ordering::Equal,
        ("Uncategorized", _) => std::cmp::Ordering::Greater,
        (_, "Uncategorized") => std::cmp::Ordering::Less,
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    });
    for cat in cats {
        let header = Label::builder().label(&cat).halign(Align::Start).build();
        header.add_css_class("nbtc-palette-cat");
        inner.append(&header);

        let mut group = by_cat.remove(&cat).unwrap_or_default();
        group.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        for t in group {
            inner.append(&build_palette_chip(t));
        }
    }

    scroll.set_child(Some(&inner));
    scroll
}

fn build_palette_chip(t: &PageTemplate) -> GtkBox {
    let chip = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(1)
        .margin_bottom(1)
        .margin_start(2)
        .margin_end(2)
        .build();
    // Use the dedicated nbtc-palette-chip class instead of the bulky
    // .notebook-card (which adds 130px min-height for the home grid).
    chip.add_css_class("nbtc-palette-chip");

    // Colour swatch — slightly bigger so it reads at a glance, but the
    // overall chip stays compact (~28px tall).
    let swatch_size: i32 = 22;
    let swatch = gtk4::DrawingArea::builder()
        .width_request(swatch_size)
        .height_request(swatch_size)
        .valign(Align::Center)
        .build();
    let bg = t.background.clone();
    swatch.set_draw_func(move |_, ctx, w, h| {
        let (r, g, b) = swatch_color(&bg);
        ctx.set_source_rgb(r, g, b);
        ctx.rectangle(0.0, 0.0, w as f64, h as f64);
        let _ = ctx.fill();
        ctx.set_source_rgba(0.0, 0.0, 0.0, 0.18);
        ctx.set_line_width(1.0);
        ctx.rectangle(0.5, 0.5, (w - 1) as f64, (h - 1) as f64);
        let _ = ctx.stroke();
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
        BT::Isometric { .. } => (0.70, 0.80, 0.78),
        BT::Hexagonal { .. } => (0.78, 0.82, 0.70),
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

    // Drop zone — wraps the chip list in a min-height GtkBox so drops land
    // even when the slot is empty. The DropTarget lives on the zone, not
    // the inner chip container, because GTK4 FlowBox / horizontal Box
    // collapses to 0 height when empty and never receives the drop.
    let drop_zone = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .height_request(48)
        .hexpand(true)
        .build();
    drop_zone.add_css_class("nbtc-drop-zone");
    section.append(&drop_zone);
    let drop_zone_rc = Rc::new(drop_zone.clone());

    // Inner chip container (horizontal, wraps via the GtkBox flow).
    let flow = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    let flow_rc = Rc::new(flow.clone());
    drop_zone.append(&flow);

    // Populate from existing template data.
    rebuild_flat_slot_flow(&flow_rc, slot, es, page_templates, options_panel);

    // Drop target on the zone — accepts even when the chip Box is empty.
    let drop = DropTarget::new(gtk4::glib::types::Type::STRING, DragAction::COPY | DragAction::MOVE);
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
            // Reorder within the same flat slot: dropping on the trailing
            // empty area moves the chip to the END of this slot.
            if let Some(rest) = s.strip_prefix("slot-chip:") {
                if let Some((src_kind, src_idx)) = parse_flat_chip_key(rest) {
                    if src_kind == slot {
                        let dst = es2.borrow().flat_slot_len(slot);
                        es2.borrow_mut().move_within_flat_slot(slot, src_idx, dst);
                        rebuild_flat_slot_flow(&flow2, slot, &es2, &pts2, &opts2);
                        return true;
                    }
                }
            }
            false
        });
    }
    {
        let zone2 = drop_zone_rc.clone();
        drop.connect_enter(move |_, _, _| {
            zone2.add_css_class("drag-target");
            DragAction::COPY
        });
    }
    {
        let zone2 = drop_zone_rc.clone();
        drop.connect_leave(move |_| {
            zone2.remove_css_class("drag-target");
        });
    }
    drop_zone.add_controller(drop);

    section
}

/// Rebuild the contents of a flat-slot chip container from the current
/// `EditorState`. Renders an empty-state placeholder when the slot is empty
/// so the drop zone reads as a target.
fn rebuild_flat_slot_flow(
    flow: &Rc<GtkBox>,
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
    if ids.is_empty() {
        let placeholder = Label::builder()
            .label("Drag page templates here")
            .halign(Align::Start)
            .valign(Align::Center)
            .build();
        placeholder.add_css_class("nbtc-empty-hint");
        flow.append(&placeholder);
        crate::notebook_template_creator::refresh_layout_preview(es, page_templates);
        return;
    }
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
            // Per-chip drop target: dropping a slot-chip onto chip N
            // moves the dragged chip to position N (insert-before).
            install_flat_chip_reorder_drop(&chip, slot, n, es, flow, page_templates, options_panel);
            flow.append(&chip);
        }
    }
    crate::notebook_template_creator::refresh_layout_preview(es, page_templates);
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
    // Drop zone wraps the chip Box so empty slots have a visible target.
    let drop_zone = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .height_request(48)
        .hexpand(true)
        .build();
    drop_zone.add_css_class("nbtc-drop-zone");
    let drop_zone_rc = Rc::new(drop_zone.clone());

    let flow = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    let flow_rc = Rc::new(flow.clone());
    drop_zone.append(&flow);

    rebuild_daily_slot_flow(&flow_rc, slot_idx, es, page_templates, options_panel);

    let drop = DropTarget::new(gtk4::glib::types::Type::STRING, DragAction::COPY | DragAction::MOVE);
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
            // Reorder within the same daily slot — drop on trailing empty
            // area moves the chip to the end.
            if let Some(rest) = s.strip_prefix("slot-chip:") {
                if let Some((src_slot, src_idx)) = parse_daily_chip_key(rest) {
                    if src_slot == si {
                        let dst = es2.borrow().daily_slot_len(si);
                        es2.borrow_mut().move_within_daily_slot(si, src_idx, dst);
                        rebuild_daily_slot_flow(&flow2, si, &es2, &pts2, &opts2);
                        return true;
                    }
                }
            }
            false
        });
    }
    {
        let zone2 = drop_zone_rc.clone();
        drop.connect_enter(move |_, _, _| {
            zone2.add_css_class("drag-target");
            DragAction::COPY
        });
    }
    {
        let zone2 = drop_zone_rc.clone();
        drop.connect_leave(move |_| {
            zone2.remove_css_class("drag-target");
        });
    }
    drop_zone.add_controller(drop);

    outer.append(&drop_zone);
    outer
}

/// Rebuild the chip container for a specific daily slot. Renders an empty
/// placeholder when the slot has no templates so the drop zone reads as a
/// target.
fn rebuild_daily_slot_flow(
    flow: &Rc<GtkBox>,
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
    if ids.is_empty() {
        let placeholder = Label::builder()
            .label("Drag page templates here")
            .halign(Align::Start)
            .valign(Align::Center)
            .build();
        placeholder.add_css_class("nbtc-empty-hint");
        flow.append(&placeholder);
        crate::notebook_template_creator::refresh_layout_preview(es, page_templates);
        return;
    }
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
            install_daily_chip_reorder_drop(&chip, slot_idx, n, es, flow, page_templates, options_panel);
            flow.append(&chip);
        }
    }
    crate::notebook_template_creator::refresh_layout_preview(es, page_templates);
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
    chip.add_css_class("nbtc-slot-chip");

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

    // Drag source: lets the user reorder the chip within its slot by
    // dragging it onto another chip (drop-before semantics) or onto the
    // empty trailing area of the same slot's drop zone.
    let drag_src = gtk4::DragSource::new();
    drag_src.set_actions(DragAction::MOVE);
    let payload = format!("slot-chip:{}", chip_key);
    drag_src.connect_prepare(move |_src, _x, _y| {
        let val = payload.clone().to_value();
        Some(gtk4::gdk::ContentProvider::for_value(&val))
    });
    chip.add_controller(drag_src);

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
