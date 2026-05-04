//! Full-screen Tool Editor — composable-brush MVP.
//!
//! Sidebar (built-in + custom brush library) → layer list → per-layer
//! settings form. Save as… persists to `~/.config/journal/brushes.toml`
//! via `crate::brush_library`. Done sets `state.active_brush_recipe`
//! and returns to the previous view.
//!
//! Phase 2 MVP scope: read/edit existing brush data; new layers
//! via "+ Layer". Drag-reorder, live preview, hue-shift, and the
//! "+ New from blank" button are stretch (Phase 2.5).
//!
//! See `docs/brush-engine.md` §4.4.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, CheckButton, DrawingArea, DropDown, Entry, Frame,
    Label, ListBox, ListBoxRow, Orientation, Paned, ScrolledWindow, Separator, SpinButton,
    StringList,
};
use journal_canvas::background_renderer::BackgroundConfig;
use journal_canvas::vello_renderer::{BrushParams, OverlayState, VelloRenderer};
use journal_canvas::ViewportTransform;
use journal_core::{
    BlendMode, Brush, BrushLayer, ColorMod, Color as JColor, Geometry, PenSettings, Point, Rect,
    Stroke, StrokePoint, TipShape, Viewport, WidthMode,
};
use uuid::Uuid;

use crate::brush_library;
use crate::state::SharedState;

