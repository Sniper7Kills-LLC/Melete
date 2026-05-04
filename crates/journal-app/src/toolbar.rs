use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::graphene::Point as GraphenePoint;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DrawingArea, FlowBox, GestureDrag, GestureLongPress, Image, Label,
    MenuButton, Orientation, Popover, PositionType, PropagationPhase, Scale, Separator,
    ToggleButton,
};

use crate::state::{SharedState, Tool};

/// Build the floating pen toolbar.
///
/// The returned widget is a `GtkBox` positioned via `margin_start` / `margin_top`
/// inside a `gtk4::Overlay` with `halign(Start)` + `valign(Start)`.  A
/// `GestureDrag` attached to the grip handle (left-most icon) lets the user
/// reposition it; the final position is saved to `~/.config/journal/config.toml`.
///
/// A chevron button at the right end toggles a collapsed mode where only the
/// drag handle and the currently-active tool icon are visible.
pub fn build_toolbar(
    state: SharedState,
    tools_open: Rc<RefCell<Option<Rc<dyn Fn(Option<journal_core::Brush>)>>>>,
) -> GtkBox {
    // ── Outer wrapper: positioned inside the Overlay via margins ─────────
    let cfg = crate::config::load();

    let bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .halign(gtk4::Align::Start)
        .valign(gtk4::Align::Start)
        .build();
    bar.add_css_class("osd");
    bar.add_css_class("toolbar");
    bar.add_css_class("floating-toolbar");

    // ── Drag handle — compact (vertical grip dots) ────────────────────────
    let handle = Image::from_icon_name("view-more-symbolic");
    handle.set_tooltip_text(Some("Drag to move toolbar"));
    handle.add_css_class("drag-handle-compact");
    handle.set_size_request(20, 32);
    handle.set_cursor_from_name(Some("grab"));
    bar.append(&handle);

    bar.append(&Separator::new(Orientation::Vertical));

    // ── Drawing-tools dropdown ────────────────────────────────────────────
    // Single MenuButton replaces the old pen/highlighter pair. The icon
    // mirrors whichever drawing tool is active; clicking opens a popover
    // with all five drawing tools. The eraser and selection tools stay as
    // separate top-level buttons because they aren't drawing tools.
    let initial_tool = state.borrow().tool;
    let tools_btn = MenuButton::builder()
        .icon_name(icon_for_tool(initial_tool))
        .tooltip_text("Drawing tool")
        .build();
    tools_btn.add_css_class("compact-tool");
    let tools_popover = Popover::new();
    tools_popover.set_position(PositionType::Bottom);
    let tools_list = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .margin_top(4)
        .margin_bottom(4)
        .margin_start(4)
        .margin_end(4)
        .build();
    for (tool, label) in [
        (Tool::Pen,         "Pen"),
        (Tool::Pencil,      "Pencil"),
        (Tool::Highlighter, "Highlighter"),
        (Tool::Paintbrush,  "Paintbrush"),
        (Tool::SprayCan,    "Spray can"),
        (Tool::Calligraphy, "Calligraphy"),
    ] {
        let item = Button::builder()
            .label(label)
            .icon_name(icon_for_tool(tool))
            .build();
        item.add_css_class("flat");
        item.set_halign(gtk4::Align::Fill);
        // Force label visible alongside icon (GTK Button with both shows
        // icon by default).
        let row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .build();
        let icon = Image::from_icon_name(icon_for_tool(tool));
        let lbl = gtk4::Label::new(Some(label));
        lbl.set_halign(gtk4::Align::Start);
        lbl.set_hexpand(true);
        row.append(&icon);
        row.append(&lbl);
        item.set_child(Some(&row));
        let state_c = state.clone();
        let popover_c = tools_popover.clone();
        let tools_btn_c = tools_btn.clone();
        item.connect_clicked(move |_| {
            crate::state::set_tool(&state_c, tool);
            tools_btn_c.set_icon_name(icon_for_tool(tool));
            popover_c.popdown();
        });
        tools_list.append(&item);
    }
    // ── Edit current tool entry ─────────────────────────────────────
    tools_list.append(&Separator::new(Orientation::Horizontal));
    let edit_btn = Button::with_label("Edit current tool…");
    edit_btn.add_css_class("flat");
    {
        let state = state.clone();
        let popover = tools_popover.clone();
        let tools_open = tools_open.clone();
        edit_btn.connect_clicked(move |_| {
            popover.popdown();
            let tool = state.borrow().tool;
            let seed = builtin_brush_for_tool(tool);
            if let Some(f) = tools_open.borrow().as_ref().cloned() {
                f(seed);
            }
        });
    }
    tools_list.append(&edit_btn);

    // Per-tool brush picker — lists built-in + custom brushes;
    // clicking one assigns it to the active tool's slot. Rebuilt on
    // every popover show so library add/rename/delete from the Tool
    // Editor land instantly without an app restart.
    tools_list.append(&Separator::new(Orientation::Horizontal));
    let brush_label = gtk4::Label::builder()
        .label("Brush for current tool")
        .halign(gtk4::Align::Start)
        .build();
    brush_label.add_css_class("dim-label");
    tools_list.append(&brush_label);
    let brush_subbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .build();
    tools_list.append(&brush_subbox);
    {
        let state = state.clone();
        let popover = tools_popover.clone();
        let brush_subbox = brush_subbox.clone();
        let tools_open = tools_open.clone();
        tools_popover.connect_visible_notify(move |po| {
            if !po.is_visible() {
                return;
            }
            rebuild_brush_picker(&brush_subbox, &state, &popover, &tools_open);
        });
    }

    tools_popover.set_child(Some(&tools_list));
    tools_btn.set_popover(Some(&tools_popover));

    // ── Eraser / selection (separate from drawing-tool dropdown) ──────────
    let eraser_btn = ToggleButton::builder()
        .icon_name("edit-clear-symbolic")
        .tooltip_text("Eraser — stroke (Ctrl+E)")
        .build();

    let partial_eraser_btn = ToggleButton::builder()
        .icon_name("edit-cut-symbolic")
        .tooltip_text("Partial Eraser — splits strokes")
        .group(&eraser_btn)
        .build();

    let selection_btn = ToggleButton::builder()
        .icon_name("edit-select-all-symbolic")
        .tooltip_text("Selection (V)")
        .group(&eraser_btn)
        .build();

    {
        let state = state.clone();
        eraser_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                let mut s = state.borrow_mut();
                s.tool = Tool::Eraser(crate::state::EraserMode::Stroke);
            }
        });
    }
    {
        let state = state.clone();
        partial_eraser_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                let mut s = state.borrow_mut();
                s.tool = Tool::Eraser(crate::state::EraserMode::Partial);
            }
        });
    }
    {
        let state = state.clone();
        selection_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                crate::state::set_tool_selection(&state);
            }
        });
    }

    for b in [&eraser_btn, &partial_eraser_btn, &selection_btn] {
        b.add_css_class("compact-tool");
    }

    // Whenever the tool changes from outside the dropdown (keyboard
    // shortcut, color-slot click that snaps to Pen, etc.), un-toggle the
    // eraser/selection buttons so the UI matches state.
    {
        let state = state.clone();
        let eraser = eraser_btn.clone();
        let partial = partial_eraser_btn.clone();
        let select = selection_btn.clone();
        let tools_btn_inner = tools_btn.clone();
        let last_tool: Rc<Cell<Tool>> = Rc::new(Cell::new(initial_tool));
        tools_btn.add_tick_callback(move |_, _| {
            let cur = state.borrow().tool;
            if cur != last_tool.get() {
                last_tool.set(cur);
                use crate::state::EraserMode;
                eraser.set_active(matches!(cur, Tool::Eraser(EraserMode::Stroke)));
                partial.set_active(matches!(cur, Tool::Eraser(EraserMode::Partial)));
                select.set_active(matches!(cur, Tool::Selection));
                if crate::state::tool_is_drawing(cur) {
                    tools_btn_inner.set_icon_name(icon_for_tool(cur));
                }
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    // ── Color slots (replaces standalone color picker) ────────────────────
    // 2-3 configurable color swatches. Tapping a slot makes it the active
    // pen color and opens a small inline RGB picker popover so the user
    // can re-tune that slot's color. Starting a stroke auto-dismisses the
    // popover (see tick callback below).
    let color_slots_box = build_color_slots(state.clone(), &cfg.color_slots);

    // ── Width scale (compact, no leading "Width" label) ──────────────────
    let scale = Scale::with_range(Orientation::Horizontal, 0.5, 12.0, 0.5);
    scale.set_value(state.borrow().pen.base_width);
    scale.set_width_request(120);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk4::PositionType::Right);
    scale.set_tooltip_text(Some("Pen width (mm)"));
    scale.add_css_class("compact-scale");
    {
        let state = state.clone();
        scale.connect_value_changed(move |s| {
            let v = s.value();
            state.borrow_mut().pen.base_width = v;
        });
    }

    // ── Collapse / expand chevron ─────────────────────────────────────────
    // collapsed: Rc<Cell<bool>> tracks whether the toolbar is currently
    // collapsed. We persist the state to config on every toggle.
    let collapsed = Rc::new(Cell::new(cfg.toolbar_collapsed));

    let chevron_btn = Button::builder()
        .icon_name(if cfg.toolbar_collapsed {
            "go-previous-symbolic"
        } else {
            "go-next-symbolic"
        })
        .tooltip_text("Collapse / expand toolbar")
        .build();
    chevron_btn.add_css_class("compact-tool");

    // Always-visible cluster: drawing-tool dropdown + color slots. These
    // are usable whether the bar is collapsed or expanded — Photoshop-style
    // tool palette where the primary picks stay reachable at all times.
    bar.append(&tools_btn);
    bar.append(&color_slots_box);

    // `extras` holds the secondary controls (eraser/selection toggles +
    // width slider). Hidden when the user collapses the bar.
    let extras = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .build();

    extras.append(&Separator::new(Orientation::Vertical));
    extras.append(&eraser_btn);
    extras.append(&partial_eraser_btn);
    extras.append(&selection_btn);
    extras.append(&Separator::new(Orientation::Vertical));
    extras.append(&scale);

    bar.append(&extras);
    bar.append(&chevron_btn);

    // Apply initial visibility based on saved collapsed state.
    extras.set_visible(!cfg.toolbar_collapsed);

    // Toggle collapse on chevron click.
    {
        let extras = extras.clone();
        let collapsed = collapsed.clone();
        let chevron_btn2 = chevron_btn.clone();
        chevron_btn.connect_clicked(move |_| {
            let now = !collapsed.get();
            collapsed.set(now);
            extras.set_visible(!now);
            chevron_btn2.set_icon_name(if now {
                "go-previous-symbolic"
            } else {
                "go-next-symbolic"
            });
            let mut cfg = crate::config::load();
            cfg.toolbar_collapsed = now;
            if let Err(e) = crate::config::save(&cfg) {
                tracing::warn!("Failed to save toolbar collapsed state: {}", e);
            }
        });
    }

    // ── Restore saved position or default to bottom-centre ───────────────
    if let (Some(x), Some(y)) = (cfg.toolbar_x, cfg.toolbar_y) {
        bar.set_margin_start(x);
        bar.set_margin_top(y);
    } else {
        let bar_for_map = bar.clone();
        bar.connect_map(move |_| {
            if bar_for_map.margin_start() == 0 && bar_for_map.margin_top() == 0 {
                if let Some(parent) = bar_for_map.parent() {
                    let pw = parent.width();
                    let ph = parent.height();
                    let bw = bar_for_map.width();
                    let bh = bar_for_map.height();
                    let x = ((pw - bw) / 2).max(0);
                    let y = (ph - bh - 16).max(0);
                    bar_for_map.set_margin_start(x);
                    bar_for_map.set_margin_top(y);
                }
            }
        });
    }

    // ── GestureDrag on the handle ─────────────────────────────────────────
    let drag = GestureDrag::builder()
        .propagation_phase(PropagationPhase::Capture)
        .build();

    let origin_x: Rc<Cell<i32>> = Rc::new(Cell::new(0));
    let origin_y: Rc<Cell<i32>> = Rc::new(Cell::new(0));
    let start_root_x: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));
    let start_root_y: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));

    {
        let bar_ref = bar.clone();
        let handle_ref = handle.clone();
        let ox = origin_x.clone();
        let oy = origin_y.clone();
        let srx = start_root_x.clone();
        let sry = start_root_y.clone();
        drag.connect_drag_begin(move |_gesture, sx, sy| {
            ox.set(bar_ref.margin_start());
            oy.set(bar_ref.margin_top());
            if let Some(root) = handle_ref.root() {
                let p = handle_ref
                    .compute_point(&root, &GraphenePoint::new(sx as f32, sy as f32))
                    .unwrap_or_else(|| GraphenePoint::new(sx as f32, sy as f32));
                srx.set(p.x() as f64);
                sry.set(p.y() as f64);
            }
        });
    }

    {
        let bar_ref = bar.clone();
        let handle_ref = handle.clone();
        let ox = origin_x.clone();
        let oy = origin_y.clone();
        let srx = start_root_x.clone();
        let sry = start_root_y.clone();
        drag.connect_drag_update(move |gesture, _dx, _dy| {
            let (cx, cy) = match gesture.point(None) {
                Some(p) => p,
                None => return,
            };
            let Some(root) = handle_ref.root() else { return; };
            let cur = handle_ref
                .compute_point(&root, &GraphenePoint::new(cx as f32, cy as f32))
                .unwrap_or_else(|| GraphenePoint::new(cx as f32, cy as f32));
            let dx_root = cur.x() as f64 - srx.get();
            let dy_root = cur.y() as f64 - sry.get();

            let mut new_x = (ox.get() as f64 + dx_root).round() as i32;
            let mut new_y = (oy.get() as f64 + dy_root).round() as i32;
            if new_x < 0 { new_x = 0; }
            if new_y < 0 { new_y = 0; }

            if let Some(parent) = bar_ref.parent() {
                let pw = parent.width();
                let ph = parent.height();
                let bw = bar_ref.width().max(48);
                let bh = bar_ref.height().max(32);
                let max_x = (pw - bw.min(pw)).max(0);
                let max_y = (ph - bh.min(ph)).max(0);
                new_x = new_x.min(max_x);
                new_y = new_y.min(max_y);
            }

            bar_ref.set_margin_start(new_x);
            bar_ref.set_margin_top(new_y);
        });
    }

    {
        let bar_ref = bar.clone();
        drag.connect_drag_end(move |_gesture, _dx, _dy| {
            let x = bar_ref.margin_start();
            let y = bar_ref.margin_top();
            let mut cfg = crate::config::load();
            cfg.toolbar_x = Some(x);
            cfg.toolbar_y = Some(y);
            if let Err(e) = crate::config::save(&cfg) {
                tracing::warn!("Failed to save toolbar position: {}", e);
            }
        });
    }

    handle.add_controller(drag);

    bar
}

