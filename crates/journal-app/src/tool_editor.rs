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
    // User-drawn strokes inside the preview canvas. Each is rendered
    // with the *current* brush so changing layer settings live-updates
    // every existing stroke — the whole point of the preview.
    let preview_strokes: Rc<RefCell<Vec<Stroke>>> = Rc::new(RefCell::new(Vec::new()));
    let preview_in_progress: Rc<RefCell<Option<Stroke>>> = Rc::new(RefCell::new(None));

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
    let dup_btn = Button::with_label("Duplicate");
    let rename_btn = Button::with_label("Rename");
    let delete_btn = Button::with_label("Delete");
    // 2x2 grid keeps the sidebar compact when the editor is narrow —
    // a single row of four buttons overflows on the Framework 12.
    let lib_btn_grid = gtk4::Grid::builder()
        .row_spacing(6)
        .column_spacing(6)
        .column_homogeneous(true)
        .build();
    new_btn.set_hexpand(true);
    dup_btn.set_hexpand(true);
    rename_btn.set_hexpand(true);
    delete_btn.set_hexpand(true);
    lib_btn_grid.attach(&new_btn, 0, 0, 1, 1);
    lib_btn_grid.attach(&dup_btn, 1, 0, 1, 1);
    lib_btn_grid.attach(&rename_btn, 0, 1, 1, 1);
    lib_btn_grid.attach(&delete_btn, 1, 1, 1, 1);
    sidebar.append(&lib_btn_grid);

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
    let remove_layer_btn = Button::with_label("Remove");
    let move_up_btn = Button::from_icon_name("go-up-symbolic");
    move_up_btn.set_tooltip_text(Some("Move layer up"));
    let move_down_btn = Button::from_icon_name("go-down-symbolic");
    move_down_btn.set_tooltip_text(Some("Move layer down"));
    let layer_btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    layer_btn_row.append(&add_layer_btn);
    layer_btn_row.append(&remove_layer_btn);
    layer_btn_row.append(&move_up_btn);
    layer_btn_row.append(&move_down_btn);
    sidebar.append(&layer_btn_row);

    paned.set_start_child(Some(&sidebar));

    // ── Right panel ─────────────────────────────────────────────────
    // Vertical layout: preview pinned at top, scrolled form below.
    let right_root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let (preview_frame, preview_area) = build_preview_area(
        editor_state.clone(),
        state.clone(),
        preview_renderer.clone(),
        preview_brush.clone(),
        preview_strokes.clone(),
        preview_in_progress.clone(),
    );
    right_root.append(&preview_frame);
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
        let right_scroll = right.clone();
        let state_outer = state.clone();
        let rebuild_self = rebuild.clone();
        let preview_area = preview_area.clone();
        let preview_brush = preview_brush.clone();
        let do_rebuild: Rc<dyn Fn()> = Rc::new(move || {
            editor_state.borrow_mut().rebuilding = true;

            // Snapshot scroll position so the right panel doesn't
            // jump to the top on every spinbutton/dropdown edit.
            let scroll_v = right_scroll.vadjustment().value();
            let scroll_h = right_scroll.hadjustment().value();

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

            // Right panel — brush-level header (cursor shape) + per-layer settings.
            build_brush_header(
                &right_body,
                editor_state.clone(),
                rebuild_self.clone(),
            );
            right_body.append(&Separator::new(Orientation::Horizontal));

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

            // Restore scroll position. Defer to idle so GTK has time
            // to recompute the adjustment's upper bound from the new
            // child widgets — setting the value before the layout
            // pass would clamp it back to 0.
            let scroll = right_scroll.clone();
            gtk4::glib::idle_add_local_once(move || {
                let vadj = scroll.vadjustment();
                vadj.set_value(scroll_v.min(vadj.upper() - vadj.page_size()).max(0.0));
                let hadj = scroll.hadjustment();
                hadj.set_value(scroll_h.min(hadj.upper() - hadj.page_size()).max(0.0));
            });

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

    // Duplicate — fork the currently-loaded brush into a new entry
    // in the library with a fresh UUID and " (copy)" appended.
    // Built-ins fork into the library too — useful when the user
    // wants to start from Pen and tweak.
    {
        let editor_state = editor_state.clone();
        let state_outer = state.clone();
        let rebuild = rebuild.clone();
        dup_btn.connect_clicked(move |_| {
            let mut new_brush = editor_state.borrow().brush.clone();
            new_brush.id = Uuid::new_v4();
            new_brush.name = format!("{} (copy)", new_brush.name);
            state_outer.borrow_mut().brush_library.push(new_brush.clone());
            let snap = state_outer.borrow().brush_library.clone();
            if let Err(e) = brush_library::save(&snap) {
                tracing::warn!("brush library save: {e}");
            }
            editor_state.borrow_mut().brush = new_brush;
            editor_state.borrow_mut().selected_layer = 0;
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Rename — only meaningful for custom brushes; built-ins ignore.
    {
        let editor_state = editor_state.clone();
        let state_outer = state.clone();
        let parent_clone = _parent.clone();
        let rebuild = rebuild.clone();
        rename_btn.connect_clicked(move |_| {
            let id = editor_state.borrow().brush.id;
            let is_builtin = brush_library::built_ins().iter().any(|b| b.id == id);
            if is_builtin {
                tracing::info!("rename: cannot rename a built-in brush");
                return;
            }
            let editor_state = editor_state.clone();
            let state_outer = state_outer.clone();
            let rebuild = rebuild.clone();
            prompt_save_as(&parent_clone, move |new_name| {
                let new_name = new_name.trim().to_string();
                if new_name.is_empty() {
                    return;
                }
                editor_state.borrow_mut().brush.name = new_name.clone();
                let id = editor_state.borrow().brush.id;
                let mut s = state_outer.borrow_mut();
                if let Some(b) = s.brush_library.iter_mut().find(|b| b.id == id) {
                    b.name = new_name;
                }
                let snap = s.brush_library.clone();
                drop(s);
                if let Err(e) = brush_library::save(&snap) {
                    tracing::warn!("brush library save: {e}");
                }
                if let Some(f) = rebuild.borrow().as_ref().cloned() {
                    f();
                }
            });
        });
    }

    // Delete — only custom brushes. Removes from library +
    // brushes.toml. Snaps editor to a built-in afterwards so the
    // user isn't staring at a dangling reference.
    {
        let editor_state = editor_state.clone();
        let state_outer = state.clone();
        let rebuild = rebuild.clone();
        delete_btn.connect_clicked(move |_| {
            let id = editor_state.borrow().brush.id;
            let is_builtin = brush_library::built_ins().iter().any(|b| b.id == id);
            if is_builtin {
                tracing::info!("delete: cannot delete a built-in brush");
                return;
            }
            {
                let mut s = state_outer.borrow_mut();
                s.brush_library.retain(|b| b.id != id);
                // Drop any per-tool slot pointing at this brush so
                // tool switching doesn't reload a deleted recipe.
                s.tool_brushes.retain(|_, b| b.id != id);
                if s.active_brush_recipe.as_ref().map(|b| b.id) == Some(id) {
                    s.active_brush_recipe = None;
                }
            }
            let snap = state_outer.borrow().brush_library.clone();
            if let Err(e) = brush_library::save(&snap) {
                tracing::warn!("brush library save: {e}");
            }
            crate::state::persist_tool_state(&state_outer);
            // Snap editor to Pen built-in.
            editor_state.borrow_mut().brush =
                brush_library::built_ins().into_iter().next().unwrap();
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

    // Move layer up — swap selected with previous. Layers render
    // first→last, so "up" in the sidebar is "drawn earlier".
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        move_up_btn.connect_clicked(move |_| {
            let mut s = editor_state.borrow_mut();
            let idx = s.selected_layer;
            if idx == 0 {
                return;
            }
            s.brush.layers.swap(idx, idx - 1);
            s.selected_layer = idx - 1;
            drop(s);
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Move layer down — swap selected with next.
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        move_down_btn.connect_clicked(move |_| {
            let mut s = editor_state.borrow_mut();
            let idx = s.selected_layer;
            if idx + 1 >= s.brush.layers.len() {
                return;
            }
            s.brush.layers.swap(idx, idx + 1);
            s.selected_layer = idx + 1;
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

    // Use this brush — set active recipe, also persist to the
    // current tool's slot so switching back to that tool re-applies.
    {
        let editor_state = editor_state.clone();
        let state_outer = state.clone();
        let on_done = on_done.clone();
        done_btn.connect_clicked(move |_| {
            let brush = editor_state.borrow().brush.clone();
            let mut s = state_outer.borrow_mut();
            s.active_brush_recipe = Some(brush.clone());
            // Brush carries an optional default ink color. Apply
            // it to the active pen so the next stroke uses it.
            if let Some(rgba) = brush.default_color {
                s.pen.color = journal_core::Color {
                    r: rgba[0],
                    g: rgba[1],
                    b: rgba[2],
                    a: rgba[3],
                };
            }
            let tool = s.tool;
            if let Some(key) = crate::tool_settings::tool_key(tool) {
                s.tool_brushes.insert(key.to_string(), brush.clone());
            }
            // Make sure the brush is referenceable across restarts —
            // built-ins are constructed on demand, but custom brushes
            // must live in `brushes.toml` so `resolve_id` can find
            // them next boot.
            let is_builtin = crate::brush_library::built_ins()
                .iter()
                .any(|b| b.id == brush.id);
            if !is_builtin
                && !s.brush_library.iter().any(|b| b.id == brush.id)
            {
                s.brush_library.push(brush.clone());
                let snap = s.brush_library.clone();
                if let Err(e) = crate::brush_library::save(&snap) {
                    tracing::warn!("brush library save: {e}");
                }
            }
            drop(s);
            crate::state::persist_tool_state(&state_outer);
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
        cursor: journal_core::CursorShape::Auto,
        default_color: None,
    }
}

fn default_layer() -> BrushLayer {
    BrushLayer {
        enabled: true,
        geometry: Geometry::Smooth { resample_step_mm: 1.0 },
        width: WidthMode::Pressure { floor: 0.6, amp: 0.4 },
        tip: TipShape::Round,
        tip_scale: 1.0,
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
        Geometry::FanOffset { .. } => "FanOffset",
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

const GEO_NAMES: &[&str] = &["Smooth", "Outline", "Scatter", "DabStamp", "FanOffset"];
const WIDTH_NAMES: &[&str] = &[
    "Constant",
    "ClampedConstant",
    "Pressure",
    "DirectionAngled",
    "TiltBand",
];
const TIP_NAMES: &[&str] = &[
    "Round",
    "Square",
    "FlatNib",
    "Diamond",
    "StarN",
    "Custom polygon",
];
const BLEND_NAMES: &[&str] = &[
    "Normal", "Multiply", "Screen", "Overlay", "Darken", "Lighten", "Erase",
];

fn geom_idx(g: &Geometry) -> u32 {
    match g {
        Geometry::Smooth { .. } => 0,
        Geometry::Outline { .. } => 1,
        Geometry::Scatter { .. } => 2,
        Geometry::DabStamp { .. } => 3,
        Geometry::FanOffset { .. } => 4,
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
        TipShape::Custom { .. } => 5,
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
        4 => Geometry::FanOffset { count: 3, spread_mult: 1.4 },
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
        5 => TipShape::Custom {
            points: default_custom_polygon(),
        },
        _ => TipShape::Round,
    }
}

/// 8-vertex regular polygon for the initial Custom tip — a fair
/// starting point the user can drag points from.
fn default_custom_polygon() -> Vec<(f64, f64)> {
    let n = 8;
    (0..n)
        .map(|i| {
            let theta = (i as f64) * std::f64::consts::TAU / (n as f64) - std::f64::consts::FRAC_PI_2;
            (theta.cos(), theta.sin())
        })
        .collect()
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

// ── Description helpers ──────────────────────────────────────────
//
// Plain-English explanations surfaced beneath each enum dropdown so
// the user knows what the variant they just picked actually does.
// Updated on every selection change via a small label widget.

fn geometry_description(g: &Geometry) -> &'static str {
    match g {
        Geometry::Smooth { .. } => {
            "Smooth — one continuous curve along the path. The most common \
             choice. Pick this for pens, pencils, highlighters, markers, \
             and any brush that should feel like a single trace.\n\n\
             Pick something else when you need: a polygon outline that \
             responds to direction (use Outline) · noisy scatter (use \
             Scatter) · a chain of discrete stamps (use Dab stamp) · \
             fan-bristle effect (use Fan offset)."
        }
        Geometry::Outline { .. } => {
            "Outline — a variable-width filled polygon, offset left and \
             right of the path. Use this for real calligraphy: pair with \
             Direction-angled width to get italic-nib behaviour, or with \
             Pressure for soft brush nibs.\n\n\
             Pick Smooth instead for normal pen-style strokes; Outline \
             is heavier and only pays off when width genuinely varies \
             along the stroke."
        }
        Geometry::Scatter { .. } => {
            "Scatter — N tip stamps per input point at randomised \
             offsets. The mechanism behind spray cans and noisy texture \
             brushes (chalk, charcoal, stipple).\n\n\
             Tweak: `density` for how many dots per point; `spread` for \
             how far they fly; `falloff` for centre-bias; the \
             directional cone biases the spread along the stylus tilt \
             (cone-airbrush feel)."
        }
        Geometry::DabStamp { .. } => {
            "Dab stamp — stamps the tip at fixed intervals along the \
             path. Use this when the tip itself IS the visual signature \
             (custom polygon, leaf, arrow) and you want a chain of those \
             stamps rather than a continuous curve.\n\n\
             Smooth + non-circular tip auto-stamps too, but Dab stamp \
             gives explicit control over the step interval."
        }
        Geometry::FanOffset { .. } => {
            "Fan offset — multiple thin parallel offset strokes spread \
             perpendicular to the path. Reads as bristle hair on a fan \
             brush.\n\n\
             Increase `count` for a denser fan; increase `spread` for a \
             wider fan. Replaces the legacy Paintbrush-Fan tool."
        }
    }
}

fn width_mode_description(w: &WidthMode) -> &'static str {
    match w {
        WidthMode::Constant { .. } => {
            "Constant — width = base × multiplier. Pressure has zero \
             effect.\n\n\
             Pick this for permanent-marker / chunky-tip pens where \
             every stroke should look the same regardless of how the \
             user pressed."
        }
        WidthMode::ClampedConstant { .. } => {
            "Clamped constant — same as Constant, but clipped between \
             a min and a max in millimetres. Used by sharp pencil cores: \
             the line never gets thinner than `min` or thicker than \
             `max` even if the user is at extreme zoom.\n\n\
             Pick over Constant when you want a hard absolute bound on \
             stroke thickness."
        }
        WidthMode::Pressure { .. } => {
            "Pressure — width = base × (floor + amp × pressure). The \
             default for natural pen/pencil/paintbrush feel.\n\n\
             `floor` = how thick a feather-touch stroke is. \
             `amp` = how much extra pressure adds. floor=0.6, amp=0.4 \
             feels like a fountain pen. floor=0, amp=1 is fully \
             pressure-driven."
        }
        WidthMode::DirectionAngled { .. } => {
            "Direction-angled — width modulates by stroke direction \
             relative to a fixed nib axis (`nib°`). The real \
             italic-nib calligraphy formula: down-strokes are thick, \
             cross-strokes are thin.\n\n\
             Only meaningful with Outline geometry; on Smooth it \
             degrades to constant base width. `min ratio` is the \
             thinnest fraction the nib ever shrinks to."
        }
        WidthMode::TiltBand { .. } => {
            "Tilt band — emits *additional* paint only where stylus \
             tilt exceeds `threshold`. Designed to layer on top of a \
             constant-width core (the standard Pencil composition).\n\n\
             By itself it produces nothing visible at low tilt — pair \
             it with another layer that handles the core line. \
             `band×` controls how wide the shading is; `alpha scale` \
             how dark."
        }
    }
}

fn tip_shape_description(t: &TipShape) -> &'static str {
    match t {
        TipShape::Round => {
            "Round — circular tip. Standard pen / pencil feel; works \
             with every Geometry (strokes cleanly, dabs cleanly, \
             scatters cleanly).\n\n\
             Pick over Square when you want soft caps. Pick over \
             Diamond/Star/Custom when you want a normal trace, not a \
             chain of stamps."
        }
        TipShape::Square => {
            "Square — axis-aligned square tip. Sharp corners give a \
             chunky pixel-art feel.\n\n\
             Pick over Round for hard-edge highlighters, blocky \
             markers. Note: when used with Smooth geometry the corners \
             are still applied at stroke caps + joins."
        }
        TipShape::FlatNib { .. } => {
            "Flat nib — rectangular tip rotated by `angle°` and \
             squished by `aspect`. The classic italic / chisel nib.\n\n\
             Pair with Smooth+Pressure for marker calligraphy, or with \
             Outline+Direction-angled for real broad-edge calligraphy. \
             Aspect: 1.0 = square, 0.2 = thin chisel."
        }
        TipShape::Diamond => {
            "Diamond — 4-point rhombus, points up/down/left/right.\n\n\
             Decorative tip — best with Dab stamp geometry so the \
             individual diamonds read clearly. On Smooth the renderer \
             auto-stamps them along the path."
        }
        TipShape::StarN { .. } => {
            "Star N — N-pointed star. `inner ratio` controls how \
             pointy: 0.4 is sharp, 0.7 is soft. Best with Dab stamp \
             so the stars read individually.\n\n\
             On Smooth the renderer stamps stars along the path \
             automatically — picking Star = chain of stars."
        }
        TipShape::Custom { .. } => {
            "Custom polygon — user-designed shape. Drag the orange \
             handles in the editor below to move vertices; +Vertex / \
             −Vertex / Reset buttons control the count.\n\n\
             Pair with Dab stamp for distinct stamps, or Smooth for an \
             auto-stamped chain. The Nib preview swatch above shows the \
             current shape."
        }
    }
}

fn cursor_shape_description(c: &journal_core::CursorShape) -> &'static str {
    use journal_core::CursorShape as CS;
    match c {
        CS::Auto => {
            "Auto — hover cursor mirrors the first layer's tip shape \
             at the brush's active size. The default; works for most \
             brushes since the cursor naturally hints at what will \
             paint.\n\n\
             Switch to Circle if Auto reads as visually noisy (e.g. a \
             star tip makes a star cursor that's hard to aim). \
             Switch to Exact tip for calligraphy nibs where the angle \
             really matters while hovering."
        }
        CS::Circle => {
            "Circle — fixed circular outline regardless of tip.\n\n\
             Calmest, most predictable cursor. Pick this when the tip \
             is non-circular but you'd rather see a clean ring while \
             aiming."
        }
        CS::Oval { .. } => {
            "Oval — ellipse with a given height : width aspect ratio. \
             0.5 = wide and flat (good cursor for flat-nib calligraphy \
             without the angle). 1.0 = circle. >1 = taller than wide.\n\n\
             Pick over Auto when you want the cursor to hint at the \
             nib's aspect without baking in the rotation."
        }
        CS::ExactTip => {
            "Exact tip — cursor mirrors the first layer's tip exactly, \
             rotation and all. Use this for italic / broad-edge \
             calligraphy so the user can see the nib angle while \
             hovering, before the stroke even starts."
        }
        CS::Custom { .. } => {
            "Custom polygon — user-designed cursor outline, \
             independent of the brush's tip. Useful for crosshair / \
             reticle / brush-silhouette cursors that don't match what \
             the brush paints."
        }
    }
}

fn blend_description(b: BlendMode) -> &'static str {
    match b {
        BlendMode::Normal => {
            "Normal — alpha-over compositing. New paint covers what's \
             beneath, weighted by opacity. The default for every \
             everyday tool."
        }
        BlendMode::Multiply => {
            "Multiply — colour × destination. Always darkens; never \
             lightens. The classic highlighter mode: yellow over text \
             stays readable.\n\n\
             Pair with low opacity for translucent overlap. Pure white \
             paint disappears in Multiply (white × anything = anything)."
        }
        BlendMode::Screen => {
            "Screen — inverse-multiply, always lightens. Good for \
             glow effects, light overlays, sparks.\n\n\
             Pure black paint disappears in Screen. Opposite of \
             Multiply."
        }
        BlendMode::Overlay => {
            "Overlay — multiply if the destination is dark, screen if \
             light. High-contrast, exaggerates whatever's beneath. \
             Used for highlight-and-shadow passes on photo-style art."
        }
        BlendMode::Darken => {
            "Darken — keeps whichever is darker per channel. Subtle \
             shadow-builder. Where stroke overlaps existing dark \
             areas, no change; over light areas it deepens them."
        }
        BlendMode::Lighten => {
            "Lighten — keeps whichever is lighter per channel. Subtle \
             highlight-builder. Opposite of Darken."
        }
        BlendMode::Erase => {
            "Erase — subtracts paint (rubs out underlying strokes). \
             Pair with high opacity for a hard erase, low opacity for \
             a soft fade."
        }
    }
}

fn dim(s: &str) -> Label {
    let l = Label::builder()
        .label(s)
        .halign(gtk4::Align::Start)
        .wrap(true)
        .wrap_mode(gtk4::pango::WrapMode::WordChar)
        .max_width_chars(50)
        .build();
    l.add_css_class("dim-label");
    l
}

// ── Brush-level header (name + cursor shape) ────────────────────

const CURSOR_NAMES: &[&str] = &["Auto (matches tip)", "Circle", "Oval", "Exact tip", "Custom polygon"];

fn cursor_idx(c: &journal_core::CursorShape) -> u32 {
    use journal_core::CursorShape as CS;
    match c {
        CS::Auto => 0,
        CS::Circle => 1,
        CS::Oval { .. } => 2,
        CS::ExactTip => 3,
        CS::Custom { .. } => 4,
    }
}
fn default_cursor_for(idx: u32) -> journal_core::CursorShape {
    use journal_core::CursorShape as CS;
    match idx {
        1 => CS::Circle,
        2 => CS::Oval { aspect: 0.5 },
        3 => CS::ExactTip,
        4 => CS::Custom {
            points: default_custom_polygon(),
        },
        _ => CS::Auto,
    }
}

fn build_brush_header(
    parent: &GtkBox,
    editor_state: Rc<RefCell<EditorState>>,
    rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) {
    let title = Label::builder()
        .label("Brush")
        .halign(gtk4::Align::Start)
        .build();
    title.add_css_class("title-4");
    parent.append(&title);

    // Brush name field.
    let name_entry = Entry::builder()
        .text(editor_state.borrow().brush.name.clone())
        .build();
    parent.append(&row("Name", name_entry.upcast_ref()));
    {
        let editor_state = editor_state.clone();
        name_entry.connect_changed(move |e| {
            editor_state.borrow_mut().brush.name = e.text().to_string();
        });
    }

    // Default ink color — applied to the active pen when "Use this
    // brush" runs. Checkbox-gated so users can pick "no default
    // color" (use whatever the toolbar has).
    let color_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    let color_chk = CheckButton::with_label("Set default color");
    color_chk.set_active(editor_state.borrow().brush.default_color.is_some());
    let color_btn = gtk4::ColorDialogButton::builder().build();
    color_btn.set_dialog(&gtk4::ColorDialog::new());
    if let Some(rgba) = editor_state.borrow().brush.default_color {
        color_btn.set_rgba(&gtk4::gdk::RGBA::new(
            rgba[0] as f32 / 255.0,
            rgba[1] as f32 / 255.0,
            rgba[2] as f32 / 255.0,
            rgba[3] as f32 / 255.0,
        ));
    }
    color_btn.set_sensitive(color_chk.is_active());
    color_row.append(&color_chk);
    color_row.append(&color_btn);
    parent.append(&color_row);
    parent.append(&dim(
        "Default color: when this brush is applied via \"Use this brush\", \
         the pen color snaps to this RGBA value. Off = keep whatever \
         color the toolbar has now.",
    ));
    {
        let editor_state = editor_state.clone();
        let color_btn_c = color_btn.clone();
        let rebuild = rebuild.clone();
        color_chk.connect_toggled(move |chk| {
            color_btn_c.set_sensitive(chk.is_active());
            if !chk.is_active() {
                editor_state.borrow_mut().brush.default_color = None;
            } else {
                let r = color_btn_c.rgba();
                editor_state.borrow_mut().brush.default_color = Some([
                    (r.red() * 255.0).clamp(0.0, 255.0) as u8,
                    (r.green() * 255.0).clamp(0.0, 255.0) as u8,
                    (r.blue() * 255.0).clamp(0.0, 255.0) as u8,
                    (r.alpha() * 255.0).clamp(0.0, 255.0) as u8,
                ]);
            }
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }
    {
        let editor_state = editor_state.clone();
        let color_chk_c = color_chk.clone();
        let rebuild = rebuild.clone();
        color_btn.connect_rgba_notify(move |btn| {
            if !color_chk_c.is_active() {
                return;
            }
            let r = btn.rgba();
            editor_state.borrow_mut().brush.default_color = Some([
                (r.red() * 255.0).clamp(0.0, 255.0) as u8,
                (r.green() * 255.0).clamp(0.0, 255.0) as u8,
                (r.blue() * 255.0).clamp(0.0, 255.0) as u8,
                (r.alpha() * 255.0).clamp(0.0, 255.0) as u8,
            ]);
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Cursor shape dropdown + description + sub-params.
    let cursor_strs = StringList::new(CURSOR_NAMES);
    let cursor_dd = DropDown::builder()
        .model(&cursor_strs)
        .hexpand(true)
        .build();
    cursor_dd.set_selected(cursor_idx(&editor_state.borrow().brush.cursor));
    cursor_dd.set_tooltip_text(Some(
        "Outline shown on the canvas while hovering with this brush. \
         Auto = match the tip. Circle = always a clean ring. Oval = \
         flat ellipse. Exact tip = mirror the tip including rotation. \
         Custom = user-designed cursor.",
    ));
    parent.append(&row("Cursor", cursor_dd.upcast_ref()));
    parent.append(&dim(cursor_shape_description(
        &editor_state.borrow().brush.cursor,
    )));

    let cursor_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_start(20)
        .build();
    fill_cursor_subparams(
        &cursor_box,
        editor_state.borrow().brush.cursor.clone(),
        editor_state.clone(),
        rebuild.clone(),
    );
    parent.append(&cursor_box);

    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        cursor_dd.connect_selected_notify(move |dd| {
            let new_c = default_cursor_for(dd.selected());
            editor_state.borrow_mut().brush.cursor = new_c;
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }
}

fn fill_cursor_subparams(
    parent: &GtkBox,
    cursor: journal_core::CursorShape,
    editor_state: Rc<RefCell<EditorState>>,
    rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) {
    use journal_core::CursorShape as CS;
    match cursor {
        CS::Auto | CS::Circle | CS::ExactTip => {
            let l = Label::builder()
                .label("(no extra parameters)")
                .halign(gtk4::Align::Start)
                .build();
            l.add_css_class("dim-label");
            parent.append(&l);
        }
        CS::Oval { aspect } => {
            let s = SpinButton::with_range(0.05, 4.0, 0.05);
            s.set_digits(2);
            s.set_value(aspect);
            parent.append(&row("Aspect (h:w)", s.upcast_ref()));
            let editor_state = editor_state.clone();
            let rebuild = rebuild.clone();
            s.connect_value_changed(move |s| {
                editor_state.borrow_mut().brush.cursor =
                    CS::Oval { aspect: s.value() };
                if let Some(f) = rebuild.borrow().as_ref().cloned() {
                    f();
                }
            });
        }
        CS::Custom { points } => {
            // Reuse the polygon editor — but it's currently
            // hard-wired to write into `layer.tip`. Build a
            // dedicated cursor variant that writes into
            // `brush.cursor`.
            let editor = polygon_editor_cursor(points, editor_state.clone(), rebuild.clone());
            parent.append(&editor);
        }
    }
}

/// Variant of `polygon_editor` that writes into `brush.cursor`
/// (not a layer's tip). Same interactive UI.
fn polygon_editor_cursor(
    initial: Vec<(f64, f64)>,
    editor_state: Rc<RefCell<EditorState>>,
    rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) -> GtkBox {
    use journal_core::CursorShape as CS;
    const CANVAS_SIZE: i32 = 220;
    const HANDLE_R: f64 = 6.0;
    let outer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .build();
    let area = DrawingArea::builder()
        .content_width(CANVAS_SIZE)
        .content_height(CANVAS_SIZE)
        .build();
    area.set_can_target(true);

    let pts: Rc<RefCell<Vec<(f64, f64)>>> = Rc::new(RefCell::new(initial));
    let drag_idx: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));

    let to_canvas = |ux: f64, uy: f64| -> (f64, f64) {
        let half = CANVAS_SIZE as f64 * 0.5;
        let r = half - 14.0;
        (half + ux * r, half + uy * r)
    };
    let to_unit = move |cx: f64, cy: f64| -> (f64, f64) {
        let half = CANVAS_SIZE as f64 * 0.5;
        let r = half - 14.0;
        ((cx - half) / r, (cy - half) / r)
    };

    {
        let pts = pts.clone();
        area.set_draw_func(move |_, ctx, _, _| {
            ctx.set_source_rgb(0.97, 0.96, 0.93);
            let _ = ctx.paint();
            ctx.set_source_rgba(0.4, 0.4, 0.45, 0.3);
            ctx.set_line_width(1.0);
            ctx.move_to(0.0, CANVAS_SIZE as f64 * 0.5);
            ctx.line_to(CANVAS_SIZE as f64, CANVAS_SIZE as f64 * 0.5);
            ctx.move_to(CANVAS_SIZE as f64 * 0.5, 0.0);
            ctx.line_to(CANVAS_SIZE as f64 * 0.5, CANVAS_SIZE as f64);
            let _ = ctx.stroke();

            let p = pts.borrow();
            if p.len() >= 2 {
                ctx.set_source_rgba(0.55, 0.3, 0.1, 0.3);
                let (x0, y0) = to_canvas(p[0].0, p[0].1);
                ctx.move_to(x0, y0);
                for v in p.iter().skip(1) {
                    let (x, y) = to_canvas(v.0, v.1);
                    ctx.line_to(x, y);
                }
                ctx.close_path();
                let _ = ctx.fill_preserve();
                ctx.set_source_rgba(0.4, 0.2, 0.05, 1.0);
                ctx.set_line_width(1.5);
                let _ = ctx.stroke();
            }
            ctx.set_source_rgb(0.85, 0.45, 0.2);
            for v in p.iter() {
                let (x, y) = to_canvas(v.0, v.1);
                ctx.arc(x, y, HANDLE_R, 0.0, std::f64::consts::TAU);
                let _ = ctx.fill();
            }
        });
    }

    {
        let area_widget: gtk4::Widget = area.clone().upcast();
        let g = gtk4::GestureDrag::new();
        g.set_button(gtk4::gdk::BUTTON_PRIMARY);
        let pts_d = pts.clone();
        let drag_idx_d = drag_idx.clone();
        let area_clone = area.clone();
        g.connect_drag_begin(move |_, x, y| {
            let p = pts_d.borrow();
            let mut hit = None;
            for (i, v) in p.iter().enumerate() {
                let (cx, cy) = to_canvas(v.0, v.1);
                let dx = cx - x;
                let dy = cy - y;
                if (dx * dx + dy * dy).sqrt() < HANDLE_R * 1.6 {
                    hit = Some(i);
                    break;
                }
            }
            *drag_idx_d.borrow_mut() = hit;
            area_clone.queue_draw();
        });
        let pts_d = pts.clone();
        let drag_idx_d = drag_idx.clone();
        let editor_state2 = editor_state.clone();
        let area_clone = area.clone();
        // Mid-drag: only mutate brush + repaint cursor canvas (see
        // `polygon_editor` for the rationale — calling rebuild
        // mid-drag tears down the widget the drag is attached to).
        g.connect_drag_update(move |g, dx, dy| {
            let Some((sx, sy)) = g.start_point() else { return };
            let cx = sx + dx;
            let cy = sy + dy;
            let (ux, uy) = to_unit(cx, cy);
            let ux = ux.clamp(-1.0, 1.0);
            let uy = uy.clamp(-1.0, 1.0);
            let i = match *drag_idx_d.borrow() {
                Some(i) => i,
                None => return,
            };
            {
                let mut p = pts_d.borrow_mut();
                if let Some(v) = p.get_mut(i) {
                    *v = (ux, uy);
                }
            }
            editor_state2.borrow_mut().brush.cursor = CS::Custom {
                points: pts_d.borrow().clone(),
            };
            area_clone.queue_draw();
        });
        let drag_idx_d = drag_idx.clone();
        let rebuild2 = rebuild.clone();
        g.connect_drag_end(move |_, _, _| {
            *drag_idx_d.borrow_mut() = None;
            if let Some(f) = rebuild2.borrow().as_ref().cloned() {
                f();
            }
        });
        area_widget.add_controller(g);
    }
    outer.append(&area);

    let btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(gtk4::Align::End)
        .build();
    let add_btn = Button::with_label("+ Vertex");
    let rem_btn = Button::with_label("− Vertex");
    btn_row.append(&rem_btn);
    btn_row.append(&add_btn);
    outer.append(&btn_row);

    {
        let pts_a = pts.clone();
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        let area_clone = area.clone();
        add_btn.connect_clicked(move |_| {
            let new_pts = {
                let mut p = pts_a.borrow().clone();
                if p.is_empty() {
                    p.push((1.0, 0.0));
                } else {
                    let last = *p.last().unwrap();
                    let first = p[0];
                    let mid = ((last.0 + first.0) * 0.5, (last.1 + first.1) * 0.5);
                    p.push(mid);
                }
                p
            };
            *pts_a.borrow_mut() = new_pts.clone();
            editor_state.borrow_mut().brush.cursor = CS::Custom { points: new_pts };
            area_clone.queue_draw();
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }
    {
        let pts_r = pts.clone();
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        let area_clone = area.clone();
        rem_btn.connect_clicked(move |_| {
            let new_pts = {
                let mut p = pts_r.borrow().clone();
                if p.len() > 3 {
                    p.pop();
                }
                p
            };
            *pts_r.borrow_mut() = new_pts.clone();
            editor_state.borrow_mut().brush.cursor = CS::Custom { points: new_pts };
            area_clone.queue_draw();
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    outer
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
    geo_dd.set_tooltip_text(Some(
        "How the layer's path is emitted onto the page. Smooth = single \
         curve. Outline = variable-width polygon. Scatter = sprayed \
         stamps. Dab = chain of stamps. Fan = parallel offset bristles.",
    ));
    parent.append(&row("Geometry", geo_dd.upcast_ref()));
    parent.append(&dim(geometry_description(&layer.geometry)));

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
    width_dd.set_tooltip_text(Some(
        "How each emitted stamp/sample is widened. Constant = ignore \
         pressure. Pressure = scale by stylus pressure. \
         Direction-angled = italic-nib calligraphy. Tilt band = \
         pencil-shading overlay.",
    ));
    parent.append(&row("Width", width_dd.upcast_ref()));
    parent.append(&dim(width_mode_description(&layer.width)));

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

    // Tip dropdown + presets row + nib swatch + sub-params.
    let tip_strs = StringList::new(TIP_NAMES);
    let tip_dd = DropDown::builder().model(&tip_strs).hexpand(true).build();
    tip_dd.set_selected(tip_idx(&layer.tip));
    tip_dd.set_tooltip_text(Some(
        "Shape of the stamp emitted at each point. Round + Square \
         stroke as continuous curves; FlatNib / Diamond / Star / \
         Custom auto-stamp along the path so the shape is visible.",
    ));
    parent.append(&row("Tip", tip_dd.upcast_ref()));
    parent.append(&dim(tip_shape_description(&layer.tip)));

    // Nib preview swatch — small Cairo canvas drawing the current
    // tip polygon at fixed scale so the user can see what shape
    // they're picking.
    parent.append(&build_nib_preview(layer.tip.clone()));

    // Tip size multiplier — independent from line width. Lets users
    // build "thin pen line that paints big stars" by combining a
    // small `width.Constant` with a high `tip_scale`.
    let tip_scale_spin = SpinButton::with_range(0.05, 50.0, 0.1);
    tip_scale_spin.set_digits(2);
    tip_scale_spin.set_value(layer.tip_scale);
    tip_scale_spin.set_tooltip_text(Some(
        "Multiplier on the tip stamp size, applied AFTER the Width \
         formula. 1.0 = same size as the Width says. 5.0 = stamps 5× \
         bigger. Doesn't affect Round/Square continuous-stroke paths \
         (those follow Width directly).",
    ));
    parent.append(&row("Tip size ×", tip_scale_spin.upcast_ref()));
    parent.append(&dim(
        "Decouples stamp size from line width. Set Width to a thin \
         Constant (e.g. 0.3) and Tip size to a big number (e.g. 8) to \
         get a thin path that drops giant stamps along it.",
    ));
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        tip_scale_spin.connect_value_changed(move |s| {
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.tip_scale = s.value();
            }
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    // Nib preset shortcut — pick a curated TipShape (Italic 45°,
    // Star, Leaf, etc.) without touching the dropdown.
    let presets = journal_core::brush::nib_presets();
    let preset_names: Vec<&str> = presets.iter().map(|(n, _)| *n).collect();
    let preset_strs = StringList::new(&preset_names);
    let preset_dd = DropDown::builder().model(&preset_strs).hexpand(true).build();
    preset_dd.set_selected(0);
    preset_dd.set_tooltip_text(Some(
        "Quick-pick a curated tip shape. Selecting a preset \
         overwrites the Tip dropdown above with the matching shape — \
         angle, aspect, points all set for you.",
    ));
    parent.append(&row("Nib preset", preset_dd.upcast_ref()));
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        let presets = presets.clone();
        preset_dd.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            if let Some((_, shape)) = presets.get(idx) {
                if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                    l.tip = shape.clone();
                }
                if let Some(f) = rebuild.borrow().as_ref().cloned() {
                    f();
                }
            }
        });
    }

    // Sub-params for the current Tip variant — angle/aspect for
    // FlatNib, points/inner_ratio for StarN, polygon editor for
    // Custom. Round/Square/Diamond have nothing to tune.
    let tip_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_start(20)
        .build();
    fill_tip_subparams(
        &tip_box,
        layer.tip.clone(),
        editor_state.clone(),
        layer_idx,
        rebuild.clone(),
    );
    parent.append(&tip_box);

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

    // Color — alpha multiplier + hue shift (degrees, -180..180).
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

    let hue = SpinButton::with_range(-180.0, 180.0, 1.0);
    hue.set_digits(1);
    hue.set_value(layer.color.hue_shift_deg);
    parent.append(&row("Hue shift°", hue.upcast_ref()));
    {
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        hue.connect_value_changed(move |s| {
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.color.hue_shift_deg = s.value();
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
    blend_dd.set_tooltip_text(Some(
        "How this layer's paint mixes with what's already on the page. \
         Normal = cover. Multiply = darken (highlighter). Screen = \
         lighten. Erase = subtract.",
    ));
    parent.append(&row("Blend", blend_dd.upcast_ref()));
    parent.append(&dim(blend_description(layer.blend)));
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

/// Tiny Cairo canvas that draws the current TipShape so the user
/// can see what shape they just picked. Sits below the Tip dropdown.
fn build_nib_preview(tip: TipShape) -> GtkBox {
    const SIZE: i32 = 96;
    let outer = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_start(20)
        .build();
    let label = Label::builder()
        .label("Nib preview")
        .halign(gtk4::Align::Start)
        .build();
    label.add_css_class("dim-label");
    outer.append(&label);
    let area = DrawingArea::builder()
        .content_width(SIZE)
        .content_height(SIZE)
        .build();
    let tip_rc = Rc::new(tip);
    {
        let tip_rc = tip_rc.clone();
        area.set_draw_func(move |_, ctx, w, h| {
            ctx.set_source_rgb(0.96, 0.95, 0.92);
            let _ = ctx.paint();
            // Crosshair.
            ctx.set_source_rgba(0.55, 0.55, 0.6, 0.25);
            ctx.set_line_width(0.6);
            ctx.move_to(0.0, h as f64 * 0.5);
            ctx.line_to(w as f64, h as f64 * 0.5);
            ctx.move_to(w as f64 * 0.5, 0.0);
            ctx.line_to(w as f64 * 0.5, h as f64);
            let _ = ctx.stroke();
            // Tip polygon at fixed radius (40% of canvas).
            let cx = w as f64 * 0.5;
            let cy = h as f64 * 0.5;
            let r = (w.min(h) as f64) * 0.40;
            ctx.set_source_rgba(0.15, 0.20, 0.45, 0.85);
            draw_tip_to_cairo(ctx, &tip_rc, (cx, cy), r);
        });
    }
    outer.append(&area);
    outer
}

/// Cairo equivalent of `vello_renderer::tip_polygon` — kept here so
/// the editor can draw nib previews without booting a Vello render
/// pipeline for every preview swatch. Stays in sync with
/// `tip_polygon` semantics.
fn draw_tip_to_cairo(
    ctx: &gtk4::cairo::Context,
    tip: &TipShape,
    center: (f64, f64),
    scale: f64,
) {
    let (cx, cy) = center;
    match tip {
        TipShape::Round => {
            ctx.arc(cx, cy, scale, 0.0, std::f64::consts::TAU);
            let _ = ctx.fill();
        }
        TipShape::Square => {
            ctx.rectangle(cx - scale, cy - scale, scale * 2.0, scale * 2.0);
            let _ = ctx.fill();
        }
        TipShape::Diamond => {
            ctx.move_to(cx, cy - scale);
            ctx.line_to(cx + scale, cy);
            ctx.line_to(cx, cy + scale);
            ctx.line_to(cx - scale, cy);
            ctx.close_path();
            let _ = ctx.fill();
        }
        TipShape::FlatNib { angle_deg, aspect } => {
            let a = angle_deg.to_radians();
            let cos = a.cos();
            let sin = a.sin();
            let half_long = scale;
            let half_short = scale * aspect.max(0.05);
            let pts: [(f64, f64); 4] = [
                (-half_long, -half_short),
                (half_long, -half_short),
                (half_long, half_short),
                (-half_long, half_short),
            ];
            for (i, (x, y)) in pts.iter().enumerate() {
                let rx = x * cos - y * sin;
                let ry = x * sin + y * cos;
                if i == 0 {
                    ctx.move_to(cx + rx, cy + ry);
                } else {
                    ctx.line_to(cx + rx, cy + ry);
                }
            }
            ctx.close_path();
            let _ = ctx.fill();
        }
        TipShape::StarN { points, inner_ratio } => {
            let n = (*points as usize).max(3);
            for i in 0..(n * 2) {
                let theta = (i as f64) * std::f64::consts::PI / (n as f64);
                let r = if i % 2 == 0 {
                    scale
                } else {
                    scale * inner_ratio
                };
                let x = cx + r * theta.cos();
                let y = cy + r * theta.sin();
                if i == 0 {
                    ctx.move_to(x, y);
                } else {
                    ctx.line_to(x, y);
                }
            }
            ctx.close_path();
            let _ = ctx.fill();
        }
        TipShape::Custom { points } => {
            if points.len() < 3 {
                ctx.arc(cx, cy, scale, 0.0, std::f64::consts::TAU);
                let _ = ctx.fill();
                return;
            }
            for (i, (ux, uy)) in points.iter().enumerate() {
                let x = cx + ux * scale;
                let y = cy + uy * scale;
                if i == 0 {
                    ctx.move_to(x, y);
                } else {
                    ctx.line_to(x, y);
                }
            }
            ctx.close_path();
            let _ = ctx.fill();
        }
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
            s.set_tooltip_text(Some(
                "Distance between resampled path points in mm. Lower = \
                 smoother curve at the cost of more GPU work.",
            ));
            parent.append(&row("Resample step (mm)", s.upcast_ref()));
            parent.append(&dim(
                "Resample step: how often the smooth curve is sampled \
                 along its length. Default 1mm reads as a continuous \
                 line. 0.3 mm = silky smooth but slower; 3 mm = \
                 visibly faceted (only useful for stamp-along chains \
                 with non-circular tips).",
            ));
            commit_geom(s, editor_state, layer_idx, rebuild, |val| {
                Geometry::Smooth { resample_step_mm: val }
            });
        }
        Geometry::Outline { resample_step_mm, smooth_outline } => {
            let s = SpinButton::with_range(0.1, 5.0, 0.1);
            s.set_digits(2);
            s.set_value(resample_step_mm);
            s.set_tooltip_text(Some(
                "Distance between offset polygon vertices, as a \
                 multiple of the brush base width. Lower = silkier \
                 outline; higher = chunkier polygon edges.",
            ));
            parent.append(&row("Resample step ×", s.upcast_ref()));
            parent.append(&dim(
                "Resample step ×: spacing between the polygon's left/\
                 right offset vertices, scaled by the base width. \
                 0.5 = legacy calligraphy default. Drop to 0.25 for \
                 silky variable-width outlines; raise to 1.0+ for \
                 chunky faceted edges.",
            ));
            let chk = CheckButton::with_label("Smooth outline");
            chk.set_active(smooth_outline);
            chk.set_tooltip_text(Some(
                "On (default): connect outline vertices with quadratic \
                 curves so the polygon reads as a continuous nib trace. \
                 Off: straight line segments — rigid, polygonal, \
                 retro-feeling.",
            ));
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
            // Density — number of stamps per input point.
            let d = SpinButton::with_range(1.0, 256.0, 1.0);
            d.set_digits(0);
            d.set_value(density as f64);
            d.set_tooltip_text(Some(
                "How many stamp tips are dropped per input point. \
                 Higher = denser cloud, more GPU work.",
            ));
            parent.append(&row("Density", d.upcast_ref()));
            parent.append(&dim(
                "Density: stamps per input point. Spray-can default 36. \
                 Try 6–12 for stipple/sketchy texture, 60+ for thick \
                 airbrush coverage. Costs scale linearly — high values \
                 on long strokes can stutter on slower hardware.",
            ));

            // Spread.
            let sp = SpinButton::with_range(0.0, 100.0, 0.5);
            sp.set_digits(2);
            sp.set_value(spread_mm);
            sp.set_tooltip_text(Some(
                "Radius of the scatter cloud, in mm. 0 = auto (uses \
                 half the brush base width).",
            ));
            parent.append(&row("Spread (mm)", sp.upcast_ref()));
            parent.append(&dim(
                "Spread: how far stamps fly from the input point. 0 = \
                 auto (half the brush base width — the legacy spray-can \
                 default). Set explicitly for a fixed-radius cloud \
                 regardless of brush size — handy for stipple textures \
                 that shouldn't grow when the user picks a thicker pen.",
            ));

            // Falloff exponent.
            let fo = SpinButton::with_range(0.1, 8.0, 0.1);
            fo.set_digits(2);
            fo.set_value(falloff);
            fo.set_tooltip_text(Some(
                "Distribution exponent. 1 = uniform across the disc. \
                 >1 biases stamps toward the centre; <1 pushes them to \
                 the edge.",
            ));
            parent.append(&row("Falloff exp", fo.upcast_ref()));
            parent.append(&dim(
                "Falloff: how stamps cluster within the spread radius. \
                 Each stamp's distance from the point is `r_unit ^ \
                 exp` × Spread, where r_unit is uniform 0..1.\n\n\
                 1.0 = uniform disc (sparse centre, lots at the edge).\n\
                 2.0 = centre-biased (legacy default — looks like a \
                 spray can).\n\
                 4.0+ = tight bullseye (most stamps near the input).\n\
                 0.5 = ring-like (most stamps near the edge).",
            ));

            // Directional cone.
            let cone_chk = CheckButton::with_label("Directional cone");
            cone_chk.set_active(directional_bias_deg.is_some());
            cone_chk.set_tooltip_text(Some(
                "Bias scatter direction along the stylus tilt instead \
                 of going 360°. Off = uniform disc (default). \
                 On = airbrush-cone feel.",
            ));
            parent.append(&cone_chk);
            parent.append(&dim(
                "Directional cone: instead of scattering uniformly in \
                 every direction, restrict stamps to a wedge. The \
                 wedge points along the stylus tilt vector — tilt the \
                 pen and the spray angles that way (real airbrush \
                 behaviour).",
            ));

            // Cone half-angle.
            let cone_deg = SpinButton::with_range(0.0, 180.0, 1.0);
            cone_deg.set_digits(1);
            cone_deg.set_value(directional_bias_deg.unwrap_or(35.0));
            cone_deg.set_tooltip_text(Some(
                "Cone half-angle in degrees. Tighter = more focused \
                 stream. Only used when Directional cone is on.",
            ));
            parent.append(&row("Cone half-angle°", cone_deg.upcast_ref()));
            parent.append(&dim(
                "Cone half-angle: how wide the directional spray opens. \
                 5° = laser-thin stream. 35° = legacy airbrush default. \
                 90° = half the disc. 180° = full circle (same as \
                 turning the cone off).",
            ));

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
            s.set_tooltip_text(Some(
                "Distance between stamps as a multiple of base width. \
                 1.0 = stamps just touching. <1 = overlapping. >1 = \
                 gaps between stamps.",
            ));
            parent.append(&row("Step ×", s.upcast_ref()));
            parent.append(&dim(
                "Step ×: stamp spacing along the path, scaled by the \
                 brush base width. 0.4–0.6 reads as a continuous trace. \
                 1.0 = stamps barely touching. 2.0+ = visibly discrete \
                 stamps (chain of stars / arrows / leaves).",
            ));
            commit_geom(s, editor_state, layer_idx, rebuild, |val| {
                Geometry::DabStamp { step_mult: val }
            });
        }
        Geometry::FanOffset { count, spread_mult } => {
            let c = SpinButton::with_range(2.0, 16.0, 1.0);
            c.set_digits(0);
            c.set_value(count as f64);
            c.set_tooltip_text(Some(
                "Number of parallel offset bristle strokes. More = \
                 denser fan, more GPU work.",
            ));
            parent.append(&row("Tine count", c.upcast_ref()));
            parent.append(&dim(
                "Tine count: how many parallel offset strokes the fan \
                 emits. 3 = legacy default. 6+ reads as dense bristle \
                 hair. 2 makes a thin parallel-pair (railroad tracks).",
            ));
            let sp = SpinButton::with_range(0.0, 6.0, 0.05);
            sp.set_digits(2);
            sp.set_value(spread_mult);
            sp.set_tooltip_text(Some(
                "Total fan width perpendicular to the path, as a \
                 multiple of the brush base width.",
            ));
            parent.append(&row("Spread ×", sp.upcast_ref()));
            parent.append(&dim(
                "Spread ×: total fan width perpendicular to the stroke, \
                 scaled by base width. 1.0 = fan equals one stroke \
                 width. 1.4 = legacy default. 3+ = wide rake-like \
                 spread.",
            ));
            let (c2, sp2) = (c.clone(), sp.clone());
            let editor_state2 = editor_state.clone();
            let rebuild2 = rebuild.clone();
            let commit = Rc::new(move || {
                if let Some(l) = editor_state2
                    .borrow_mut()
                    .brush
                    .layers
                    .get_mut(layer_idx)
                {
                    l.geometry = Geometry::FanOffset {
                        count: c2.value() as u32,
                        spread_mult: sp2.value(),
                    };
                }
                if let Some(f) = rebuild2.borrow().as_ref().cloned() {
                    f();
                }
            });
            {
                let commit = commit.clone();
                c.connect_value_changed(move |_| commit());
            }
            sp.connect_value_changed(move |_| commit());
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
            s.set_tooltip_text(Some(
                "Multiplier on the brush base width. 1.0 = exactly the \
                 base width. 0.5 = half. 2.0 = double.",
            ));
            parent.append(&row("Width ×", s.upcast_ref()));
            parent.append(&dim(
                "Width ×: stroke thickness as a multiple of the brush \
                 base width. Pressure has no effect — every stroke \
                 looks the same regardless of how the user pressed.",
            ));
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
            m.set_tooltip_text(Some("Multiplier on the brush base width."));
            parent.append(&row("Width ×", m.upcast_ref()));
            let mn = SpinButton::with_range(0.0, 50.0, 0.05);
            mn.set_digits(2);
            mn.set_value(min_mm);
            mn.set_tooltip_text(Some(
                "Minimum stroke width in mm. The line never gets thinner \
                 than this even at high zoom-out.",
            ));
            parent.append(&row("Min (mm)", mn.upcast_ref()));
            let mx = SpinButton::with_range(0.0, 50.0, 0.05);
            mx.set_digits(2);
            mx.set_value(max_mm);
            mx.set_tooltip_text(Some(
                "Maximum stroke width in mm. The line never gets thicker \
                 than this even at extreme zoom-in.",
            ));
            parent.append(&row("Max (mm)", mx.upcast_ref()));
            parent.append(&dim(
                "Clamped constant: width = base × multiplier, then \
                 clipped between Min and Max in mm. Pencil cores live \
                 here so the line stays sharp regardless of zoom.",
            ));
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
            fl.set_tooltip_text(Some(
                "Width fraction at zero pressure. 0 = invisible at \
                 zero touch. 0.6 = solid even with a feather-light \
                 stylus.",
            ));
            parent.append(&row("Floor", fl.upcast_ref()));
            let am = SpinButton::with_range(0.0, 4.0, 0.05);
            am.set_digits(2);
            am.set_value(amp);
            am.set_tooltip_text(Some(
                "How much extra width pressure adds. 0 = pressure \
                 has no effect. 1 = max pressure doubles the line \
                 (when floor=1).",
            ));
            parent.append(&row("Amp", am.upcast_ref()));
            parent.append(&dim(
                "Pressure formula: width = base × (Floor + Amp × \
                 pressure). Tune both:\n\
                 • Floor 0.6 + Amp 0.4 = fountain pen feel (always \
                 visible, light pressure tweak).\n\
                 • Floor 0 + Amp 1 = fully pressure-driven (invisible \
                 at zero touch, full width at max press).\n\
                 • Floor 1 + Amp 0 = same as Constant ×1.",
            ));
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
            na.set_tooltip_text(Some(
                "Orientation of the nib axis. 0° = horizontal nib. \
                 45° = standard italic (down-strokes thick). 90° = \
                 vertical nib (cross-strokes thick).",
            ));
            parent.append(&row("Nib angle°", na.upcast_ref()));
            let mr = SpinButton::with_range(0.0, 1.0, 0.05);
            mr.set_digits(2);
            mr.set_value(min_ratio);
            mr.set_tooltip_text(Some(
                "Minimum width fraction perpendicular to the nib. \
                 0 = nib goes infinitely thin at the wrong direction \
                 (sharp italic). 1 = no direction effect (constant).",
            ));
            parent.append(&row("Min ratio", mr.upcast_ref()));
            parent.append(&dim(
                "Direction-angled: real italic-nib formula. The line \
                 is widest when stroked along the nib axis, thinnest \
                 perpendicular to it.\n\n\
                 • Nib angle 45° + min 0.18 = classic broad-edge \
                 calligraphy.\n\
                 • Nib angle 0° + min 0.5 = subtle horizontal-bias \
                 marker.\n\n\
                 Only meaningful with Outline geometry; on Smooth \
                 it falls back to a constant-width stroke.",
            ));
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
            th.set_tooltip_text(Some(
                "Minimum stylus tilt (0..1) before this layer emits \
                 anything. Below the threshold = no shading. 0.12 = \
                 legacy pencil default.",
            ));
            parent.append(&row("Threshold", th.upcast_ref()));
            let bm = SpinButton::with_range(0.0, 32.0, 0.1);
            bm.set_digits(2);
            bm.set_value(band_mult);
            bm.set_tooltip_text(Some(
                "How much wider the tilt band is than the core. 8 = \
                 legacy pencil default. Higher = bigger smear when \
                 the user lays the stylus down.",
            ));
            parent.append(&row("Band ×", bm.upcast_ref()));
            let al = SpinButton::with_range(0.0, 2.0, 0.01);
            al.set_digits(2);
            al.set_value(alpha_scale);
            al.set_tooltip_text(Some(
                "Alpha multiplier for the tilt band. 0.22 = legacy \
                 pencil default (subtle). 0.5+ = darker, more \
                 graphite-rich shading.",
            ));
            parent.append(&row("Alpha scale", al.upcast_ref()));
            parent.append(&dim(
                "Tilt band emits *additional* per-segment paint only \
                 where stylus tilt exceeds Threshold. Designed to \
                 layer on top of a constant-width core (the standard \
                 Pencil composition).\n\n\
                 • Threshold: how much tilt is needed before shading \
                 appears.\n\
                 • Band ×: relative width of the shading.\n\
                 • Alpha scale: opacity of the shading.\n\n\
                 By itself this layer paints nothing at low tilt — \
                 pair with another layer that draws the core line.",
            ));
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
// Interactive Vello-rendered drawing area. The user draws inside it
// (mouse or stylus); strokes are stored without a recipe and
// re-rendered every frame against the current editor brush — so any
// layer-settings change live-updates every existing stroke. Clear
// button wipes the canvas.

const PREVIEW_W: i32 = 480;
const PREVIEW_H: i32 = 200;
const PREVIEW_BASE_WIDTH: f64 = 8.0;

fn build_preview_area(
    _editor_state: Rc<RefCell<EditorState>>,
    state: SharedState,
    renderer: Rc<RefCell<Option<VelloRenderer>>>,
    preview_brush: Rc<RefCell<Brush>>,
    preview_strokes: Rc<RefCell<Vec<Stroke>>>,
    preview_in_progress: Rc<RefCell<Option<Stroke>>>,
) -> (Frame, DrawingArea) {
    let frame = Frame::builder()
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(16)
        .margin_end(16)
        .build();
    frame.set_label(Some("Preview — draw here"));

    let outer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    let area = DrawingArea::builder()
        .content_width(PREVIEW_W)
        .content_height(PREVIEW_H)
        .build();
    area.set_can_target(true);
    area.set_focusable(true);

    {
        let renderer = renderer.clone();
        let preview_brush = preview_brush.clone();
        let preview_strokes = preview_strokes.clone();
        let preview_in_progress = preview_in_progress.clone();
        let state_outer = state.clone();
        area.set_draw_func(move |_, ctx, w, h| {
            // Cream page surface.
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
            // Pen settings come from the live state (toolbar color,
            // opacity, blend mode) so the preview matches what a real
            // stroke would look like on the canvas. The brush's
            // `default_color` overrides if set — same precedence as
            // "Use this brush" applies on canvas.
            let (pen_color, pen_opacity, pen_blend, pen_base_width) = {
                let s = state_outer.borrow();
                let color = match brush.default_color {
                    Some(rgba) => journal_core::Color {
                        r: rgba[0],
                        g: rgba[1],
                        b: rgba[2],
                        a: rgba[3],
                    },
                    None => s.pen.color,
                };
                (color, s.pen.opacity, s.pen.blend_mode, s.pen.base_width)
            };

            // Compose all strokes (committed + in-progress) with the
            // current brush as their recipe and overwrite their pen
            // settings with the live state — changing the toolbar
            // color or the brush's default-color or opacity instantly
            // re-tints every prior preview stroke.
            let inject = |s: &Stroke| -> Stroke {
                let mut s2 = s.clone();
                s2.brush_recipe = Some(brush.clone());
                s2.pen.color = pen_color;
                s2.pen.opacity = pen_opacity;
                s2.pen.blend_mode = pen_blend;
                s2.pen.base_width = pen_base_width;
                s2
            };
            let mut strokes: Vec<Stroke> = preview_strokes
                .borrow()
                .iter()
                .map(inject)
                .collect();
            if let Some(ip) = preview_in_progress.borrow().as_ref() {
                strokes.push(inject(ip));
            }
            if strokes.is_empty() {
                draw_preview_hint(ctx, w, h);
                return;
            }

            let viewport = Viewport {
                center: Point {
                    x: w as f64 * 0.5,
                    y: h as f64 * 0.5,
                },
                zoom: 1.0,
                rotation: 0.0,
            };
            let transform = ViewportTransform::new(viewport, w as f64, h as f64);
            let page_rect = Rect {
                x: 0.0,
                y: 0.0,
                width: w as f64,
                height: h as f64,
            };
            let bytes = match r.render_rgba(
                &transform,
                &BackgroundConfig::Blank,
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

    // Stylus input.
    {
        let area_widget: gtk4::Widget = area.clone().upcast();
        let g = gtk4::GestureStylus::new();
        g.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let area_clone = area.clone();
        let in_progress = preview_in_progress.clone();
        g.connect_down(move |g, x, y| {
            let pressure = g.axis(gtk4::gdk::AxisUse::Pressure).unwrap_or(0.5) as f32;
            let tilt_x = g.axis(gtk4::gdk::AxisUse::Xtilt).unwrap_or(0.0) as f32;
            let tilt_y = g.axis(gtk4::gdk::AxisUse::Ytilt).unwrap_or(0.0) as f32;
            *in_progress.borrow_mut() = Some(new_preview_stroke(x, y, pressure, tilt_x, tilt_y));
            area_clone.queue_draw();
        });
        let area_clone = area.clone();
        let in_progress = preview_in_progress.clone();
        g.connect_motion(move |g, x, y| {
            let pressure = g.axis(gtk4::gdk::AxisUse::Pressure).unwrap_or(0.5) as f32;
            let tilt_x = g.axis(gtk4::gdk::AxisUse::Xtilt).unwrap_or(0.0) as f32;
            let tilt_y = g.axis(gtk4::gdk::AxisUse::Ytilt).unwrap_or(0.0) as f32;
            if let Some(s) = in_progress.borrow_mut().as_mut() {
                push_preview_point(s, x, y, pressure, tilt_x, tilt_y);
            }
            area_clone.queue_draw();
        });
        let area_clone = area.clone();
        let in_progress = preview_in_progress.clone();
        let strokes = preview_strokes.clone();
        g.connect_up(move |_, _, _| {
            if let Some(s) = in_progress.borrow_mut().take() {
                if s.points.len() >= 2 {
                    strokes.borrow_mut().push(s);
                }
            }
            area_clone.queue_draw();
        });
        area_widget.add_controller(g);
    }

    // Mouse drag fallback (no stylus).
    {
        let area_widget: gtk4::Widget = area.clone().upcast();
        let g = gtk4::GestureDrag::new();
        g.set_button(gtk4::gdk::BUTTON_PRIMARY);
        g.set_propagation_phase(gtk4::PropagationPhase::Bubble);
        let area_clone = area.clone();
        let in_progress = preview_in_progress.clone();
        g.connect_drag_begin(move |_, x, y| {
            *in_progress.borrow_mut() = Some(new_preview_stroke(x, y, 0.7, 0.0, 0.0));
            area_clone.queue_draw();
        });
        let area_clone = area.clone();
        let in_progress = preview_in_progress.clone();
        g.connect_drag_update(move |g, dx, dy| {
            if let Some((sx, sy)) = g.start_point() {
                if let Some(s) = in_progress.borrow_mut().as_mut() {
                    push_preview_point(s, sx + dx, sy + dy, 0.7, 0.0, 0.0);
                }
                area_clone.queue_draw();
            }
        });
        let area_clone = area.clone();
        let in_progress = preview_in_progress.clone();
        let strokes = preview_strokes.clone();
        g.connect_drag_end(move |_, _, _| {
            if let Some(s) = in_progress.borrow_mut().take() {
                if s.points.len() >= 2 {
                    strokes.borrow_mut().push(s);
                }
            }
            area_clone.queue_draw();
        });
        area_widget.add_controller(g);
    }

    outer.append(&area);

    // Toolbar row beneath the canvas — Clear button.
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(gtk4::Align::End)
        .build();
    let clear_btn = Button::with_label("Clear");
    {
        let strokes = preview_strokes.clone();
        let in_progress = preview_in_progress.clone();
        let area_clone = area.clone();
        clear_btn.connect_clicked(move |_| {
            strokes.borrow_mut().clear();
            *in_progress.borrow_mut() = None;
            area_clone.queue_draw();
        });
    }
    row.append(&clear_btn);
    outer.append(&row);

    frame.set_child(Some(&outer));
    (frame, area)
}

fn new_preview_stroke(x: f64, y: f64, pressure: f32, tx: f32, ty: f32) -> Stroke {
    Stroke {
        id: Uuid::nil(),
        points: vec![StrokePoint {
            x,
            y,
            pressure,
            tilt_x: tx,
            tilt_y: ty,
            timestamp_ms: 0,
        }],
        pen: PenSettings {
            color: JColor {
                r: 30,
                g: 30,
                b: 35,
                a: 255,
            },
            base_width: PREVIEW_BASE_WIDTH,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            brush_style: journal_core::BrushStyle::Pen,
        },
        zoom_at_creation: 1.0,
        bounding_box: Rect {
            x,
            y,
            width: 1.0,
            height: 1.0,
        },
        brush_recipe: None,
    }
}

fn push_preview_point(s: &mut Stroke, x: f64, y: f64, pressure: f32, tx: f32, ty: f32) {
    s.points.push(StrokePoint {
        x,
        y,
        pressure,
        tilt_x: tx,
        tilt_y: ty,
        timestamp_ms: s.points.len() as u64,
    });
    // Update bbox so render culling doesn't drop the stroke.
    let bbox = &mut s.bounding_box;
    let pad = PREVIEW_BASE_WIDTH;
    let min_x = bbox.x.min(x - pad);
    let max_x = (bbox.x + bbox.width).max(x + pad);
    let min_y = bbox.y.min(y - pad);
    let max_y = (bbox.y + bbox.height).max(y + pad);
    bbox.x = min_x;
    bbox.y = min_y;
    bbox.width = max_x - min_x;
    bbox.height = max_y - min_y;
}

fn draw_preview_hint(ctx: &gtk4::cairo::Context, w: i32, h: i32) {
    ctx.set_source_rgba(0.4, 0.4, 0.4, 0.5);
    ctx.move_to(20.0, h as f64 * 0.5);
    let _ = ctx.show_text("Draw here to preview the brush. Stylus or mouse.");
    let _ = w;
}

// ── Tip sub-params + custom polygon editor ────────────────────────

fn fill_tip_subparams(
    parent: &GtkBox,
    tip: TipShape,
    editor_state: Rc<RefCell<EditorState>>,
    layer_idx: usize,
    rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) {
    match tip {
        TipShape::Round | TipShape::Square | TipShape::Diamond => {
            // No tunable params — emit a hint label.
            let l = Label::builder()
                .label("(no extra parameters)")
                .halign(gtk4::Align::Start)
                .build();
            l.add_css_class("dim-label");
            parent.append(&l);
        }
        TipShape::FlatNib { angle_deg, aspect } => {
            let a = SpinButton::with_range(-180.0, 180.0, 1.0);
            a.set_digits(1);
            a.set_value(angle_deg);
            parent.append(&row("Nib angle°", a.upcast_ref()));
            let asp = SpinButton::with_range(0.05, 1.0, 0.05);
            asp.set_digits(2);
            asp.set_value(aspect);
            parent.append(&row("Aspect (short:long)", asp.upcast_ref()));
            let (a2, asp2) = (a.clone(), asp.clone());
            let editor_state2 = editor_state.clone();
            let rebuild2 = rebuild.clone();
            let commit = Rc::new(move || {
                if let Some(l) = editor_state2
                    .borrow_mut()
                    .brush
                    .layers
                    .get_mut(layer_idx)
                {
                    l.tip = TipShape::FlatNib {
                        angle_deg: a2.value(),
                        aspect: asp2.value(),
                    };
                }
                if let Some(f) = rebuild2.borrow().as_ref().cloned() {
                    f();
                }
            });
            {
                let commit = commit.clone();
                a.connect_value_changed(move |_| commit());
            }
            asp.connect_value_changed(move |_| commit());
        }
        TipShape::StarN { points, inner_ratio } => {
            let n = SpinButton::with_range(3.0, 24.0, 1.0);
            n.set_digits(0);
            n.set_value(points as f64);
            parent.append(&row("Points", n.upcast_ref()));
            let ir = SpinButton::with_range(0.05, 1.0, 0.05);
            ir.set_digits(2);
            ir.set_value(inner_ratio);
            parent.append(&row("Inner ratio", ir.upcast_ref()));
            let (n2, ir2) = (n.clone(), ir.clone());
            let editor_state2 = editor_state.clone();
            let rebuild2 = rebuild.clone();
            let commit = Rc::new(move || {
                if let Some(l) = editor_state2
                    .borrow_mut()
                    .brush
                    .layers
                    .get_mut(layer_idx)
                {
                    l.tip = TipShape::StarN {
                        points: (n2.value() as u32).clamp(3, 255) as u8,
                        inner_ratio: ir2.value(),
                    };
                }
                if let Some(f) = rebuild2.borrow().as_ref().cloned() {
                    f();
                }
            });
            {
                let commit = commit.clone();
                n.connect_value_changed(move |_| commit());
            }
            ir.connect_value_changed(move |_| commit());
        }
        TipShape::Custom { points } => {
            // Vertex polygon editor — draws the polygon with handles
            // and lets the user drag them. Two helper buttons add /
            // remove vertices.
            let editor = polygon_editor(points, editor_state.clone(), layer_idx, rebuild.clone());
            parent.append(&editor);
        }
    }
}

/// Draggable polygon editor for `TipShape::Custom`. Renders the
/// polygon centred in a fixed-size canvas; each vertex is a draggable
/// handle. Buttons below add / remove vertices.
fn polygon_editor(
    initial: Vec<(f64, f64)>,
    editor_state: Rc<RefCell<EditorState>>,
    layer_idx: usize,
    rebuild: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) -> GtkBox {
    const CANVAS_SIZE: i32 = 220;
    const HANDLE_R: f64 = 6.0;

    let outer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .build();

    let area = DrawingArea::builder()
        .content_width(CANVAS_SIZE)
        .content_height(CANVAS_SIZE)
        .build();
    area.set_can_target(true);

    let pts: Rc<RefCell<Vec<(f64, f64)>>> = Rc::new(RefCell::new(initial));
    let drag_idx: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));

    // Map between unit-space (-1..1) and canvas-space (0..CANVAS_SIZE).
    let to_canvas = |ux: f64, uy: f64| -> (f64, f64) {
        let half = CANVAS_SIZE as f64 * 0.5;
        let r = half - 14.0;
        (half + ux * r, half + uy * r)
    };
    let to_unit = move |cx: f64, cy: f64| -> (f64, f64) {
        let half = CANVAS_SIZE as f64 * 0.5;
        let r = half - 14.0;
        ((cx - half) / r, (cy - half) / r)
    };

    {
        let pts = pts.clone();
        area.set_draw_func(move |_, ctx, _, _| {
            // Background.
            ctx.set_source_rgb(0.97, 0.96, 0.93);
            let _ = ctx.paint();
            // Crosshair.
            ctx.set_source_rgba(0.4, 0.4, 0.45, 0.3);
            ctx.set_line_width(1.0);
            ctx.move_to(0.0, CANVAS_SIZE as f64 * 0.5);
            ctx.line_to(CANVAS_SIZE as f64, CANVAS_SIZE as f64 * 0.5);
            ctx.move_to(CANVAS_SIZE as f64 * 0.5, 0.0);
            ctx.line_to(CANVAS_SIZE as f64 * 0.5, CANVAS_SIZE as f64);
            let _ = ctx.stroke();

            // Polygon.
            let p = pts.borrow();
            if p.len() >= 2 {
                ctx.set_source_rgba(0.2, 0.3, 0.55, 0.35);
                let (x0, y0) = to_canvas(p[0].0, p[0].1);
                ctx.move_to(x0, y0);
                for v in p.iter().skip(1) {
                    let (x, y) = to_canvas(v.0, v.1);
                    ctx.line_to(x, y);
                }
                ctx.close_path();
                let _ = ctx.fill_preserve();
                ctx.set_source_rgba(0.15, 0.2, 0.4, 1.0);
                ctx.set_line_width(1.5);
                let _ = ctx.stroke();
            }
            // Handles.
            ctx.set_source_rgb(0.85, 0.45, 0.2);
            for v in p.iter() {
                let (x, y) = to_canvas(v.0, v.1);
                ctx.arc(x, y, HANDLE_R, 0.0, std::f64::consts::TAU);
                let _ = ctx.fill();
            }
        });
    }

    // Drag pointer.
    {
        let area_widget: gtk4::Widget = area.clone().upcast();
        let g = gtk4::GestureDrag::new();
        g.set_button(gtk4::gdk::BUTTON_PRIMARY);
        let pts_d = pts.clone();
        let drag_idx_d = drag_idx.clone();
        let area_clone = area.clone();
        g.connect_drag_begin(move |_, x, y| {
            let p = pts_d.borrow();
            let mut hit = None;
            for (i, v) in p.iter().enumerate() {
                let (cx, cy) = to_canvas(v.0, v.1);
                let dx = cx - x;
                let dy = cy - y;
                if (dx * dx + dy * dy).sqrt() < HANDLE_R * 1.6 {
                    hit = Some(i);
                    break;
                }
            }
            *drag_idx_d.borrow_mut() = hit;
            area_clone.queue_draw();
        });
        let pts_d = pts.clone();
        let drag_idx_d = drag_idx.clone();
        let editor_state2 = editor_state.clone();
        let area_clone = area.clone();
        // Mid-drag: only mutate brush + repaint the polygon canvas.
        // Calling `rebuild` here would tear down the very widget the
        // drag is attached to, ending the drag prematurely. Preview
        // catches up at drag_end.
        g.connect_drag_update(move |g, dx, dy| {
            let Some((sx, sy)) = g.start_point() else { return };
            let cx = sx + dx;
            let cy = sy + dy;
            let (ux, uy) = to_unit(cx, cy);
            let ux = ux.clamp(-1.0, 1.0);
            let uy = uy.clamp(-1.0, 1.0);
            let i = match *drag_idx_d.borrow() {
                Some(i) => i,
                None => return,
            };
            {
                let mut p = pts_d.borrow_mut();
                if let Some(v) = p.get_mut(i) {
                    *v = (ux, uy);
                }
            }
            if let Some(l) = editor_state2.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.tip = TipShape::Custom {
                    points: pts_d.borrow().clone(),
                };
            }
            area_clone.queue_draw();
        });
        let drag_idx_d = drag_idx.clone();
        let rebuild2 = rebuild.clone();
        g.connect_drag_end(move |_, _, _| {
            *drag_idx_d.borrow_mut() = None;
            if let Some(f) = rebuild2.borrow().as_ref().cloned() {
                f();
            }
        });
        area_widget.add_controller(g);
    }
    outer.append(&area);

    // Add / Remove vertex buttons.
    let btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(gtk4::Align::End)
        .build();
    let add_btn = Button::with_label("+ Vertex");
    let rem_btn = Button::with_label("− Vertex");
    let reset_btn = Button::with_label("Reset");
    btn_row.append(&reset_btn);
    btn_row.append(&rem_btn);
    btn_row.append(&add_btn);
    outer.append(&btn_row);

    // Helper: regenerate polygon ring from N vertices.
    let regen_ring = |n: usize| -> Vec<(f64, f64)> {
        (0..n)
            .map(|i| {
                let theta = (i as f64) * std::f64::consts::TAU / (n as f64)
                    - std::f64::consts::FRAC_PI_2;
                (theta.cos(), theta.sin())
            })
            .collect()
    };

    {
        let pts_a = pts.clone();
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        let area_clone = area.clone();
        add_btn.connect_clicked(move |_| {
            let new_pts = {
                let mut p = pts_a.borrow().clone();
                // Insert midpoint between last and first.
                if p.is_empty() {
                    p.push((1.0, 0.0));
                } else {
                    let last = *p.last().unwrap();
                    let first = p[0];
                    let mid = ((last.0 + first.0) * 0.5, (last.1 + first.1) * 0.5);
                    p.push(mid);
                }
                p
            };
            *pts_a.borrow_mut() = new_pts.clone();
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.tip = TipShape::Custom { points: new_pts };
            }
            area_clone.queue_draw();
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }
    {
        let pts_r = pts.clone();
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        let area_clone = area.clone();
        rem_btn.connect_clicked(move |_| {
            let new_pts = {
                let mut p = pts_r.borrow().clone();
                if p.len() > 3 {
                    p.pop();
                }
                p
            };
            *pts_r.borrow_mut() = new_pts.clone();
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.tip = TipShape::Custom { points: new_pts };
            }
            area_clone.queue_draw();
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }
    {
        let pts_rs = pts.clone();
        let editor_state = editor_state.clone();
        let rebuild = rebuild.clone();
        let area_clone = area.clone();
        reset_btn.connect_clicked(move |_| {
            let n = pts_rs.borrow().len().max(3);
            let new_pts = regen_ring(n);
            *pts_rs.borrow_mut() = new_pts.clone();
            if let Some(l) = editor_state.borrow_mut().brush.layers.get_mut(layer_idx) {
                l.tip = TipShape::Custom { points: new_pts };
            }
            area_clone.queue_draw();
            if let Some(f) = rebuild.borrow().as_ref().cloned() {
                f();
            }
        });
    }

    outer
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
