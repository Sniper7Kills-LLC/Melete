use std::cell::RefCell;
use std::rc::Rc;

use gtk4::cairo;
use gtk4::gdk::RGBA;
use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, ColorDialog, ColorDialogButton, DrawingArea, Entry,
    Label, MenuButton, Orientation, Paned, Popover, ScrolledWindow, SpinButton, Switch,
};
use journal_canvas::{draw_widgets_with_context, ViewportTransform, WidgetRenderContext};
use journal_core::{
    Color, PageTemplate, Rect, TemplateWidget, WidgetKind, WidgetRect, WidgetStyle,
};
use journal_templates::{serialize_template_toml, template_file_from_page_template};
use uuid::Uuid;

use crate::state::SharedState;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PlaceTool {
    None,
    TextBlock,
    Rectangle,
    Ellipse,
    Line,
    GridRegion,
    LinesRegion,
    DotsRegion,
    CalendarMonth,
    Timeline,
    Checklist,
    BigThree,
    PriorityList,
    DailyAppointments,
    WeeklyCompass,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Handle {
    Move,
    ResizeBottomRight,
}

// ── Template-editor undo/redo history (Feature 2) ────────────────────────────

/// A single undoable/redoable edit in the template editor.
#[derive(Clone, Debug)]
enum EditOp {
    Insert { idx: usize, widget: TemplateWidget },
    Remove { idx: usize, widget: TemplateWidget },
    Move { idx: usize, before_rect: WidgetRect, after_rect: WidgetRect },
    Resize { idx: usize, before_rect: WidgetRect, after_rect: WidgetRect },
}

struct EditorHistory {
    undo: Vec<EditOp>,
    redo: Vec<EditOp>,
}

impl EditorHistory {
    fn new() -> Self {
        Self { undo: Vec::new(), redo: Vec::new() }
    }

    fn push(&mut self, op: EditOp) {
        self.undo.push(op);
        self.redo.clear();
    }

    /// Pop the top undo op (without applying it — caller applies and calls redo_push).
    fn undo_pop(&mut self) -> Option<EditOp> {
        self.undo.pop()
    }

    /// Pop the top redo op (without applying it — caller applies and calls undo_push).
    fn redo_pop(&mut self) -> Option<EditOp> {
        self.redo.pop()
    }

    fn undo_push(&mut self, op: EditOp) {
        self.undo.push(op);
    }

    fn redo_push(&mut self, op: EditOp) {
        self.redo.push(op);
    }
}

/// Apply `op` forward (for redo). Returns the resulting `selected_idx`.
fn apply_op(op: &EditOp, widgets: &mut Vec<TemplateWidget>) -> Option<usize> {
    match op {
        EditOp::Insert { idx, widget } => {
            let i = (*idx).min(widgets.len());
            widgets.insert(i, widget.clone());
            Some(i)
        }
        EditOp::Remove { idx, .. } => {
            let i = *idx;
            if i < widgets.len() { widgets.remove(i); }
            None
        }
        EditOp::Move { idx, after_rect, .. } => {
            if let Some(w) = widgets.get_mut(*idx) { w.rect = after_rect.clone(); }
            Some(*idx)
        }
        EditOp::Resize { idx, after_rect, .. } => {
            if let Some(w) = widgets.get_mut(*idx) { w.rect = after_rect.clone(); }
            Some(*idx)
        }
    }
}

/// Apply the inverse of `op` (for undo). Returns the resulting `selected_idx`.
fn apply_inverse(op: &EditOp, widgets: &mut Vec<TemplateWidget>) -> Option<usize> {
    match op {
        EditOp::Insert { idx, .. } => {
            let i = *idx;
            if i < widgets.len() { widgets.remove(i); }
            None
        }
        EditOp::Remove { idx, widget } => {
            let i = (*idx).min(widgets.len());
            widgets.insert(i, widget.clone());
            Some(i)
        }
        EditOp::Move { idx, before_rect, .. } => {
            if let Some(w) = widgets.get_mut(*idx) { w.rect = before_rect.clone(); }
            Some(*idx)
        }
        EditOp::Resize { idx, before_rect, .. } => {
            if let Some(w) = widgets.get_mut(*idx) { w.rect = before_rect.clone(); }
            Some(*idx)
        }
    }
}

// ── CreatorState ─────────────────────────────────────────────────────────────

struct CreatorState {
    template: PageTemplate,
    selected_idx: Option<usize>,
    tool: PlaceTool,
    drag_start_canvas: Option<(f64, f64)>,
    drag_active: bool,
    drag_handle: Handle,
    drag_orig_rect: Option<WidgetRect>,

    /// Undo/redo stack for template edits (Feature 2).
    history: EditorHistory,

    // ── Snap-to-grid (Feature 3) ──────────────────────────────────────────
    /// When `Some(mm)`, dragged/resized widget coordinates snap to this grid.
    snap_grid_mm: Option<f64>,
    /// Whether smart-guide alignment hints are rendered during a drag.
    smart_guides_active: bool,
}

impl CreatorState {
    fn new(template: PageTemplate) -> Self {
        Self {
            template,
            selected_idx: None,
            tool: PlaceTool::None,
            drag_start_canvas: None,
            drag_active: false,
            drag_handle: Handle::Move,
            drag_orig_rect: None,
            history: EditorHistory::new(),
            snap_grid_mm: None,
            smart_guides_active: true,
        }
    }

    /// Snap a value to the nearest grid multiple (if snap is enabled).
    fn snap(&self, v: f64) -> f64 {
        match self.snap_grid_mm {
            Some(g) if g > 0.0 => (v / g).round() * g,
            _ => v,
        }
    }
}

// ── Selection observer (Feature 4) ───────────────────────────────────────────
//
// Stored separately from `CreatorState` so we can call it after releasing any
// mutable borrow on `cs`.  The observer rebuilds the props panel whenever
// `selected_idx` changes.

type SelectionObserverFn = Rc<dyn Fn(Option<usize>)>;

/// Helper: change `selected_idx` in `cs`, then fire the observer (separately,
/// after the borrow has been dropped).
fn select_widget(
    cs: &Rc<RefCell<CreatorState>>,
    idx: Option<usize>,
    observer: &Option<SelectionObserverFn>,
) {
    cs.borrow_mut().selected_idx = idx;
    if let Some(obs) = observer {
        obs(idx);
    }
}

/// Build the full-screen template editor view (root widget tree).
///
/// The caller is responsible for placing the returned `GtkBox` into the app
/// stack and routing back-navigation through `on_done` (called from both
/// Save and Cancel).
pub fn build_editor_view(
    _parent: &ApplicationWindow,
    state: SharedState,
    edit: Option<PageTemplate>,
    on_done: Rc<dyn Fn()>,
) -> GtkBox {
    let template = edit.unwrap_or_else(PageTemplate::default);
    let cs = Rc::new(RefCell::new(CreatorState::new(template)));

    // ── Selection observer — stored outside `cs` to avoid re-entrant borrows.
    // Initialised as `None`; wired up below after `props_box_rc` exists.
    let sel_obs: Rc<RefCell<Option<SelectionObserverFn>>> = Rc::new(RefCell::new(None));

    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    // ── Top action bar (back / save) ────────────────────────────────────
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
    let title = Label::builder()
        .label("Template Editor")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    title.add_css_class("title-3");
    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    let saved_indicator = Label::builder()
        .label("")
        .halign(gtk4::Align::End)
        .build();
    saved_indicator.add_css_class("dim-label");
    action_row.append(&back_btn);
    action_row.append(&title);
    action_row.append(&saved_indicator);
    action_row.append(&save_btn);
    root.append(&action_row);

    let meta_row = build_meta_row(&cs);
    root.append(&meta_row);

    // ── Snap row ─────────────────────────────────────────────────────────
    let snap_row = build_snap_row(&cs);
    root.append(&snap_row);

    let palette = build_palette(&cs, &sel_obs);
    let canvas_area = build_canvas_area(&cs, &sel_obs);

    let props_scroll = ScrolledWindow::builder()
        .width_request(260)
        .vexpand(true)
        .build();
    let props_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_start(8)
        .margin_end(8)
        .margin_bottom(8)
        .build();
    props_scroll.set_child(Some(&props_box));
    let props_box_rc = Rc::new(props_box);

    // ── Feature 4: wire up the selection observer ─────────────────────────
    // Initial render of props panel.
    refresh_props_panel(&props_box_rc, &cs, &canvas_area);

    {
        let cs2 = cs.clone();
        let props2 = props_box_rc.clone();
        let area2 = canvas_area.clone();
        *sel_obs.borrow_mut() = Some(Rc::new(move |_new_sel: Option<usize>| {
            refresh_props_panel(&props2, &cs2, &area2);
        }));
    }

    let inner_paned = Paned::new(Orientation::Horizontal);
    inner_paned.set_start_child(Some(&canvas_area));
    inner_paned.set_end_child(Some(&props_scroll));
    inner_paned.set_position(720);

    let paned = Paned::new(Orientation::Horizontal);
    paned.set_start_child(Some(&palette));
    paned.set_end_child(Some(&inner_paned));
    paned.set_position(160);

    root.append(&paned);

    {
        let on_done = on_done.clone();
        back_btn.connect_clicked(move |_| (on_done)());
    }

    let do_save = {
        let cs = cs.clone();
        let state = state.clone();
        let on_done = on_done.clone();
        let indicator = saved_indicator.clone();
        Rc::new(move || {
            let t = cs.borrow().template.clone();
            if let Err(e) = save_template(&t, &state) {
                tracing::error!("save template: {:#}", e);
                indicator.set_text("Save failed");
                return;
            }
            indicator.set_text("Saved \u{2713}");
            let on_done = on_done.clone();
            gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(450),
                move || (on_done)(),
            );
        })
    };

    {
        let do_save = do_save.clone();
        save_btn.connect_clicked(move |_| (do_save)());
    }

    // Ctrl+S → save, Ctrl+Z → undo, Ctrl+Shift+Z → redo.
    {
        let key = gtk4::EventControllerKey::new();
        key.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let do_save = do_save.clone();
        let cs2 = cs.clone();
        let area2 = canvas_area.clone();
        let obs2 = sel_obs.clone();
        key.connect_key_pressed(move |_c, keyval, _code, mods| {
            let ctrl = mods.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            let shift = mods.contains(gtk4::gdk::ModifierType::SHIFT_MASK);

            if ctrl && (keyval == gtk4::gdk::Key::s || keyval == gtk4::gdk::Key::S) {
                (do_save)();
                return gtk4::glib::Propagation::Stop;
            }

            // Ctrl+Z → undo
            if ctrl && !shift && (keyval == gtk4::gdk::Key::z || keyval == gtk4::gdk::Key::Z) {
                // Pull out the top undo op, apply its inverse to the widget list.
                let result = {
                    let mut s = cs2.borrow_mut();
                    if let Some(op) = s.history.undo_pop() {
                        let sel = apply_inverse(&op, &mut s.template.widgets);
                        s.history.redo_push(op);
                        Some(sel)
                    } else {
                        None
                    }
                };
                if let Some(new_sel) = result {
                    // select_widget releases cs2 borrow before firing observer,
                    // so the observer can safely borrow cs2 to rebuild props.
                    let obs_clone = obs2.borrow().clone();
                    select_widget(&cs2, new_sel, &obs_clone);
                    area2.queue_draw();
                }
                return gtk4::glib::Propagation::Stop;
            }

            // Ctrl+Shift+Z → redo
            if ctrl && shift && (keyval == gtk4::gdk::Key::z || keyval == gtk4::gdk::Key::Z) {
                let result = {
                    let mut s = cs2.borrow_mut();
                    if let Some(op) = s.history.redo_pop() {
                        let sel = apply_op(&op, &mut s.template.widgets);
                        s.history.undo_push(op);
                        Some(sel)
                    } else {
                        None
                    }
                };
                if let Some(new_sel) = result {
                    let obs_clone = obs2.borrow().clone();
                    select_widget(&cs2, new_sel, &obs_clone);
                    area2.queue_draw();
                }
                return gtk4::glib::Propagation::Stop;
            }

            gtk4::glib::Propagation::Proceed
        });
        root.add_controller(key);
    }

    root
}