// (Pen-preset chips removed: color slots cover the same role and the
// duplicate row was visually noisy.)

/// Returns the symbolic icon name for the given tool. The names lean on
/// freedesktop.org symbolic icons that ship with most icon themes; missing
/// names fall back to a generic image-missing glyph.
/// Refresh the per-tool brush-picker subsection of the tools
/// popover. Called every time the popover becomes visible so that
/// brushes added / renamed / deleted in the Tool Editor land
/// instantly without restarting the app.
fn rebuild_brush_picker(
    container: &GtkBox,
    state: &SharedState,
    popover: &Popover,
    tools_open: &Rc<RefCell<Option<Rc<dyn Fn(Option<journal_core::Brush>)>>>>,
) {
    while let Some(c) = container.first_child() {
        container.remove(&c);
    }
    let tool = state.borrow().tool;
    if !crate::state::tool_is_drawing(tool) {
        let l = gtk4::Label::builder()
            .label("(non-drawing tool)")
            .halign(gtk4::Align::Start)
            .build();
        l.add_css_class("dim-label");
        container.append(&l);
        return;
    }
    let active_id = crate::tool_settings::tool_key(tool)
        .and_then(|k| state.borrow().tool_brushes.get(k).map(|b| b.id));

    let mut entries: Vec<journal_core::Brush> =
        crate::brush_library::built_ins();
    entries.extend(state.borrow().brush_library.clone());

    for brush in entries {
        let row = gtk4::Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(6)
            .build();
        let mark = gtk4::Label::builder()
            .label(if Some(brush.id) == active_id { "●" } else { "  " })
            .width_chars(2)
            .build();
        let label = gtk4::Label::builder()
            .label(&brush.name)
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        row.append(&mark);
        row.append(&label);
        let btn = Button::builder().child(&row).build();
        btn.add_css_class("flat");
        let state_c = state.clone();
        let pop_c = popover.clone();
        let brush_c = brush.clone();
        btn.connect_clicked(move |_| {
            let tool = state_c.borrow().tool;
            let key = match crate::tool_settings::tool_key(tool) {
                Some(k) => k.to_string(),
                None => {
                    pop_c.popdown();
                    return;
                }
            };
            {
                let mut s = state_c.borrow_mut();
                s.tool_brushes.insert(key, brush_c.clone());
                s.active_brush_recipe = Some(brush_c.clone());
                if let Some(rgba) = brush_c.default_color {
                    s.pen.color = journal_core::Color {
                        r: rgba[0],
                        g: rgba[1],
                        b: rgba[2],
                        a: rgba[3],
                    };
                }
            }
            crate::state::persist_tool_state(&state_c);
            pop_c.popdown();
        });
        container.append(&btn);
    }

    // Clear-assignment entry — falls back to the legacy adapter.
    let clear_row = gtk4::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    let mark = gtk4::Label::builder()
        .label(if active_id.is_none() { "●" } else { "  " })
        .width_chars(2)
        .build();
    let label = gtk4::Label::builder()
        .label("(none — use built-in)")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    clear_row.append(&mark);
    clear_row.append(&label);
    let clear_btn = Button::builder().child(&clear_row).build();
    clear_btn.add_css_class("flat");
    {
        let state_c = state.clone();
        let pop_c = popover.clone();
        clear_btn.connect_clicked(move |_| {
            let tool = state_c.borrow().tool;
            if let Some(key) = crate::tool_settings::tool_key(tool) {
                let mut s = state_c.borrow_mut();
                s.tool_brushes.remove(key);
                s.active_brush_recipe = None;
            }
            crate::state::persist_tool_state(&state_c);
            pop_c.popdown();
        });
    }
    container.append(&clear_btn);

    let _ = tools_open;
}