/// Build the Tool Editor stack-page contents.
///
/// `seed_brush` — `Some(b)` opens the editor on `b`; `None` opens
/// blank-slate (a default Pen composition the user can fork).
pub fn build_editor_view(
    _parent: &ApplicationWindow,
    state: SharedState,
    seed_brush: Option<Brush>,
    on_done: Rc<dyn Fn()>,
) -> GtkBox {
    let editor_state = Rc::new(RefCell::new(EditorState {
        brush: seed_brush.unwrap_or_else(default_seed_brush),
        selected_layer: 0,
        rebuilding: false,
    }));

    // Lazy-initialised Vello renderer for the live preview canvas.
    // First open of the editor pays the wgpu init cost (~1s); subsequent
    // repaints reuse it.
    let preview_renderer: Rc<RefCell<Option<VelloRenderer>>> =
        Rc::new(RefCell::new(None));
    let preview_brush: Rc<RefCell<Brush>> = Rc::new(RefCell::new(
        editor_state.borrow().brush.clone(),
    ));

    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    // Top bar — Cancel / Save as / Done.
    let top = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();
    let back_btn = Button::from_icon_name("go-previous-symbolic");
    back_btn.set_tooltip_text(Some("Cancel"));
    let title = Label::builder()
        .label("Tool Editor")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    title.add_css_class("title-3");
    let save_as_btn = Button::with_label("Save as…");
    let done_btn = Button::with_label("Use this brush");
    done_btn.add_css_class("suggested-action");
    top.append(&back_btn);
    top.append(&title);
    top.append(&save_as_btn);
    top.append(&done_btn);
    root.append(&top);
    root.append(&Separator::new(Orientation::Horizontal));

    // Body — split paned.
    let paned = Paned::builder()
        .orientation(Orientation::Horizontal)
        .hexpand(true)
        .vexpand(true)
        .position(300)
        .build();

    // ── Left sidebar ────────────────────────────────────────────────
    let sidebar = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(10)
        .margin_end(10)
        .width_request(280)
        .build();

    let lib_label = Label::builder()
        .label("Brushes")
        .halign(gtk4::Align::Start)
        .build();
    lib_label.add_css_class("title-4");
    sidebar.append(&lib_label);

    let lib_scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    let lib_list = ListBox::new();
    lib_list.set_selection_mode(gtk4::SelectionMode::Browse);
    lib_scroll.set_child(Some(&lib_list));
    sidebar.append(&lib_scroll);

    let new_btn = Button::with_label("+ New blank");
    sidebar.append(&new_btn);

    sidebar.append(&Separator::new(Orientation::Horizontal));

    let layers_label = Label::builder()
        .label("Layers")
        .halign(gtk4::Align::Start)
        .build();
    layers_label.add_css_class("title-4");
    sidebar.append(&layers_label);

    let layers_scroll = ScrolledWindow::builder()
        .height_request(180)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    let layers_list = ListBox::new();
    layers_list.set_selection_mode(gtk4::SelectionMode::Browse);
    layers_scroll.set_child(Some(&layers_list));
    sidebar.append(&layers_scroll);

    let add_layer_btn = Button::with_label("+ Layer");
    let remove_layer_btn = Button::with_label("Remove layer");
    let layer_btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    layer_btn_row.append(&add_layer_btn);
    layer_btn_row.append(&remove_layer_btn);
    sidebar.append(&layer_btn_row);

    paned.set_start_child(Some(&sidebar));

    // ── Right panel ─────────────────────────────────────────────────
    // Vertical layout: preview pinned at top, scrolled form below.
    let right_root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let preview_area = build_preview_area(
        editor_state.clone(),
        preview_renderer.clone(),
        preview_brush.clone(),
    );
    right_root.append(&preview_area);
    right_root.append(&Separator::new(Orientation::Horizontal));

    let right = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .build();
    let right_body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(16)
        .margin_end(16)
        .build();
    right.set_child(Some(&right_body));
    right_root.append(&right);
    paned.set_end_child(Some(&right_root));

    root.append(&paned);

    // Repaint orchestration — the body needs to rebuild on layer
    // changes. Closure captures `editor_state` + the GTK widgets it
    // needs to mutate.
    let rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    {
        let editor_state = editor_state.clone();
        let lib_list = lib_list.clone();
        let layers_list = layers_list.clone();
        let right_body = right_body.clone();
        let state_outer = state.clone();
        let rebuild_self = rebuild.clone();
        let preview_area = preview_area.clone();
        let preview_brush = preview_brush.clone();
        let do_rebuild: Rc<dyn Fn()> = Rc::new(move || {
            editor_state.borrow_mut().rebuilding = true;

            // Clear lists.
            while let Some(child) = lib_list.first_child() {
                lib_list.remove(&child);
            }
            while let Some(child) = layers_list.first_child() {
                layers_list.remove(&child);
            }
            while let Some(child) = right_body.first_child() {
                right_body.remove(&child);
            }

            // Library — built-ins first, then custom.
            let built_in = built_in_brushes_list();
            let custom = state_outer.borrow().brush_library.clone();
            for b in built_in.iter().chain(custom.iter()) {
                let row = ListBoxRow::new();
                let label = Label::builder()
                    .label(format!(
                        "{}{}",
                        b.name,
                        if custom.iter().any(|c| c.id == b.id) {
                            " (custom)"
                        } else {
                            ""
                        },
                    ))
                    .halign(gtk4::Align::Start)
                    .build();
                row.set_child(Some(&label));
                lib_list.append(&row);
                if b.id == editor_state.borrow().brush.id {
                    lib_list.select_row(Some(&row));
                }
            }

            // Layer list — current brush.
            let layers = editor_state.borrow().brush.layers.clone();
            let selected_layer = editor_state.borrow().selected_layer;
            for (i, layer) in layers.iter().enumerate() {
                let row = ListBoxRow::new();
                let row_box = GtkBox::builder()
                    .orientation(Orientation::Horizontal)
                    .spacing(6)
                    .build();
                let chk = CheckButton::new();
                {
                    let editor_state = editor_state.clone();
                    let i_clone = i;
                    let rebuild_self = rebuild_self.clone();
                    chk.connect_toggled(move |b| {
                        if editor_state.borrow().rebuilding {
                            return;
                        }
                        if let Some(l) = editor_state
                            .borrow_mut()
                            .brush
                            .layers
                            .get_mut(i_clone)
                        {
                            l.enabled = b.is_active();
                        }
                        if let Some(f) = rebuild_self.borrow().as_ref().cloned() {
                            f();
                        }
                    });
                }
                chk.set_active(layer.enabled);
                let label = Label::builder()
                    .label(layer_summary(layer, i))
                    .halign(gtk4::Align::Start)
                    .build();
                row_box.append(&chk);
                row_box.append(&label);
                row.set_child(Some(&row_box));
                layers_list.append(&row);
                if i == selected_layer {
                    layers_list.select_row(Some(&row));
                }
            }

            // Right panel — selected layer settings.
            let layers_now = editor_state.borrow().brush.layers.clone();
            let selected = editor_state.borrow().selected_layer;
            if let Some(layer) = layers_now.get(selected) {
                build_layer_settings(
                    &right_body,
                    editor_state.clone(),
                    selected,
                    layer.clone(),
                    rebuild_self.clone(),
                );
            } else {
                let lbl = Label::builder()
                    .label("(no layer selected)")
                    .halign(gtk4::Align::Center)
                    .build();
                lbl.add_css_class("dim-label");
                right_body.append(&lbl);
            }

            // Refresh preview against the latest brush snapshot.
            *preview_brush.borrow_mut() = editor_state.borrow().brush.clone();
            preview_area.queue_draw();

            editor_state.borrow_mut().rebuilding = false;
        });
        *rebuild.borrow_mut() = Some(do_rebuild);
    }
    if let Some(f) = rebuild.borrow().as_ref().cloned() {
        f();
    }

    // Library row click → load that brush as the working copy.
    {
        let editor_state = editor_state.clone();
        let state_outer = state.clone();
        let rebuild = rebuild.clone();
        lib_list.connect_row_selected(move |_, row| {
            if editor_state.borrow().rebuilding {
                return;
            }
            let Some(row) = row else { return };
            let idx = row.index();
            if idx < 0 {
                return;
            }
            let mut brushes = built_in_brushes_list();
            brushes.extend(state_outer.borrow().brush_library.clone());
            if let Some(b) = brushes.get(idx as usize) {
                editor_state.borrow_mut().brush = b.clone();
                editor_state.borrow_mut().selected_layer = 0;
            }
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Layer row click → select that layer.
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        layers_list.connect_row_selected(move |_, row| {
            if editor_state.borrow().rebuilding {
                return;
            }
            let Some(row) = row else { return };
            let idx = row.index();
            if idx >= 0 {
                editor_state.borrow_mut().selected_layer = idx as usize;
            }
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // + New blank.
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        new_btn.connect_clicked(move |_| {
            editor_state.borrow_mut().brush = default_seed_brush();
            editor_state.borrow_mut().selected_layer = 0;
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // + Layer.
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        add_layer_btn.connect_clicked(move |_| {
            editor_state.borrow_mut().brush.layers.push(default_layer());
            let n = editor_state.borrow().brush.layers.len();
            editor_state.borrow_mut().selected_layer = n - 1;
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Remove layer.
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        remove_layer_btn.connect_clicked(move |_| {
            let mut s = editor_state.borrow_mut();
            if s.brush.layers.len() <= 1 {
                return;
            }
            let idx = s.selected_layer.min(s.brush.layers.len() - 1);
            s.brush.layers.remove(idx);
            s.selected_layer = idx.saturating_sub(1).max(0).min(s.brush.layers.len() - 1);
            drop(s);
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Cancel.
    {
        let on_done = on_done.clone();
        back_btn.connect_clicked(move |_| (on_done)());
    }

    // Save as…
    {
        let editor_state = editor_state.clone();
        let state_outer = state.clone();
        let parent_clone = _parent.clone();
        let rebuild = rebuild.clone();
        save_as_btn.connect_clicked(move |_| {
            let editor_state = editor_state.clone();
            let state_outer = state_outer.clone();
            let rebuild = rebuild.clone();
            prompt_save_as(&parent_clone, move |name| {
                if name.trim().is_empty() {
                    return;
                }
                let mut new_brush = editor_state.borrow().brush.clone();
                new_brush.id = Uuid::new_v4();
                new_brush.name = name.trim().to_string();
                state_outer.borrow_mut().brush_library.push(new_brush.clone());
                let lib_snapshot = state_outer.borrow().brush_library.clone();
                if let Err(e) = brush_library::save(&lib_snapshot) {
                    tracing::warn!("brush library save failed: {}", e);
                }
                editor_state.borrow_mut().brush = new_brush;
                if let Some(f) = rebuild.borrow().as_ref().cloned() {
                    f();
                }
            });
        });
    }

    // Use this brush — set active recipe, return.
    {
        let editor_state = editor_state.clone();
        let state_outer = state.clone();
        let on_done = on_done.clone();
        done_btn.connect_clicked(move |_| {
            let brush = editor_state.borrow().brush.clone();
            state_outer.borrow_mut().active_brush_recipe = Some(brush);
            (on_done)();
        });
    }

    root
}

// ── Editor state ───────────────────────────────────────────────────

struct EditorState {
    brush: Brush,
    selected_layer: usize,
    /// Set while `do_rebuild` is mutating the GTK widgets. Signal
    /// handlers (row-selected / toggled / value-changed) early-out
    /// when this is true — programmatic `select_row` and
    /// `set_active` calls fire those signals just like user input,
    /// and re-entrant rebuild calls quickly overflow the stack.
    rebuilding: bool,
}

fn default_seed_brush() -> Brush {
    Brush {
        id: Uuid::new_v4(),
        name: "New brush".into(),
        layers: vec![default_layer()],
    }
}

fn default_layer() -> BrushLayer {
    BrushLayer {
        enabled: true,
        geometry: Geometry::Smooth { resample_step_mm: 1.0 },
        width: WidthMode::Pressure { floor: 0.6, amp: 0.4 },
        tip: TipShape::Round,
        color: ColorMod::default(),
        blend: BlendMode::Normal,
    }
}

fn built_in_brushes_list() -> Vec<Brush> {
    use journal_canvas::built_in_brushes as bi;
    vec![
        bi::pen(0.6, 0.4),
        bi::pencil(0.4, 0.9, 0.12, 8.0, 0.22),
        bi::highlighter(0.6, 0.4),
        bi::paintbrush(1.6, 1.4, 0.95, 0.07, 0.20, 0.95),
        bi::spray(36, 0.06, 0.35),
        bi::calligraphy(45.0, 0.18, 0.5, true),
    ]
}

fn layer_summary(layer: &BrushLayer, idx: usize) -> String {
    let geo = match layer.geometry {
        Geometry::Smooth { .. } => "Smooth",
        Geometry::Outline { .. } => "Outline",
        Geometry::Scatter { .. } => "Scatter",
        Geometry::DabStamp { .. } => "DabStamp",
    };
    let w = match layer.width {
        WidthMode::Constant { .. } => "Const",
        WidthMode::ClampedConstant { .. } => "Clamped",
        WidthMode::Pressure { .. } => "Pressure",
        WidthMode::DirectionAngled { .. } => "Angled",
        WidthMode::TiltBand { .. } => "TiltBand",
    };
    format!("Layer {} — {} + {}", idx + 1, geo, w)
}

// ── Right-panel settings form ─────────────────────────────────────

const GEO_NAMES: &[&str] = &["Smooth", "Outline", "Scatter", "DabStamp"];
const WIDTH_NAMES: &[&str] = &["Constant", "ClampedConstant", "Pressure", "DirectionAngled", "TiltBand"];
const TIP_NAMES: &[&str] = &["Round", "Square", "FlatNib", "Diamond", "StarN"];
const BLEND_NAMES: &[&str] = &[
    "Normal", "Multiply", "Screen", "Overlay", "Darken", "Lighten", "Erase",
];

fn geom_idx(g: &Geometry) -> u32 {
    match g {
        Geometry::Smooth { .. } => 0,
        Geometry::Outline { .. } => 1,
        Geometry::Scatter { .. } => 2,
        Geometry::DabStamp { .. } => 3,
    }
}
fn width_idx(w: &WidthMode) -> u32 {
    match w {
        WidthMode::Constant { .. } => 0,
        WidthMode::ClampedConstant { .. } => 1,
        WidthMode::Pressure { .. } => 2,
        WidthMode::DirectionAngled { .. } => 3,
        WidthMode::TiltBand { .. } => 4,
    }
}
fn tip_idx(t: &TipShape) -> u32 {
    match t {
        TipShape::Round => 0,
        TipShape::Square => 1,
        TipShape::FlatNib { .. } => 2,
        TipShape::Diamond => 3,
        TipShape::StarN { .. } => 4,
    }
}
fn blend_idx(b: BlendMode) -> u32 {
    match b {
        BlendMode::Normal => 0,
        BlendMode::Multiply => 1,
        BlendMode::Screen => 2,
        BlendMode::Overlay => 3,
        BlendMode::Darken => 4,
        BlendMode::Lighten => 5,
        BlendMode::Erase => 6,
    }
}

fn default_geometry_for(idx: u32) -> Geometry {
    match idx {
        1 => Geometry::Outline { resample_step_mm: 0.5, smooth_outline: true },
        2 => Geometry::Scatter {
            density: 36,
            spread_mm: 0.0,
            falloff: 2.0,
            directional_bias_deg: None,
        },
        3 => Geometry::DabStamp { step_mult: 1.0 },
        _ => Geometry::Smooth { resample_step_mm: 1.0 },
    }
}
fn default_width_for(idx: u32) -> WidthMode {
    match idx {
        1 => WidthMode::ClampedConstant {
            width_mult: 1.0,
            min_mm: 0.4,
            max_mm: 0.9,
        },
        2 => WidthMode::Pressure { floor: 0.6, amp: 0.4 },
        3 => WidthMode::DirectionAngled { nib_deg: 45.0, min_ratio: 0.18 },
        4 => WidthMode::TiltBand {
            threshold: 0.12,
            band_mult: 8.0,
            alpha_scale: 0.22,
        },
        _ => WidthMode::Constant { width_mult: 1.0 },
    }
}
fn default_tip_for(idx: u32) -> TipShape {
    match idx {
        1 => TipShape::Square,
        2 => TipShape::FlatNib { angle_deg: 45.0, aspect: 0.3 },
        3 => TipShape::Diamond,
        4 => TipShape::StarN { points: 5, inner_ratio: 0.5 },
        _ => TipShape::Round,
    }
}
fn blend_from_idx(idx: u32) -> BlendMode {
    match idx {
        1 => BlendMode::Multiply,
        2 => BlendMode::Screen,
        3 => BlendMode::Overlay,
        4 => BlendMode::Darken,
        5 => BlendMode::Lighten,
        6 => BlendMode::Erase,
        _ => BlendMode::Normal,
    }
}

/// Build the right-panel form for a single layer. Each input has a
/// commit closure that writes back into `editor_state.brush.layers[idx]`
/// then triggers `rebuild` so the form/sidebar refresh.
fn build_layer_settings(
    parent: &GtkBox,
    editor_state: Rc<RefCell<EditorState>>,
    layer_idx: usize,
    layer: BrushLayer,
    rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) {
    let title = Label::builder()
        .label(format!("Layer {} settings", layer_idx + 1))
        .halign(gtk4::Align::Start)
        .build();
    title.add_css_class("title-4");
    parent.append(&title);

    // Geometry dropdown.
    let geo_strs = StringList::new(GEO_NAMES);
    let geo_dd = DropDown::builder().model(&geo_strs).hexpand(true).build();
    geo_dd.set_selected(geom_idx(&layer.geometry));
    parent.append(&row("Geometry", geo_dd.upcast_ref()));

    // Geometry sub-params.
    let geo_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_start(20)
        .build();
    fill_geometry_subparams(&geo_box, layer.geometry.clone(), editor_state.clone(), layer_idx, rebuild.clone());
    parent.append(&geo_box);

    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        geo_dd.connect_selected_notify(move |dd| {
            let new_geo = default_geometry_for(dd.selected());
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.geometry = new_geo;
            }
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Width dropdown.
    let width_strs = StringList::new(WIDTH_NAMES);
    let width_dd = DropDown::builder().model(&width_strs).hexpand(true).build();
    width_dd.set_selected(width_idx(&layer.width));
    parent.append(&row("Width", width_dd.upcast_ref()));

    let width_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_start(20)
        .build();
    fill_width_subparams(&width_box, layer.width, editor_state.clone(), layer_idx, rebuild.clone());
    parent.append(&width_box);

    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        width_dd.connect_selected_notify(move |dd| {
            let new_w = default_width_for(dd.selected());
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.width = new_w;
            }
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Tip dropdown.
    let tip_strs = StringList::new(TIP_NAMES);
    let tip_dd = DropDown::builder().model(&tip_strs).hexpand(true).build();
    tip_dd.set_selected(tip_idx(&layer.tip));
    parent.append(&row("Tip", tip_dd.upcast_ref()));

    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        tip_dd.connect_selected_notify(move |dd| {
            let new_t = default_tip_for(dd.selected());
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.tip = new_t;
            }
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Color — alpha multiplier.
    let alpha = SpinButton::with_range(0.0, 2.0, 0.05);
    alpha.set_digits(2);
    alpha.set_value(layer.color.alpha_mult);
    parent.append(&row("Alpha ×", alpha.upcast_ref()));
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        alpha.connect_value_changed(move |s| {
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.color.alpha_mult = s.value();
            }
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Blend.
    let blend_strs = StringList::new(BLEND_NAMES);
    let blend_dd = DropDown::builder()
        .model(&blend_strs)
        .hexpand(true)
        .build();
    blend_dd.set_selected(blend_idx(layer.blend));
    parent.append(&row("Blend", blend_dd.upcast_ref()));
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        blend_dd.connect_selected_notify(move |dd| {
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.blend = blend_from_idx(dd.selected());
            }
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }
}

fn fill_geometry_subparams(
    parent: &GtkBox,
    geo: Geometry,
    editor_state: Rc<RefCell<EditorState>>,
    layer_idx: usize,
    rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) {
    match geo {
        Geometry::Smooth { resample_step_mm } => {
            let s = SpinButton::with_range(0.1, 5.0, 0.1);
            s.set_digits(2);
            s.set_value(resample_step_mm);
            parent.append(&row("Resample step (mm)", s.upcast_ref()));
            commit_geom(s, editor_state, layer_idx, rebuild, |val| {
                Geometry::Smooth { resample_step_mm: val }
            });
        }
        Geometry::Outline { resample_step_mm, smooth_outline } => {
            let s = SpinButton::with_range(0.1, 5.0, 0.1);
            s.set_digits(2);
            s.set_value(resample_step_mm);
            parent.append(&row("Resample step ×", s.upcast_ref()));
            let chk = CheckButton::with_label("Smooth outline");
            chk.set_active(smooth_outline);
            parent.append(&chk);
            // Closures need shared state. Commit on either change.
            let editor_state2 = editor_state.clone();
            let rebuild2 = rebuild.clone();
            let chk2 = chk.clone();
            let s2 = s.clone();
            let commit = Rc::new(move || {
                if let Some(l) = editor_state2
                    .borrow_mut()
                    .brush
                    .layers
                    .get_mut(layer_idx)
                {
                    l.geometry = Geometry::Outline {
                        resample_step_mm: s2.value(),
                        smooth_outline: chk2.is_active(),
                    };
                }
                if let Some(f) = rebuild2.borrow().as_ref().cloned() {
                    f();
                }
            });
            {
                let commit = commit.clone();
                s.connect_value_changed(move |_| commit());
            }
            chk.connect_toggled(move |_| commit());
        }
        Geometry::Scatter { density, spread_mm, falloff, directional_bias_deg } => {
            let d = SpinButton::with_range(1.0, 256.0, 1.0);
            d.set_digits(0);
            d.set_value(density as f64);
            parent.append(&row("Density", d.upcast_ref()));
            let sp = SpinButton::with_range(0.0, 100.0, 0.5);
            sp.set_digits(2);
            sp.set_value(spread_mm);
            parent.append(&row("Spread (mm, 0 = auto)", sp.upcast_ref()));
            let fo = SpinButton::with_range(0.1, 8.0, 0.1);
            fo.set_digits(2);
            fo.set_value(falloff);
            parent.append(&row("Falloff exp", fo.upcast_ref()));
            let cone_chk = CheckButton::with_label("Directional cone");
            cone_chk.set_active(directional_bias_deg.is_some());
            let cone_deg = SpinButton::with_range(0.0, 180.0, 1.0);
            cone_deg.set_digits(1);
            cone_deg.set_value(directional_bias_deg.unwrap_or(35.0));
            parent.append(&cone_chk);
            parent.append(&row("Cone half-angle°", cone_deg.upcast_ref()));

            let editor_state2 = editor_state.clone();
            let rebuild2 = rebuild.clone();
            let (d2, sp2, fo2, cone_chk2, cone_deg2) =
                (d.clone(), sp.clone(), fo.clone(), cone_chk.clone(), cone_deg.clone());
            let commit = Rc::new(move || {
                let dir = if cone_chk2.is_active() {
                    Some(cone_deg2.value())
                } else {
                    None
                };
                if let Some(l) = editor_state2
                    .borrow_mut()
                    .brush
                    .layers
                    .get_mut(layer_idx)
                {
                    l.geometry = Geometry::Scatter {
                        density: d2.value() as u32,
                        spread_mm: sp2.value(),
                        falloff: fo2.value(),
                        directional_bias_deg: dir,
                    };
                }
                if let Some(f) = rebuild2.borrow().as_ref().cloned() {
                    f();
                }
            });
            {
                let commit = commit.clone();
                d.connect_value_changed(move |_| commit());
            }
            {
                let commit = commit.clone();
                sp.connect_value_changed(move |_| commit());
            }
            {
                let commit = commit.clone();
                fo.connect_value_changed(move |_| commit());
            }
            {
                let commit = commit.clone();
                cone_chk.connect_toggled(move |_| commit());
            }
            cone_deg.connect_value_changed(move |_| commit());
        }
        Geometry::DabStamp { step_mult } => {
            let s = SpinButton::with_range(0.1, 8.0, 0.1);
            s.set_digits(2);
            s.set_value(step_mult);
            parent.append(&row("Step ×", s.upcast_ref()));
            commit_geom(s, editor_state, layer_idx, rebuild, |val| {
                Geometry::DabStamp { step_mult: val }
            });
        }
    }
}

/// Single-spinbutton geometry committer (Smooth + DabStamp).
fn commit_geom(
    s: SpinButton,
    editor_state: Rc<RefCell<EditorState>>,
    layer_idx: usize,
    rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    make_geo: impl Fn(f64) -> Geometry + 'static,
) {
    s.connect_value_changed(move |s| {
        if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
            l.geometry = make_geo(s.value());
        }
        if let Some(f) = rebuild.borrow().as_ref().cloned() {
            f();
        }
    });
}

fn fill_width_subparams(
    parent: &GtkBox,
    width: WidthMode,
    editor_state: Rc<RefCell<EditorState>>,
    layer_idx: usize,
    rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) {
    match width {
        WidthMode::Constant { width_mult } => {
            let s = SpinButton::with_range(0.05, 12.0, 0.05);
            s.set_digits(2);
            s.set_value(width_mult);
            parent.append(&row("Width ×", s.upcast_ref()));
            let es = editor_state.clone();
            let rb = rebuild.clone();
            s.connect_value_changed(move |s| {
                if let Some(l) = es.borrow_mut().brush.layers.get_mut(layer_idx) {
                    l.width = WidthMode::Constant { width_mult: s.value() };
                }
                if let Some(f) = rb.borrow().as_ref().cloned() {
                    f();
                }
            });
        }
        WidthMode::ClampedConstant { width_mult, min_mm, max_mm } => {
            let m = SpinButton::with_range(0.05, 12.0, 0.05);
            m.set_digits(2);
            m.set_value(width_mult);
            parent.append(&row("Width ×", m.upcast_ref()));
            let mn = SpinButton::with_range(0.0, 50.0, 0.05);
            mn.set_digits(2);
            mn.set_value(min_mm);
            parent.append(&row("Min (mm)", mn.upcast_ref()));
            let mx = SpinButton::with_range(0.0, 50.0, 0.05);
            mx.set_digits(2);
            mx.set_value(max_mm);
            parent.append(&row("Max (mm)", mx.upcast_ref()));
            let (m2, mn2, mx2) = (m.clone(), mn.clone(), mx.clone());
            let es = editor_state.clone();
            let rb = rebuild.clone();
            let commit = Rc::new(move || {
                if let Some(l) = es.borrow_mut().brush.layers.get_mut(layer_idx) {
                    l.width = WidthMode::ClampedConstant {
                        width_mult: m2.value(),
                        min_mm: mn2.value(),
                        max_mm: mx2.value(),
                    };
                }
                if let Some(f) = rb.borrow().as_ref().cloned() {
                    f();
                }
            });
            {
                let commit = commit.clone();
                m.connect_value_changed(move |_| commit());
            }
            {
                let commit = commit.clone();
                mn.connect_value_changed(move |_| commit());
            }
            mx.connect_value_changed(move |_| commit());
        }
        WidthMode::Pressure { floor, amp } => {
            let fl = SpinButton::with_range(0.0, 1.5, 0.05);
            fl.set_digits(2);
            fl.set_value(floor);
            parent.append(&row("Floor", fl.upcast_ref()));
            let am = SpinButton::with_range(0.0, 4.0, 0.05);
            am.set_digits(2);
            am.set_value(amp);
            parent.append(&row("Amp", am.upcast_ref()));
            let (fl2, am2) = (fl.clone(), am.clone());
            let es = editor_state.clone();
            let rb = rebuild.clone();
            let commit = Rc::new(move || {
                if let Some(l) = es.borrow_mut().brush.layers.get_mut(layer_idx) {
                    l.width = WidthMode::Pressure {
                        floor: fl2.value(),
                        amp: am2.value(),
                    };
                }
                if let Some(f) = rb.borrow().as_ref().cloned() {
                    f();
                }
            });
            {
                let commit = commit.clone();
                fl.connect_value_changed(move |_| commit());
            }
            am.connect_value_changed(move |_| commit());
        }
        WidthMode::DirectionAngled { nib_deg, min_ratio } => {
            let na = SpinButton::with_range(-180.0, 180.0, 1.0);
            na.set_digits(1);
            na.set_value(nib_deg);
            parent.append(&row("Nib angle°", na.upcast_ref()));
            let mr = SpinButton::with_range(0.0, 1.0, 0.05);
            mr.set_digits(2);
            mr.set_value(min_ratio);
            parent.append(&row("Min ratio", mr.upcast_ref()));
            let (na2, mr2) = (na.clone(), mr.clone());
            let es = editor_state.clone();
            let rb = rebuild.clone();
            let commit = Rc::new(move || {
                if let Some(l) = es.borrow_mut().brush.layers.get_mut(layer_idx) {
                    l.width = WidthMode::DirectionAngled {
                        nib_deg: na2.value(),
                        min_ratio: mr2.value(),
                    };
                }
                if let Some(f) = rb.borrow().as_ref().cloned() {
                    f();
                }
            });
            {
                let commit = commit.clone();
                na.connect_value_changed(move |_| commit());
            }
            mr.connect_value_changed(move |_| commit());
        }
        WidthMode::TiltBand { threshold, band_mult, alpha_scale } => {
            let th = SpinButton::with_range(0.0, 1.0, 0.01);
            th.set_digits(2);
            th.set_value(threshold);
            parent.append(&row("Threshold", th.upcast_ref()));
            let bm = SpinButton::with_range(0.0, 32.0, 0.1);
            bm.set_digits(2);
            bm.set_value(band_mult);
            parent.append(&row("Band ×", bm.upcast_ref()));
            let al = SpinButton::with_range(0.0, 2.0, 0.01);
            al.set_digits(2);
            al.set_value(alpha_scale);
            parent.append(&row("Alpha scale", al.upcast_ref()));
            let (th2, bm2, al2) = (th.clone(), bm.clone(), al.clone());
            let es = editor_state.clone();
            let rb = rebuild.clone();
            let commit = Rc::new(move || {
                if let Some(l) = es.borrow_mut().brush.layers.get_mut(layer_idx) {
                    l.width = WidthMode::TiltBand {
                        threshold: th2.value(),
                        band_mult: bm2.value(),
                        alpha_scale: al2.value(),
                    };
                }
                if let Some(f) = rb.borrow().as_ref().cloned() {
                    f();
                }
            });
            {
                let commit = commit.clone();
                th.connect_value_changed(move |_| commit());
            }
            {
                let commit = commit.clone();
                bm.connect_value_changed(move |_| commit());
            }
            al.connect_value_changed(move |_| commit());
        }
    }
}

fn row(label: &str, widget: &gtk4::Widget) -> GtkBox {
    let r = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    let l = Label::builder()
        .label(label)
        .halign(gtk4::Align::Start)
        .width_chars(20)
        .build();
    r.append(&l);
    r.append(widget);
    r
}

// ── Save-as prompt ────────────────────────────────────────────────

fn prompt_save_as(parent: &ApplicationWindow, on_name: impl Fn(String) + 'static) {
    use gtk4::Window;
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Save brush as…")
        .default_width(360)
        .default_height(120)
        .build();
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(14)
        .margin_bottom(14)
        .margin_start(14)
        .margin_end(14)
        .build();
    let entry = Entry::builder().placeholder_text("Brush name").build();
    body.append(&entry);
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build();
    let cancel = Button::with_label("Cancel");
    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");
    row.append(&cancel);
    row.append(&save);
    body.append(&row);
    win.set_child(Some(&body));
    let on_name = Rc::new(on_name);
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    {
        let win = win.clone();
        let entry = entry.clone();
        let on_name = on_name.clone();
        save.connect_clicked(move |_| {
            let text = entry.text().to_string();
            (on_name)(text);
            win.close();
        });
    }
    win.present();
}

// ── Live preview ──────────────────────────────────────────────────
//
// Mini Vello-rendered S-curve in a 360×100 cell. Shows what the
// current brush composition draws before the user commits. Renderer
// is created lazily on first paint (wgpu init is ~1s).

const PREVIEW_W: i32 = 360;
const PREVIEW_H: i32 = 100;

fn build_preview_area(
    editor_state: Rc<RefCell<EditorState>>,
    renderer: Rc<RefCell<Option<VelloRenderer>>>,
    preview_brush: Rc<RefCell<Brush>>,
) -> Frame {
    let frame = Frame::builder()
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(16)
        .margin_end(16)
        .build();
    frame.set_label(Some("Preview"));
    let area = DrawingArea::builder()
        .content_width(PREVIEW_W)
        .content_height(PREVIEW_H)
        .build();
    {
        let renderer = renderer.clone();
        let preview_brush = preview_brush.clone();
        let editor_state = editor_state.clone();
        area.set_draw_func(move |_, ctx, w, h| {
            let dark = editor_state.borrow().rebuilding;
            let _ = dark;
            // Background fill — light cream so the preview reads as a
            // page surface, contrasting with the dark editor chrome.
            ctx.set_source_rgb(0.96, 0.95, 0.92);
            let _ = ctx.paint();

            let mut renderer_slot = renderer.borrow_mut();
            if renderer_slot.is_none() {
                match VelloRenderer::new() {
                    Ok(r) => *renderer_slot = Some(r),
                    Err(e) => {
                        tracing::warn!("preview vello init: {e:?}");
                        draw_preview_init_failure(ctx, w, h);
                        return;
                    }
                }
            }
            let r = renderer_slot.as_mut().unwrap();

            let brush = preview_brush.borrow().clone();
            let strokes = vec![preview_stroke(brush, w as f64, h as f64)];
            let viewport = Viewport {
                center: Point {
                    x: w as f64 * 0.5,
                    y: h as f64 * 0.5,
                },
                zoom: 1.0,
                rotation: 0.0,
            };
            let transform = ViewportTransform::new(viewport, w as f64, h as f64);
            let bg = BackgroundConfig::Blank;
            let page_rect = Rect {
                x: 0.0,
                y: 0.0,
                width: w as f64,
                height: h as f64,
            };
            let bytes = match r.render_rgba(
                &transform,
                &bg,
                page_rect,
                &strokes,
                &HashSet::new(),
                &OverlayState::default(),
                &BrushParams::default(),
                w as u32,
                h as u32,
                |_, _, _| {},
            ) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("preview render: {e:?}");
                    return;
                }
            };
            blit_rgba_to_cairo(ctx, &bytes, w as u32, h as u32);
        });
    }
    frame.set_child(Some(&area));
    frame
}

/// Synthesize a fixed S-curve stroke using `brush` as the recipe.
/// Width / pressure ramp 0.3 → 1.0 → 0.5 across the curve so all
/// pressure-driven WidthModes show a visible taper.
fn preview_stroke(brush: Brush, w: f64, h: f64) -> Stroke {
    let n = 48;
    let mut pts = Vec::with_capacity(n);
    let pad = 18.0;
    let usable_w = (w - pad * 2.0).max(20.0);
    let mid_y = h * 0.5;
    let amp = (h * 0.32).min(usable_w * 0.18);
    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let x = pad + usable_w * t;
        // Two-hump S-curve.
        let y = mid_y - amp * (t * std::f64::consts::TAU).sin();
        // Pressure: 0.25 → 1.0 → 0.5
        let pressure = if t < 0.5 {
            0.25 + 1.5 * t
        } else {
            1.0 - 0.5 * (t - 0.5) * 2.0
        };
        pts.push(StrokePoint {
            x,
            y,
            pressure: pressure as f32,
            tilt_x: 0.0,
            tilt_y: 0.0,
            timestamp_ms: i as u64,
        });
    }
    Stroke {
        id: Uuid::nil(),
        points: pts,
        pen: PenSettings {
            color: JColor {
                r: 30,
                g: 30,
                b: 35,
                a: 255,
            },
            base_width: 6.0,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            brush_style: journal_core::BrushStyle::Pen,
        },
        zoom_at_creation: 1.0,
        bounding_box: Rect {
            x: 0.0,
            y: 0.0,
            width: w,
            height: h,
        },
        brush_recipe: Some(brush),
    }
}

/// Vello produces RGBA8 (R,G,B,A premultiplied). Cairo ARgb32 on
/// little-endian wants BGRA8 in memory. Swap channels in place into a
/// scratch buffer and blit via `ImageSurface::create_for_data`.
fn blit_rgba_to_cairo(ctx: &gtk4::cairo::Context, rgba: &[u8], w: u32, h: u32) {
    use gtk4::cairo;
    let mut bgra = vec![0u8; rgba.len()];
    for px in 0..(rgba.len() / 4) {
        let i = px * 4;
        bgra[i] = rgba[i + 2]; // B
        bgra[i + 1] = rgba[i + 1]; // G
        bgra[i + 2] = rgba[i]; // R
        bgra[i + 3] = rgba[i + 3]; // A
    }
    let stride = cairo::Format::ARgb32.stride_for_width(w).unwrap_or((w * 4) as i32);
    let surface = match cairo::ImageSurface::create_for_data(
        bgra,
        cairo::Format::ARgb32,
        w as i32,
        h as i32,
        stride,
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("preview surface: {e}");
            return;
        }
    };
    let _ = ctx.set_source_surface(&surface, 0.0, 0.0);
    let _ = ctx.paint();
}

fn draw_preview_init_failure(ctx: &gtk4::cairo::Context, w: i32, h: i32) {
    ctx.set_source_rgb(0.6, 0.3, 0.3);
    ctx.move_to(12.0, h as f64 * 0.55);
    let _ = ctx.show_text("(GPU preview unavailable)");
    let _ = ctx;
    let _ = w;
}