/// Build the snap-to-grid / smart guides control row.
fn build_snap_row(cs: &Rc<RefCell<CreatorState>>) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(4)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();

    row.append(&Label::new(Some("Snap:")));

    let snap_switch = Switch::new();
    snap_switch.set_active(false);
    snap_switch.set_tooltip_text(Some("Enable snap-to-grid"));
    row.append(&snap_switch);

    let grid_spin = SpinButton::with_range(0.5, 50.0, 0.5);
    grid_spin.set_digits(1);
    grid_spin.set_value(5.0);
    grid_spin.set_tooltip_text(Some("Snap grid spacing (mm)"));
    grid_spin.set_sensitive(false);
    row.append(&grid_spin);

    row.append(&Label::new(Some("mm")));
    row.append(&Label::new(Some("  Smart guides:")));

    let guides_switch = Switch::new();
    guides_switch.set_active(true);
    guides_switch.set_tooltip_text(Some("Show alignment guides while dragging"));
    row.append(&guides_switch);

    {
        let cs2 = cs.clone();
        let grid_spin2 = grid_spin.clone();
        snap_switch.connect_active_notify(move |sw| {
            let on = sw.is_active();
            grid_spin2.set_sensitive(on);
            cs2.borrow_mut().snap_grid_mm = if on { Some(grid_spin2.value()) } else { None };
        });
    }

    {
        let cs2 = cs.clone();
        let snap_switch2 = snap_switch.clone();
        grid_spin.connect_value_changed(move |sb| {
            if snap_switch2.is_active() {
                cs2.borrow_mut().snap_grid_mm = Some(sb.value());
            }
        });
    }

    {
        let cs2 = cs.clone();
        guides_switch.connect_active_notify(move |sw| {
            cs2.borrow_mut().smart_guides_active = sw.is_active();
        });
    }

    row
}