/// Map a drawing tool to its matching built-in brush composition.
/// Used by the toolbar's "Edit current tool…" entry to pre-seed the
/// Tool Editor with the recipe the user is currently drawing with.
fn builtin_brush_for_tool(tool: Tool) -> Option<journal_core::Brush> {
    use journal_canvas::built_in_brushes as bi;
    Some(match tool {
        Tool::Pen => bi::pen(0.6, 0.4),
        Tool::Pencil => bi::pencil(0.4, 0.9, 0.12, 8.0, 0.22),
        Tool::Highlighter => bi::highlighter(0.6, 0.4),
        Tool::Paintbrush => bi::paintbrush(1.6, 1.4, 0.95, 0.07, 0.20, 0.95),
        Tool::SprayCan => bi::spray(36, 0.06, 0.35),
        Tool::Calligraphy => bi::calligraphy(45.0, 0.18, 0.5, true),
        _ => return None,
    })
}

fn icon_for_tool(tool: Tool) -> &'static str {
    match tool {
        Tool::Pen => "document-edit-symbolic",
        Tool::Pencil => "edit-symbolic",
        Tool::Highlighter => "marker-symbolic",
        Tool::Paintbrush => "applications-graphics-symbolic",
        Tool::SprayCan => "weather-fog-symbolic",
        Tool::Calligraphy => "format-text-italic-symbolic",
        Tool::Eraser(crate::state::EraserMode::Stroke) => "edit-clear-symbolic",
        Tool::Eraser(crate::state::EraserMode::Partial) => "edit-cut-symbolic",
        Tool::Selection => "edit-select-all-symbolic",
    }
}

