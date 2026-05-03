use std::cell::Cell;
use std::rc::Rc;

use gtk4::gdk::RGBA;
use gtk4::graphene::Point as GraphenePoint;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, ColorDialog, ColorDialogButton, GestureDrag, Image, Orientation,
    PropagationPhase, Scale, Separator, ToggleButton,
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
pub fn build_toolbar(state: SharedState) -> GtkBox {
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

    // ── Tool buttons ──────────────────────────────────────────────────────
    let pen_btn = ToggleButton::builder()
        .icon_name("document-edit-symbolic")
        .tooltip_text("Pen (B)")
        .active(true)
        .build();

    let highlighter_btn = ToggleButton::builder()
        .icon_name("marker-symbolic")
        .tooltip_text("Highlighter (H)")
        .group(&pen_btn)
        .build();

    let eraser_btn = ToggleButton::builder()
        .icon_name("edit-clear-symbolic")
        .tooltip_text("Eraser — stroke (Ctrl+E)")
        .group(&pen_btn)
        .build();

    let partial_eraser_btn = ToggleButton::builder()
        .icon_name("edit-cut-symbolic")
        .tooltip_text("Partial Eraser — splits strokes")
        .group(&pen_btn)
        .build();

    let selection_btn = ToggleButton::builder()
        .icon_name("edit-select-all-symbolic")
        .tooltip_text("Selection (V)")
        .group(&pen_btn)
        .build();

    {
        let state = state.clone();
        pen_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                crate::state::set_tool_pen(&state);
            }
        });
    }
    {
        let state = state.clone();
        highlighter_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                crate::state::set_tool_highlighter(&state);
            }
        });
    }
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

    for b in [&pen_btn, &highlighter_btn, &eraser_btn, &partial_eraser_btn, &selection_btn] {
        b.add_css_class("compact-tool");
    }

    // ── Color picker ──────────────────────────────────────────────────────
    let initial = state.borrow().pen.color;
    let initial_rgba = RGBA::new(
        initial.r as f32 / 255.0,
        initial.g as f32 / 255.0,
        initial.b as f32 / 255.0,
        initial.a as f32 / 255.0,
    );

    let dialog = ColorDialog::builder().with_alpha(true).build();
    let color_btn = ColorDialogButton::new(Some(dialog));
    color_btn.set_rgba(&initial_rgba);
    {
        let state = state.clone();
        color_btn.connect_rgba_notify(move |btn| {
            let rgba = btn.rgba();
            let color = journal_core::Color {
                r: (rgba.red() * 255.0) as u8,
                g: (rgba.green() * 255.0) as u8,
                b: (rgba.blue() * 255.0) as u8,
                a: (rgba.alpha() * 255.0) as u8,
            };
            let mut s = state.borrow_mut();
            s.pen.color = color;
            s.saved_pen_color = color;
        });
    }

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
            let mut st = state.borrow_mut();
            st.pen.base_width = v;
            st.saved_pen_width = v;
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

    // The "extra" box holds everything that should hide when collapsed:
    // tool buttons, separator, color, scale, separator before chevron.
    let extras = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .build();

    // Pen tool buttons go in extras
    for b in [&pen_btn, &highlighter_btn, &eraser_btn, &partial_eraser_btn, &selection_btn] {
        extras.append(b);
    }
    extras.append(&Separator::new(Orientation::Vertical));
    extras.append(&color_btn);
    extras.append(&scale);

    // "Collapsed active tool" icon — shown only when collapsed.
    // We use a Button so clicking it also expands the toolbar.
    let collapsed_tool_btn = Button::builder()
        .icon_name("document-edit-symbolic")
        .tooltip_text("Active tool — click to expand")
        .build();
    collapsed_tool_btn.add_css_class("compact-tool");

    bar.append(&collapsed_tool_btn);
    bar.append(&extras);
    bar.append(&chevron_btn);

    // Apply initial visibility based on saved collapsed state.
    extras.set_visible(!cfg.toolbar_collapsed);
    collapsed_tool_btn.set_visible(cfg.toolbar_collapsed);

    // `icon_for_tool` is defined at the bottom of this module; call it directly.

    // Tick callback to keep the collapsed-tool icon in sync with current tool.
    {
        let state = state.clone();
        let collapsed_btn = collapsed_tool_btn.clone();
        let collapsed_cell = collapsed.clone();
        let last_tool: Rc<Cell<Tool>> = Rc::new(Cell::new(state.borrow().tool));
        bar.add_tick_callback(move |_, _| {
            let cur = state.borrow().tool;
            if collapsed_cell.get() && cur != last_tool.get() {
                last_tool.set(cur);
                collapsed_btn.set_icon_name(icon_for_tool(cur));
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    // Toggle collapse on chevron click.
    {
        let extras = extras.clone();
        let collapsed_tool_btn = collapsed_tool_btn.clone();
        let collapsed = collapsed.clone();
        let chevron_btn2 = chevron_btn.clone();
        let state = state.clone();
        chevron_btn.connect_clicked(move |_| {
            let now = !collapsed.get();
            collapsed.set(now);
            extras.set_visible(!now);
            collapsed_tool_btn.set_visible(now);
            if now {
                // Update collapsed icon immediately.
                collapsed_tool_btn.set_icon_name(icon_for_tool(state.borrow().tool));
                chevron_btn2.set_icon_name("go-previous-symbolic");
            } else {
                chevron_btn2.set_icon_name("go-next-symbolic");
            }
            // Persist
            let mut cfg = crate::config::load();
            cfg.toolbar_collapsed = now;
            if let Err(e) = crate::config::save(&cfg) {
                tracing::warn!("Failed to save toolbar collapsed state: {}", e);
            }
        });
    }

    // Toggle expand on collapsed-tool-btn click.
    {
        let extras = extras.clone();
        let collapsed_tool_btn2 = collapsed_tool_btn.clone();
        let collapsed = collapsed.clone();
        let chevron_btn2 = chevron_btn.clone();
        collapsed_tool_btn.connect_clicked(move |_| {
            // Always expand.
            collapsed.set(false);
            extras.set_visible(true);
            collapsed_tool_btn2.set_visible(false);
            chevron_btn2.set_icon_name("go-next-symbolic");
            let mut cfg = crate::config::load();
            cfg.toolbar_collapsed = false;
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

/// Returns the symbolic icon name for the given tool.
fn icon_for_tool(tool: Tool) -> &'static str {
    match tool {
        Tool::Pen => "document-edit-symbolic",
        Tool::Highlighter => "marker-symbolic",
        Tool::Eraser(crate::state::EraserMode::Stroke) => "edit-clear-symbolic",
        Tool::Eraser(crate::state::EraserMode::Partial) => "edit-cut-symbolic",
        Tool::Selection => "edit-select-all-symbolic",
    }
}