fn build_meta_row(cs: &Rc<RefCell<CreatorState>>) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();

    row.append(&Label::new(Some("Name:")));
    let name_entry = Entry::builder().placeholder_text("Template name").hexpand(true).build();
    {
        let t = cs.borrow();
        name_entry.set_text(&t.template.name);
    }
    name_entry.connect_changed({
        let cs = cs.clone();
        move |e| { cs.borrow_mut().template.name = e.text().to_string(); }
    });
    row.append(&name_entry);

    row.append(&Label::new(Some("Description:")));
    let desc_entry = Entry::builder().placeholder_text("Optional description").hexpand(true).build();
    {
        let t = cs.borrow();
        desc_entry.set_text(&t.template.description);
    }
    desc_entry.connect_changed({
        let cs = cs.clone();
        move |e| { cs.borrow_mut().template.description = e.text().to_string(); }
    });
    row.append(&desc_entry);

    row.append(&Label::new(Some("Category:")));
    let cat_entry = Entry::builder()
        .placeholder_text("e.g. Daily Planner, Basics")
        .build();
    cat_entry.set_width_chars(18);
    {
        let t = cs.borrow();
        cat_entry.set_text(&t.template.category);
    }
    cat_entry.connect_changed({
        let cs = cs.clone();
        move |e| { cs.borrow_mut().template.category = e.text().to_string(); }
    });
    row.append(&cat_entry);

    row
}

fn build_palette(
    cs: &Rc<RefCell<CreatorState>>,
    sel_obs: &Rc<RefCell<Option<SelectionObserverFn>>>,
) -> ScrolledWindow {
    let scroller = ScrolledWindow::builder()
        .width_request(140)
        .vexpand(true)
        .build();

    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let label = Label::builder().label("Widgets").halign(gtk4::Align::Start).build();
    label.add_css_class("title-4");
    vbox.append(&label);

    let tools: &[(&str, PlaceTool)] = &[
        ("Text", PlaceTool::TextBlock),
        ("Rectangle", PlaceTool::Rectangle),
        ("Ellipse", PlaceTool::Ellipse),
        ("Line", PlaceTool::Line),
        ("Grid Area", PlaceTool::GridRegion),
        ("Ruled Lines", PlaceTool::LinesRegion),
        ("Dot Grid", PlaceTool::DotsRegion),
        ("Calendar Month", PlaceTool::CalendarMonth),
        ("Timeline", PlaceTool::Timeline),
        ("Checklist", PlaceTool::Checklist),
        ("Big Three", PlaceTool::BigThree),
        ("Priority List", PlaceTool::PriorityList),
        ("Day Schedule", PlaceTool::DailyAppointments),
        ("Weekly Compass", PlaceTool::WeeklyCompass),
    ];

    for (label_text, tool) in tools {
        let btn = Button::with_label(label_text);
        let cs2 = cs.clone();
        let obs2 = sel_obs.clone();
        let t = *tool;
        btn.connect_clicked(move |_| {
            cs2.borrow_mut().tool = t;
            let obs = obs2.borrow().clone();
            select_widget(&cs2, None, &obs);
        });
        vbox.append(&btn);
    }

    let desel_btn = Button::with_label("Select/Move");
    desel_btn.connect_clicked({
        let cs = cs.clone();
        move |_| { cs.borrow_mut().tool = PlaceTool::None; }
    });
    vbox.prepend(&desel_btn);

    let del_btn = Button::with_label("Delete widget");
    del_btn.add_css_class("destructive-action");
    del_btn.connect_clicked({
        let cs = cs.clone();
        let obs2 = sel_obs.clone();
        move |_| {
            let removed = {
                let mut s = cs.borrow_mut();
                if let Some(idx) = s.selected_idx {
                    if idx < s.template.widgets.len() {
                        let w = s.template.widgets.remove(idx);
                        s.history.push(EditOp::Remove { idx, widget: w });
                        Some(idx)
                    } else { None }
                } else { None }
            };
            if removed.is_some() {
                let obs = obs2.borrow().clone();
                select_widget(&cs, None, &obs);
            }
        }
    });
    vbox.append(&del_btn);

    scroller.set_child(Some(&vbox));
    scroller
}