/// Long-press palette popover for a color slot. Shows the current hex
/// value, a "Save to palette" button (writes the slot color into the
/// active drawing tool's per-tool palette in `state.tool_palettes`),
/// and a swatch grid of the active tool's saved colors. Tapping a
/// swatch overwrites the slot's color and dismisses. Audit §7.
fn build_palette_popover(
    state: SharedState,
    slot_color: Rc<Cell<[u8; 4]>>,
    persist_slot: Rc<dyn Fn([u8; 4])>,
    swatch: DrawingArea,
) -> Popover {
    let popover = Popover::new();
    popover.set_position(PositionType::Bottom);
    popover.set_autohide(true);

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let [r, g, b, a] = slot_color.get();
    let is_empty = a == 0;
    let hex = if is_empty {
        "(empty slot)".to_string()
    } else {
        format!("#{:02X}{:02X}{:02X}", r, g, b)
    };
    let hex_lbl = Label::builder().label(&hex).halign(gtk4::Align::Start).build();
    hex_lbl.add_css_class("kbd");
    body.append(&hex_lbl);

    let save_btn = Button::with_label("Save to palette");
    save_btn.set_sensitive(!is_empty);
    {
        let state_c = state.clone();
        let slot_color_c = slot_color.clone();
        let popover_c = popover.clone();
        save_btn.connect_clicked(move |_| {
            let tool = state_c.borrow().tool;
            let key = crate::tool_settings::tool_key(tool).map(|k| k.to_string());
            let Some(key) = key else {
                popover_c.popdown();
                return;
            };
            let color = slot_color_c.get();
            if color[3] == 0 {
                popover_c.popdown();
                return;
            }
            {
                let mut s = state_c.borrow_mut();
                let entry = s.tool_palettes.entry(key).or_default();
                if !entry.iter().any(|c| *c == color) {
                    entry.push(color);
                }
            }
            crate::state::persist_tool_state(&state_c);
            popover_c.popdown();
        });
    }
    body.append(&save_btn);

    // Clear the slot — flips it to the empty state (alpha 0, drawn as
    // a diagonal stripe). Lets the user signal "drag a colour here" or
    // re-fill via the inline picker on next tap.
    let clear_btn = Button::with_label("Clear slot");
    clear_btn.set_sensitive(!is_empty);
    {
        let slot_color_c = slot_color.clone();
        let popover_c = popover.clone();
        let persist_slot_c = persist_slot.clone();
        let swatch_c = swatch.clone();
        clear_btn.connect_clicked(move |_| {
            slot_color_c.set([0, 0, 0, 0]);
            (persist_slot_c)([0, 0, 0, 0]);
            swatch_c.queue_draw();
            popover_c.popdown();
        });
    }
    body.append(&clear_btn);

    let tool = state.borrow().tool;
    let key = crate::tool_settings::tool_key(tool).map(|k| k.to_string());
    let palette: Vec<[u8; 4]> = key
        .as_deref()
        .and_then(|k| state.borrow().tool_palettes.get(k).cloned())
        .unwrap_or_default();

    if palette.is_empty() {
        let hint = Label::builder()
            .label("No saved swatches yet — pick a color, then long-press the slot to save.")
            .wrap(true)
            .max_width_chars(28)
            .build();
        hint.add_css_class("dim-label");
        body.append(&hint);
    } else {
        let title = Label::builder()
            .label("Saved")
            .halign(gtk4::Align::Start)
            .build();
        title.add_css_class("dim-label");
        body.append(&title);

        let flow = FlowBox::builder()
            .max_children_per_line(6)
            .min_children_per_line(6)
            .selection_mode(gtk4::SelectionMode::None)
            .row_spacing(4)
            .column_spacing(4)
            .build();
        for color in palette {
            let swatch = DrawingArea::new();
            swatch.set_size_request(22, 22);
            let color_cell: Rc<Cell<[u8; 4]>> = Rc::new(Cell::new(color));
            {
                let color_cell = color_cell.clone();
                swatch.set_draw_func(move |_, cr, w, h| {
                    let [cr_, cg, cb, ca] = color_cell.get();
                    let cx = w as f64 / 2.0;
                    let cy = h as f64 / 2.0;
                    let radius = (cx.min(cy) - 1.0).max(1.0);
                    cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
                    cr.set_source_rgba(
                        cr_ as f64 / 255.0,
                        cg as f64 / 255.0,
                        cb as f64 / 255.0,
                        ca as f64 / 255.0,
                    );
                    let _ = cr.fill();
                    cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.4);
                    cr.set_line_width(0.8);
                    let _ = cr.stroke();
                });
            }
            let btn = Button::builder().build();
            btn.add_css_class("flat");
            btn.set_size_request(28, 28);
            btn.set_child(Some(&swatch));
            {
                let state_c = state.clone();
                let slot_color_c = slot_color.clone();
                let popover_c = popover.clone();
                let color_cell = color_cell.clone();
                let persist_slot_c = persist_slot.clone();
                let swatch_c = swatch.clone();
                btn.connect_clicked(move |_| {
                    let [r, g, b, a] = color_cell.get();
                    slot_color_c.set([r, g, b, a]);
                    {
                        let mut s = state_c.borrow_mut();
                        s.pen.color = journal_core::Color { r, g, b, a };
                        if !crate::state::tool_is_drawing(s.tool) {
                            s.tool = Tool::Pen;
                        }
                    }
                    (persist_slot_c)([r, g, b, a]);
                    swatch_c.queue_draw();
                    popover_c.popdown();
                });
            }
            flow.insert(&btn, -1);
        }
        body.append(&flow);
    }

    popover.set_child(Some(&body));
    popover
}