fn build_canvas_area(
    cs: &Rc<RefCell<CreatorState>>,
    sel_obs: &Rc<RefCell<Option<SelectionObserverFn>>>,
) -> DrawingArea {
    let area = DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();

    area.set_draw_func({
        let cs = cs.clone();
        move |_area, ctx, w, h| {
            draw_creator_canvas(ctx, w as f64, h as f64, &cs.borrow());
        }
    });

    let drag = gtk4::GestureDrag::new();
    drag.connect_drag_begin({
        let cs = cs.clone();
        let area = area.clone();
        let obs = sel_obs.clone();
        move |gesture, x, y| {
            let size = get_area_size(&area);
            let canvas_pt = screen_to_template(x, y, size, &cs.borrow().template);
            let sel_change: Option<Option<usize>>;
            {
                let mut s = cs.borrow_mut();
                if s.tool != PlaceTool::None {
                    s.drag_start_canvas = Some(canvas_pt);
                    s.drag_active = false;
                    sel_change = None; // no selection change
                } else {
                    let hit = hit_test(&s.template.widgets, canvas_pt);
                    if let Some(idx) = hit {
                        let handle = resize_handle_hit(&s.template.widgets[idx].rect, canvas_pt);
                        s.drag_handle = handle;
                        s.drag_orig_rect = Some(s.template.widgets[idx].rect.clone());
                        s.drag_start_canvas = Some(canvas_pt);
                        s.drag_active = true;
                        let prev = s.selected_idx;
                        s.selected_idx = Some(idx);
                        sel_change = if prev != Some(idx) { Some(Some(idx)) } else { None };
                    } else {
                        let prev = s.selected_idx;
                        s.selected_idx = None;
                        s.drag_start_canvas = None;
                        sel_change = if prev.is_some() { Some(None) } else { None };
                    }
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                }
            }
            // Fire observer outside the borrow.
            if let Some(new_sel) = sel_change {
                let obs_fn = obs.borrow().clone();
                if let Some(f) = obs_fn { f(new_sel); }
            }
        }
    });

    drag.connect_drag_update({
        let cs = cs.clone();
        let area = area.clone();
        move |_, dx, dy| {
            let size = get_area_size(&area);
            let scale = template_scale(size, &cs.borrow().template);
            let (dcx, dcy) = (dx / scale, dy / scale);
            let mut s = cs.borrow_mut();
            if s.tool != PlaceTool::None {
                s.drag_active = true;
            } else if s.drag_active {
                if let (Some(orig), Some(idx)) = (s.drag_orig_rect.clone(), s.selected_idx) {
                    if idx < s.template.widgets.len() {
                        match s.drag_handle {
                            Handle::Move => {
                                let raw_x = orig.x + dcx;
                                let raw_y = orig.y + dcy;
                                // Apply snap-to-grid first.
                                let snapped_x = s.snap(raw_x);
                                let snapped_y = s.snap(raw_y);
                                s.template.widgets[idx].rect.x = snapped_x;
                                s.template.widgets[idx].rect.y = snapped_y;

                                // Smart guides: snap to other widget edges / page edges.
                                if s.smart_guides_active {
                                    apply_smart_snap(
                                        &mut s.template.widgets,
                                        idx,
                                        orig.clone(),
                                        dcx,
                                        dcy,
                                    );
                                }
                            }
                            Handle::ResizeBottomRight => {
                                let snapped_w = s.snap(orig.width + dcx).max(2.0);
                                let snapped_h = s.snap(orig.height + dcy).max(2.0);
                                s.template.widgets[idx].rect.width = snapped_w;
                                s.template.widgets[idx].rect.height = snapped_h;
                            }
                        }
                    }
                }
            }
            drop(s);
            area.queue_draw();
        }
    });

    drag.connect_drag_end({
        let cs = cs.clone();
        let area = area.clone();
        let obs = sel_obs.clone();
        move |_, dx, dy| {
            let size = get_area_size(&area);
            let canvas_start = cs.borrow().drag_start_canvas;
            let Some(start) = canvas_start else { return };
            let scale = template_scale(size, &cs.borrow().template);
            let end = (start.0 + dx / scale, start.1 + dy / scale);

            let sel_change: Option<Option<usize>>;
            {
                let mut s = cs.borrow_mut();
                if s.tool != PlaceTool::None && s.drag_active {
                    // Place a new widget — snap coordinates.
                    let rx = s.snap(start.0.min(end.0));
                    let ry = s.snap(start.1.min(end.1));
                    let rw = s.snap((end.0 - start.0).abs()).max(2.0);
                    let rh = s.snap((end.1 - start.1).abs()).max(2.0);
                    let kind = default_kind_for(s.tool);
                    let widget = TemplateWidget {
                        id: Uuid::new_v4(),
                        kind,
                        rect: WidgetRect { x: rx, y: ry, width: rw, height: rh },
                        style: WidgetStyle::default(),
                    };
                    let insert_idx = s.template.widgets.len();
                    s.template.widgets.push(widget.clone());
                    s.history.push(EditOp::Insert { idx: insert_idx, widget });
                    let new_sel = s.template.widgets.len() - 1;
                    s.selected_idx = Some(new_sel);
                    s.tool = PlaceTool::None;
                    sel_change = Some(Some(new_sel));
                } else if s.drag_active {
                    // Move/resize finished — push undo op.
                    if let (Some(orig), Some(idx)) = (s.drag_orig_rect.clone(), s.selected_idx) {
                        if idx < s.template.widgets.len() {
                            let after = s.template.widgets[idx].rect.clone();
                            if after != orig {
                                let op = match s.drag_handle {
                                    Handle::Move => EditOp::Move {
                                        idx, before_rect: orig, after_rect: after,
                                    },
                                    Handle::ResizeBottomRight => EditOp::Resize {
                                        idx, before_rect: orig, after_rect: after,
                                    },
                                };
                                s.history.push(op);
                            }
                        }
                    }
                    sel_change = None;
                } else {
                    sel_change = None;
                }
                s.drag_start_canvas = None;
                s.drag_active = false;
                s.drag_orig_rect = None;
            }
            // Fire observer outside the borrow.
            if let Some(new_sel) = sel_change {
                let obs_fn = obs.borrow().clone();
                if let Some(f) = obs_fn { f(new_sel); }
            }
            area.queue_draw();
        }
    });

    area.add_controller(drag);

    let key = gtk4::EventControllerKey::new();
    key.connect_key_pressed({
        let cs = cs.clone();
        let area = area.clone();
        let obs = sel_obs.clone();
        move |_, key, _, _| {
            if key == gtk4::gdk::Key::Delete || key == gtk4::gdk::Key::BackSpace {
                let removed = {
                    let mut s = cs.borrow_mut();
                    if let Some(idx) = s.selected_idx {
                        if idx < s.template.widgets.len() {
                            let w = s.template.widgets.remove(idx);
                            s.history.push(EditOp::Remove { idx, widget: w });
                            true
                        } else { false }
                    } else { false }
                };
                if removed {
                    let obs_fn = obs.borrow().clone();
                    // selected_idx is already removed above; update it to None.
                    cs.borrow_mut().selected_idx = None;
                    if let Some(f) = obs_fn { f(None); }
                    area.queue_draw();
                }
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        }
    });
    area.set_focusable(true);
    area.add_controller(key);

    area
}

/// Smart-guide alignment snap: while dragging `idx`, if the widget's
/// left/right/top/bottom edge aligns within GUIDE_SNAP_MM of another widget's
/// corresponding edge (or the page left/top edges), snap to that edge.
fn apply_smart_snap(
    widgets: &mut Vec<TemplateWidget>,
    idx: usize,
    orig: WidgetRect,
    dcx: f64,
    dcy: f64,
) {
    const GUIDE_SNAP_MM: f64 = 1.5;

    let proposed_x = orig.x + dcx;
    let proposed_y = orig.y + dcy;
    let w = widgets[idx].rect.width;
    let h = widgets[idx].rect.height;

    let mut x_cands: Vec<f64> = vec![0.0];
    let mut y_cands: Vec<f64> = vec![0.0];

    for (i, ww) in widgets.iter().enumerate() {
        if i == idx { continue; }
        // Snap dragged-widget's left edge to other widget's left/right edges,
        // and dragged-widget's right edge aligned the same way.
        x_cands.push(ww.rect.x);
        x_cands.push(ww.rect.x + ww.rect.width - w);
        x_cands.push(ww.rect.x + ww.rect.width);
        x_cands.push(ww.rect.x + (ww.rect.width - w) * 0.5);

        y_cands.push(ww.rect.y);
        y_cands.push(ww.rect.y + ww.rect.height - h);
        y_cands.push(ww.rect.y + ww.rect.height);
        y_cands.push(ww.rect.y + (ww.rect.height - h) * 0.5);
    }

    let snapped_x = x_cands.iter().copied()
        .filter(|&c| (proposed_x - c).abs() < GUIDE_SNAP_MM)
        .min_by(|a, b| {
            (proposed_x - a).abs().partial_cmp(&(proposed_x - b).abs()).unwrap()
        })
        .unwrap_or(proposed_x);

    let snapped_y = y_cands.iter().copied()
        .filter(|&c| (proposed_y - c).abs() < GUIDE_SNAP_MM)
        .min_by(|a, b| {
            (proposed_y - a).abs().partial_cmp(&(proposed_y - b).abs()).unwrap()
        })
        .unwrap_or(proposed_y);

    widgets[idx].rect.x = snapped_x;
    widgets[idx].rect.y = snapped_y;
}

fn color_to_rgba(c: Color) -> RGBA {
    RGBA::new(
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    )
}

fn rgba_to_color(rgba: RGBA) -> Color {
    Color {
        r: (rgba.red() * 255.0) as u8,
        g: (rgba.green() * 255.0) as u8,
        b: (rgba.blue() * 255.0) as u8,
        a: (rgba.alpha() * 255.0) as u8,
    }
}

const TEXT_VARIABLES: &[(&str, &str)] = &[
    ("{date}", "ISO date — 2026-05-02"),
    ("{year}", "Year — 2026"),
    ("{month}", "Month number — 05"),
    ("{month_name}", "Month name — May"),
    ("{week}", "ISO week — 18"),
    ("{day}", "Day of month — 02"),
    ("{weekday}", "Weekday name — Saturday"),
];

/// Rebuild the properties side panel based on the currently-selected widget.
/// `area` is queue_draw'd whenever a property changes so the canvas reflects
/// the edit immediately.
fn refresh_props_panel(
    vbox: &Rc<GtkBox>,
    cs: &Rc<RefCell<CreatorState>>,
    area: &DrawingArea,
) {
    while let Some(child) = vbox.first_child() {
        vbox.remove(&child);
    }

    let header = Label::builder().label("Properties").halign(gtk4::Align::Start).build();
    header.add_css_class("title-4");
    vbox.append(&header);

    let sel_idx = cs.borrow().selected_idx;
    let Some(idx) = sel_idx else {
        let hint = Label::builder()
            .label("Select a widget to edit its properties.")
            .halign(gtk4::Align::Start)
            .wrap(true)
            .build();
        hint.add_css_class("dim-label");
        vbox.append(&hint);
        return;
    };

    let widget_kind_clone = cs.borrow().template.widgets.get(idx).map(|w| w.kind.clone());
    let Some(kind) = widget_kind_clone else { return; };

    let kind_lbl = Label::builder()
        .label(kind_label(&kind))
        .halign(gtk4::Align::Start)
        .build();
    kind_lbl.add_css_class("heading");
    vbox.append(&kind_lbl);

    // ── Stroke color ─────────────────────────────────────────────────────
    let style = cs.borrow().template.widgets[idx].style.clone();
    vbox.append(&Label::builder().label("Stroke color").halign(gtk4::Align::Start).build());
    let stroke_dialog = ColorDialog::builder().with_alpha(true).build();
    let stroke_btn = ColorDialogButton::new(Some(stroke_dialog));
    stroke_btn.set_rgba(&color_to_rgba(style.stroke_color));
    {
        let cs2 = cs.clone();
        let area2 = area.clone();
        stroke_btn.connect_rgba_notify(move |b| {
            let c = rgba_to_color(b.rgba());
            if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                w.style.stroke_color = c;
            }
            area2.queue_draw();
        });
    }
    vbox.append(&stroke_btn);

    // ── Fill color (with on/off toggle) ──────────────────────────────────
    let fill_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    fill_row.append(&Label::new(Some("Fill")));
    let fill_switch = Switch::new();
    fill_switch.set_active(style.fill_color.is_some());
    fill_row.append(&fill_switch);
    vbox.append(&fill_row);

    let fill_dialog = ColorDialog::builder().with_alpha(true).build();
    let fill_btn = ColorDialogButton::new(Some(fill_dialog));
    let fill_seed = style.fill_color.unwrap_or(Color { r: 240, g: 240, b: 240, a: 255 });
    fill_btn.set_rgba(&color_to_rgba(fill_seed));
    fill_btn.set_sensitive(style.fill_color.is_some());
    {
        let cs2 = cs.clone();
        let area2 = area.clone();
        let fill_btn2 = fill_btn.clone();
        fill_switch.connect_active_notify(move |sw| {
            let on = sw.is_active();
            fill_btn2.set_sensitive(on);
            if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                w.style.fill_color = if on {
                    Some(rgba_to_color(fill_btn2.rgba()))
                } else { None };
            }
            area2.queue_draw();
        });
    }
    {
        let cs2 = cs.clone();
        let area2 = area.clone();
        fill_btn.connect_rgba_notify(move |b| {
            let c = rgba_to_color(b.rgba());
            if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                if w.style.fill_color.is_some() {
                    w.style.fill_color = Some(c);
                }
            }
            area2.queue_draw();
        });
    }
    vbox.append(&fill_btn);

    // ── Stroke width (mm) ─────────────────────────────────────────────────
    vbox.append(&Label::builder().label("Stroke width (mm)").halign(gtk4::Align::Start).build());
    let width_spin = SpinButton::with_range(0.05, 5.0, 0.05);
    width_spin.set_digits(2);
    width_spin.set_value(style.stroke_width_mm);
    {
        let cs2 = cs.clone();
        let area2 = area.clone();
        width_spin.connect_value_changed(move |sb| {
            if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                w.style.stroke_width_mm = sb.value();
            }
            area2.queue_draw();
        });
    }
    vbox.append(&width_spin);

    // ── Per-kind controls ────────────────────────────────────────────────
    match kind {
        WidgetKind::TextBlock { text, font_size_mm } => {
            vbox.append(&Label::builder().label("Text").halign(gtk4::Align::Start).build());
            let entry = Entry::builder().text(&text).hexpand(true).build();
            entry.set_tooltip_text(Some(
                "Use {date} {weekday} {month_name} {year} {week} {day} {month}",
            ));
            {
                let cs2 = cs.clone();
                let area2 = area.clone();
                entry.connect_changed(move |e| {
                    let s = e.text().to_string();
                    if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                        if let WidgetKind::TextBlock { text, .. } = &mut w.kind {
                            *text = s;
                        }
                    }
                    area2.queue_draw();
                });
            }
            vbox.append(&entry);

            let var_btn = MenuButton::builder().label("Insert variable…").build();
            let popover = Popover::new();
            let pop_box = GtkBox::builder()
                .orientation(Orientation::Vertical)
                .spacing(2)
                .margin_top(6).margin_bottom(6).margin_start(6).margin_end(6)
                .build();

            let preview = Label::builder()
                .label("").halign(gtk4::Align::Start).wrap(true).build();
            preview.add_css_class("var-preview");
            let refresh_preview = {
                let preview = preview.clone();
                let entry = entry.clone();
                move || {
                    let today = chrono::Local::now().date_naive();
                    let ctx = journal_core::TitleContext::new(today);
                    let expanded = journal_core::render_title(&entry.text(), &ctx);
                    preview.set_text(&format!("Today → {}", expanded));
                }
            };
            refresh_preview();
            {
                let rp = refresh_preview.clone();
                entry.connect_changed(move |_| rp());
            }
            pop_box.append(&preview);

            let hdr = Label::builder().label("Date variables").halign(gtk4::Align::Start).build();
            hdr.add_css_class("var-group-header");
            pop_box.append(&hdr);

            for (token, desc) in TEXT_VARIABLES {
                let row = Button::with_label(&format!("{}  —  {}", token, desc));
                row.set_halign(gtk4::Align::Fill);
                row.add_css_class("flat");
                let entry2 = entry.clone();
                let pop2 = popover.clone();
                let tok = (*token).to_string();
                let rp = refresh_preview.clone();
                row.connect_clicked(move |_| {
                    let cur = entry2.text().to_string();
                    let pos = entry2.position();
                    let mut chars: Vec<char> = cur.chars().collect();
                    let insert_at = (pos as usize).min(chars.len());
                    for (i, ch) in tok.chars().enumerate() {
                        chars.insert(insert_at + i, ch);
                    }
                    entry2.set_text(&chars.into_iter().collect::<String>());
                    entry2.set_position((insert_at + tok.chars().count()) as i32);
                    rp();
                    pop2.popdown();
                });
                pop_box.append(&row);
            }
            popover.set_child(Some(&pop_box));
            var_btn.set_popover(Some(&popover));
            vbox.append(&var_btn);

            vbox.append(&Label::builder().label("Font size (mm)").halign(gtk4::Align::Start).build());
            let font_spin = SpinButton::with_range(1.0, 80.0, 0.5);
            font_spin.set_digits(1);
            font_spin.set_value(font_size_mm);
            {
                let cs2 = cs.clone();
                let area2 = area.clone();
                font_spin.connect_value_changed(move |sb| {
                    if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                        if let WidgetKind::TextBlock { font_size_mm, .. } = &mut w.kind {
                            *font_size_mm = sb.value();
                        }
                    }
                    area2.queue_draw();
                });
            }
            vbox.append(&font_spin);
        }
        WidgetKind::Line { thickness_mm } => {
            vbox.append(&Label::builder().label("Thickness (mm)").halign(gtk4::Align::Start).build());
            let spin = SpinButton::with_range(0.05, 10.0, 0.1);
            spin.set_digits(2);
            spin.set_value(thickness_mm);
            let cs2 = cs.clone();
            let area2 = area.clone();
            spin.connect_value_changed(move |sb| {
                if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                    if let WidgetKind::Line { thickness_mm } = &mut w.kind {
                        *thickness_mm = sb.value();
                    }
                }
                area2.queue_draw();
            });
            vbox.append(&spin);
        }
        WidgetKind::GridRegion { spacing_mm }
        | WidgetKind::LinesRegion { spacing_mm }
        | WidgetKind::DotsRegion { spacing_mm } => {
            vbox.append(&Label::builder().label("Spacing (mm)").halign(gtk4::Align::Start).build());
            let spin = SpinButton::with_range(1.0, 50.0, 0.5);
            spin.set_digits(1);
            spin.set_value(spacing_mm);
            let cs2 = cs.clone();
            let area2 = area.clone();
            spin.connect_value_changed(move |sb| {
                if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                    let v = sb.value();
                    match &mut w.kind {
                        WidgetKind::GridRegion { spacing_mm }
                        | WidgetKind::LinesRegion { spacing_mm }
                        | WidgetKind::DotsRegion { spacing_mm } => *spacing_mm = v,
                        _ => {}
                    }
                }
                area2.queue_draw();
            });
            vbox.append(&spin);
        }
        WidgetKind::Timeline { .. } | WidgetKind::DailyAppointments { .. } => {
            vbox.append(&Label::builder().label("Start hour").halign(gtk4::Align::Start).build());
            let start_spin = SpinButton::with_range(0.0, 23.0, 1.0);
            start_spin.set_digits(0);
            let cur_start = match &cs.borrow().template.widgets[idx].kind {
                WidgetKind::Timeline { start_hour, .. } => *start_hour as f64,
                WidgetKind::DailyAppointments { start_hour, .. } => *start_hour as f64,
                _ => 8.0,
            };
            start_spin.set_value(cur_start);
            {
                let cs2 = cs.clone();
                let area2 = area.clone();
                start_spin.connect_value_changed(move |sb| {
                    let v = sb.value() as u8;
                    if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                        match &mut w.kind {
                            WidgetKind::Timeline { start_hour, .. } => *start_hour = v,
                            WidgetKind::DailyAppointments { start_hour, .. } => *start_hour = v,
                            _ => {}
                        }
                    }
                    area2.queue_draw();
                });
            }
            vbox.append(&start_spin);

            vbox.append(&Label::builder().label("End hour").halign(gtk4::Align::Start).build());
            let end_spin = SpinButton::with_range(1.0, 24.0, 1.0);
            end_spin.set_digits(0);
            let cur_end = match &cs.borrow().template.widgets[idx].kind {
                WidgetKind::Timeline { end_hour, .. } => *end_hour as f64,
                WidgetKind::DailyAppointments { end_hour, .. } => *end_hour as f64,
                _ => 20.0,
            };
            end_spin.set_value(cur_end);
            {
                let cs2 = cs.clone();
                let area2 = area.clone();
                end_spin.connect_value_changed(move |sb| {
                    let v = sb.value() as u8;
                    if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                        match &mut w.kind {
                            WidgetKind::Timeline { end_hour, .. } => *end_hour = v,
                            WidgetKind::DailyAppointments { end_hour, .. } => *end_hour = v,
                            _ => {}
                        }
                    }
                    area2.queue_draw();
                });
            }
            vbox.append(&end_spin);
        }
        WidgetKind::PriorityList { count } => {
            vbox.append(&Label::builder().label("Rows").halign(gtk4::Align::Start).build());
            let spin = SpinButton::with_range(1.0, 60.0, 1.0);
            spin.set_digits(0);
            spin.set_value(count as f64);
            let cs2 = cs.clone();
            let area2 = area.clone();
            spin.connect_value_changed(move |sb| {
                if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                    if let WidgetKind::PriorityList { count } = &mut w.kind {
                        *count = sb.value() as u32;
                    }
                }
                area2.queue_draw();
            });
            vbox.append(&spin);
        }
        WidgetKind::Checklist { items } => {
            vbox.append(&Label::builder().label("Items (one per line)").halign(gtk4::Align::Start).build());
            let entry = Entry::builder().text(&items.join(" | ")).hexpand(true).build();
            entry.set_tooltip_text(Some("Separator: ' | '"));
            let cs2 = cs.clone();
            let area2 = area.clone();
            entry.connect_changed(move |e| {
                let parts: Vec<String> =
                    e.text().split('|').map(|s| s.trim().to_string()).collect();
                if let Some(w) = cs2.borrow_mut().template.widgets.get_mut(idx) {
                    if let WidgetKind::Checklist { items } = &mut w.kind {
                        *items = parts;
                    }
                }
                area2.queue_draw();
            });
            vbox.append(&entry);
        }
        _ => {}
    }
}

fn kind_label(k: &WidgetKind) -> &'static str {
    match k {
        WidgetKind::TextBlock { .. } => "Text Block",
        WidgetKind::Rectangle => "Rectangle",
        WidgetKind::Ellipse => "Ellipse",
        WidgetKind::Line { .. } => "Line",
        WidgetKind::GridRegion { .. } => "Grid Region",
        WidgetKind::LinesRegion { .. } => "Lines Region",
        WidgetKind::DotsRegion { .. } => "Dots Region",
        WidgetKind::CalendarMonth => "Calendar Month",
        WidgetKind::Timeline { .. } => "Timeline",
        WidgetKind::Checklist { .. } => "Checklist",
        WidgetKind::BigThree => "Big Three",
        WidgetKind::PriorityList { .. } => "Priority List",
        WidgetKind::DailyAppointments { .. } => "Day Schedule",
        WidgetKind::WeeklyCompass => "Weekly Compass",
    }
}

fn get_area_size(area: &DrawingArea) -> (f64, f64) {
    (area.width() as f64, area.height() as f64)
}

fn template_scale(screen_size: (f64, f64), template: &PageTemplate) -> f64 {
    let margin = 0.9;
    let (sw, sh) = screen_size;
    let (tw, th) = template.size_mm;
    if sw <= 0.0 || sh <= 0.0 || tw <= 0.0 || th <= 0.0 { return 1.0; }
    (sw / tw).min(sh / th) * margin
}