/// Build the color-slot row. Each slot is a small button rendering a filled
/// circle in the slot's color. Tapping a slot:
///   1. Sets `state.pen.color` to the slot's color (and switches to a
///      drawing tool if currently on eraser/selection).
///   2. Opens a small inline RGB picker popover anchored to the slot.
///      Adjusting the sliders updates both the slot's persisted color and
///      `state.pen.color` live.
///   3. The popover auto-dismisses when the user starts a stroke (a tick
///      callback watches `state.pointer_drawing`).
fn build_color_slots(state: SharedState, slots: &[[u8; 4]]) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .build();

    // Cell of the currently-open picker popover so the tick callback can
    // dismiss it on stroke start without owning a reference per-slot.
    let active_picker: Rc<RefCell<Option<Popover>>> = Rc::new(RefCell::new(None));

    for (idx, raw) in slots.iter().enumerate() {
        let slot_color: Rc<Cell<[u8; 4]>> = Rc::new(Cell::new(*raw));

        let swatch = DrawingArea::new();
        swatch.set_size_request(20, 20);
        {
            let slot_color = slot_color.clone();
            swatch.set_draw_func(move |_, cr, w, h| {
                let [r, g, b, a] = slot_color.get();
                let cx = w as f64 / 2.0;
                let cy = h as f64 / 2.0;
                let radius = (cx.min(cy) - 1.0).max(1.0);
                if a == 0 {
                    // Empty slot — diagonal-stripe affordance signalling
                    // "drag a colour here". Audit §7.
                    cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
                    cr.clip();
                    let stripe = 4.0;
                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.18);
                    cr.set_line_width(1.4);
                    let mut d = -2.0 * radius;
                    while d < 2.0 * radius {
                        cr.move_to(cx - radius + d, cy - radius);
                        cr.line_to(cx - radius + d + 2.0 * radius, cy + radius);
                        let _ = cr.stroke();
                        d += stripe;
                    }
                    cr.reset_clip();
                    cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
                    cr.set_line_width(0.8);
                    let _ = cr.stroke();
                    return;
                }
                cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
                cr.set_source_rgba(
                    r as f64 / 255.0,
                    g as f64 / 255.0,
                    b as f64 / 255.0,
                    a as f64 / 255.0,
                );
                let _ = cr.fill();
                cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.4);
                cr.set_line_width(0.8);
                let _ = cr.stroke();
            });
        }

        let btn = Button::builder()
            .tooltip_text(format!("Color slot {} — tap to pick", idx + 1))
            .build();
        btn.add_css_class("compact-tool");
        btn.set_size_request(28, 28);
        btn.set_child(Some(&swatch));

        // Inline RGB picker popover (small, in-toolbar — no system dialog).
        let picker = Popover::new();
        picker.set_position(PositionType::Bottom);
        picker.set_parent(&btn);
        picker.set_autohide(true);

        let picker_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();

        let preview = DrawingArea::new();
        preview.set_size_request(180, 22);
        {
            let slot_color = slot_color.clone();
            preview.set_draw_func(move |_, cr, w, h| {
                let [r, g, b, a] = slot_color.get();
                cr.set_source_rgba(
                    r as f64 / 255.0,
                    g as f64 / 255.0,
                    b as f64 / 255.0,
                    a as f64 / 255.0,
                );
                cr.rectangle(0.0, 0.0, w as f64, h as f64);
                let _ = cr.fill();
            });
        }
        picker_box.append(&preview);

        let make_scale = |label: &str, init: u8| -> (GtkBox, Scale) {
            let row = GtkBox::builder()
                .orientation(Orientation::Horizontal)
                .spacing(6)
                .build();
            let l = gtk4::Label::new(Some(label));
            l.set_width_chars(2);
            row.append(&l);
            let s = Scale::with_range(Orientation::Horizontal, 0.0, 255.0, 1.0);
            s.set_value(init as f64);
            s.set_width_request(160);
            s.set_draw_value(true);
            s.set_value_pos(PositionType::Right);
            row.append(&s);
            (row, s)
        };
        let (r_row, r_scale) = make_scale("R", raw[0]);
        let (g_row, g_scale) = make_scale("G", raw[1]);
        let (b_row, b_scale) = make_scale("B", raw[2]);
        picker_box.append(&r_row);
        picker_box.append(&g_row);
        picker_box.append(&b_row);
        picker.set_child(Some(&picker_box));

        // Single source of truth for "write [r,g,b,a] to this slot's
        // persistent index, refresh widgets that depend on it." Used by
        // both the inline RGB picker and the long-press palette popover.
        let persist_slot: Rc<dyn Fn([u8; 4])> = {
            let preview = preview.clone();
            let swatch = swatch.clone();
            let slot_color = slot_color.clone();
            Rc::new(move |new_color: [u8; 4]| {
                slot_color.set(new_color);
                preview.queue_draw();
                swatch.queue_draw();
                let mut cfg = crate::config::load();
                while cfg.color_slots.len() <= idx {
                    cfg.color_slots.push([20, 20, 20, 255]);
                }
                cfg.color_slots[idx] = new_color;
                if let Err(e) = crate::config::save(&cfg) {
                    tracing::warn!("Failed to save color slot {}: {}", idx, e);
                }
            })
        };

        let on_change: Rc<dyn Fn()> = {
            let r_scale = r_scale.clone();
            let g_scale = g_scale.clone();
            let b_scale = b_scale.clone();
            let state = state.clone();
            let persist_slot = persist_slot.clone();
            Rc::new(move || {
                let r = r_scale.value().round().clamp(0.0, 255.0) as u8;
                let g = g_scale.value().round().clamp(0.0, 255.0) as u8;
                let b = b_scale.value().round().clamp(0.0, 255.0) as u8;
                {
                    let mut s = state.borrow_mut();
                    s.pen.color = journal_core::Color { r, g, b, a: 255 };
                }
                (persist_slot)([r, g, b, 255]);
            })
        };
        for s in [&r_scale, &g_scale, &b_scale] {
            let on_change = on_change.clone();
            s.connect_value_changed(move |_| (on_change)());
        }

        // Tap slot → activate slot color + open inline picker. Empty
        // slots open the picker without touching pen.color so a tap
        // becomes a "fill this slot" path — the user picks an RGB and
        // the slot saves on first scale change.
        {
            let state = state.clone();
            let picker = picker.clone();
            let slot_color = slot_color.clone();
            let r_scale = r_scale.clone();
            let g_scale = g_scale.clone();
            let b_scale = b_scale.clone();
            let active_picker = active_picker.clone();
            btn.connect_clicked(move |_| {
                let [r, g, b, a] = slot_color.get();
                if a > 0 {
                    let mut s = state.borrow_mut();
                    s.pen.color = journal_core::Color { r, g, b, a };
                    if !crate::state::tool_is_drawing(s.tool) {
                        s.tool = Tool::Pen;
                    }
                }
                let init_r = if a > 0 { r } else { 20 };
                let init_g = if a > 0 { g } else { 20 };
                let init_b = if a > 0 { b } else { 20 };
                r_scale.set_value(init_r as f64);
                g_scale.set_value(init_g as f64);
                b_scale.set_value(init_b as f64);
                if let Some(prev) = active_picker.borrow_mut().take() {
                    if prev != picker {
                        prev.popdown();
                    }
                }
                picker.popup();
                *active_picker.borrow_mut() = Some(picker.clone());
            });
        }

        // Long-press → palette popover. Touch users get a discoverable
        // path to the per-tool palette (audit §7) without losing the
        // existing tap-to-pick RGB behaviour.
        {
            let state = state.clone();
            let slot_color = slot_color.clone();
            let active_picker = active_picker.clone();
            let btn_for_palette = btn.clone();
            let long = GestureLongPress::new();
            long.set_propagation_phase(PropagationPhase::Capture);
            long.set_touch_only(false);
            let persist_slot_for_lp = persist_slot.clone();
            let swatch_for_lp = swatch.clone();
            long.connect_pressed(move |_, _, _| {
                if let Some(prev) = active_picker.borrow_mut().take() {
                    prev.popdown();
                }
                let popover = build_palette_popover(
                    state.clone(),
                    slot_color.clone(),
                    persist_slot_for_lp.clone(),
                    swatch_for_lp.clone(),
                );
                popover.set_parent(&btn_for_palette);
                // Tear the popover off the button on close so repeated
                // long-presses don't pile up child popovers under the slot.
                popover.connect_closed(|p| p.unparent());
                popover.popup();
                *active_picker.borrow_mut() = Some(popover);
            });
            btn.add_controller(long);
        }

        row.append(&btn);
    }

    // Tick callback: dismiss the open picker the moment the user starts
    // drawing, since they've decided "use the color I just picked, don't
    // keep editing the slot."
    {
        let state = state.clone();
        let active_picker = active_picker.clone();
        let was_drawing: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        row.add_tick_callback(move |_, _| {
            let drawing = state.borrow().pointer_drawing;
            if drawing && !was_drawing.get() {
                if let Some(p) = active_picker.borrow_mut().take() {
                    p.popdown();
                }
            }
            was_drawing.set(drawing);
            gtk4::glib::ControlFlow::Continue
        });
    }

    row
}