fn template_origin(screen_size: (f64, f64), template: &PageTemplate) -> (f64, f64) {
    let scale = template_scale(screen_size, template);
    let (sw, sh) = screen_size;
    let (tw, th) = template.size_mm;
    ((sw - tw * scale) * 0.5, (sh - th * scale) * 0.5)
}

fn screen_to_template(sx: f64, sy: f64, size: (f64, f64), template: &PageTemplate) -> (f64, f64) {
    let scale = template_scale(size, template);
    let (ox, oy) = template_origin(size, template);
    ((sx - ox) / scale, (sy - oy) / scale)
}

fn hit_test(widgets: &[TemplateWidget], pt: (f64, f64)) -> Option<usize> {
    for (i, w) in widgets.iter().enumerate().rev() {
        let r = &w.rect;
        if pt.0 >= r.x && pt.0 <= r.x + r.width && pt.1 >= r.y && pt.1 <= r.y + r.height {
            return Some(i);
        }
    }
    None
}

fn resize_handle_hit(r: &WidgetRect, pt: (f64, f64)) -> Handle {
    let margin = 8.0;
    let bx = r.x + r.width;
    let by = r.y + r.height;
    if (pt.0 - bx).abs() < margin && (pt.1 - by).abs() < margin {
        Handle::ResizeBottomRight
    } else {
        Handle::Move
    }
}

fn default_kind_for(tool: PlaceTool) -> WidgetKind {
    match tool {
        PlaceTool::TextBlock => WidgetKind::TextBlock { text: "Text".into(), font_size_mm: 5.0 },
        PlaceTool::Rectangle => WidgetKind::Rectangle,
        PlaceTool::Ellipse => WidgetKind::Ellipse,
        PlaceTool::Line => WidgetKind::Line { thickness_mm: 0.5 },
        PlaceTool::GridRegion => WidgetKind::GridRegion { spacing_mm: 5.0 },
        PlaceTool::LinesRegion => WidgetKind::LinesRegion { spacing_mm: 7.0 },
        PlaceTool::DotsRegion => WidgetKind::DotsRegion { spacing_mm: 5.0 },
        PlaceTool::CalendarMonth => WidgetKind::CalendarMonth,
        PlaceTool::Timeline => WidgetKind::Timeline { start_hour: 8, end_hour: 20, slot_minutes: 30 },
        PlaceTool::Checklist => WidgetKind::Checklist {
            items: vec!["Item 1".into(), "Item 2".into(), "Item 3".into()],
        },
        PlaceTool::BigThree => WidgetKind::BigThree,
        PlaceTool::PriorityList => WidgetKind::PriorityList { count: 12 },
        PlaceTool::DailyAppointments => WidgetKind::DailyAppointments { start_hour: 7, end_hour: 19 },
        PlaceTool::WeeklyCompass => WidgetKind::WeeklyCompass,
        PlaceTool::None => WidgetKind::Rectangle,
    }
}

fn draw_creator_canvas(ctx: &cairo::Context, w: f64, h: f64, cs: &CreatorState) {
    ctx.set_source_rgb(0.85, 0.85, 0.88);
    let _ = ctx.paint();

    if w <= 0.0 || h <= 0.0 { return; }

    let template = &cs.template;
    let scale = template_scale((w, h), template);
    let (ox, oy) = template_origin((w, h), template);
    let (tw, th) = template.size_mm;

    ctx.save().ok();
    ctx.translate(ox, oy);
    ctx.scale(scale, scale);

    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.rectangle(0.0, 0.0, tw, th);
    let _ = ctx.fill();

    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.15);
    ctx.set_line_width(0.5 / scale);
    ctx.rectangle(0.0, 0.0, tw, th);
    let _ = ctx.stroke();

    let page_rect = Rect { x: 0.0, y: 0.0, width: tw, height: th };

    let viewport = journal_core::Viewport {
        center: journal_core::Point { x: tw * 0.5, y: th * 0.5 },
        zoom: scale,
        rotation: 0.0,
    };
    let transform = ViewportTransform::new(viewport, tw * scale, th * scale);

    if !template.widgets.is_empty() {
        let render_ctx = WidgetRenderContext {
            date: Some(chrono::Local::now().date_naive()),
        };
        draw_widgets_with_context(ctx, &transform, &template.widgets, page_rect, &render_ctx);
    }

    if let Some(idx) = cs.selected_idx {
        if let Some(w_ref) = template.widgets.get(idx) {
            draw_selection_overlay(ctx, &w_ref.rect, scale);
        }
    }

    // ── Smart guide lines (Feature 3) ─────────────────────────────────────
    // Render amber alignment guides while a Move drag is active.
    if cs.smart_guides_active
        && cs.drag_active
        && cs.drag_handle == Handle::Move
        && cs.drag_start_canvas.is_some()
    {
        if let Some(idx) = cs.selected_idx {
            if let Some(cur) = cs.template.widgets.get(idx) {
                draw_smart_guides(ctx, &cs.template.widgets, idx, cur, scale, tw, th);
            }
        }
    }

    ctx.restore().ok();
}

/// Render amber alignment guide lines during a drag.
fn draw_smart_guides(
    ctx: &cairo::Context,
    widgets: &[TemplateWidget],
    idx: usize,
    cur: &TemplateWidget,
    scale: f64,
    page_w: f64,
    page_h: f64,
) {
    const GUIDE_SNAP_MM: f64 = 1.5;

    ctx.save().ok();
    ctx.set_source_rgba(1.0, 0.65, 0.0, 0.85); // amber
    ctx.set_line_width(0.5 / scale);

    let r = &cur.rect;
    let edges_x = [r.x, r.x + r.width * 0.5, r.x + r.width];
    let edges_y = [r.y, r.y + r.height * 0.5, r.y + r.height];

    for (i, other) in widgets.iter().enumerate() {
        if i == idx { continue; }
        let o = &other.rect;
        let ox_edges = [o.x, o.x + o.width * 0.5, o.x + o.width];
        let oy_edges = [o.y, o.y + o.height * 0.5, o.y + o.height];

        for ex in &edges_x {
            for oex in &ox_edges {
                if (ex - oex).abs() < GUIDE_SNAP_MM {
                    ctx.move_to(*oex, 0.0);
                    ctx.line_to(*oex, page_h);
                    let _ = ctx.stroke();
                }
            }
        }
        for ey in &edges_y {
            for oey in &oy_edges {
                if (ey - oey).abs() < GUIDE_SNAP_MM {
                    ctx.move_to(0.0, *oey);
                    ctx.line_to(page_w, *oey);
                    let _ = ctx.stroke();
                }
            }
        }
    }

    // Page edge guides.
    for ex in &edges_x {
        for px in &[0.0_f64, page_w] {
            if (ex - px).abs() < GUIDE_SNAP_MM {
                ctx.move_to(*px, 0.0);
                ctx.line_to(*px, page_h);
                let _ = ctx.stroke();
            }
        }
    }
    for ey in &edges_y {
        for py in &[0.0_f64, page_h] {
            if (ey - py).abs() < GUIDE_SNAP_MM {
                ctx.move_to(0.0, *py);
                ctx.line_to(page_w, *py);
                let _ = ctx.stroke();
            }
        }
    }

    ctx.restore().ok();
}

fn draw_selection_overlay(ctx: &cairo::Context, r: &WidgetRect, scale: f64) {
    let lw = 1.5 / scale;
    ctx.set_line_width(lw);
    ctx.set_source_rgba(0.2, 0.5, 1.0, 0.8);
    ctx.rectangle(r.x, r.y, r.width, r.height);
    let _ = ctx.stroke();

    let handle_sz = 6.0 / scale;
    let hx = r.x + r.width - handle_sz * 0.5;
    let hy = r.y + r.height - handle_sz * 0.5;
    ctx.set_source_rgba(0.2, 0.5, 1.0, 1.0);
    ctx.rectangle(hx, hy, handle_sz, handle_sz);
    let _ = ctx.fill();
}

fn templates_dir() -> Option<std::path::PathBuf> {
    let base = dirs::data_dir().or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))?;
    Some(base.join("journal").join("templates"))
}

fn save_template(template: &PageTemplate, state: &SharedState) -> anyhow::Result<()> {
    let tdir = templates_dir().ok_or_else(|| anyhow::anyhow!("could not resolve data dir"))?;
    std::fs::create_dir_all(&tdir)?;
    let toml_path = tdir.join(format!("{}.toml", template.id.0));
    let file = template_file_from_page_template(template);
    let toml_text = serialize_template_toml(&file)
        .map_err(|e| anyhow::anyhow!("serialize: {}", e))?;
    std::fs::write(&toml_path, toml_text)?;
    let s = state.borrow();
    s.templates.borrow_mut().insert(template.clone());
    Ok(())
}
