use std::cell::Cell;
use std::rc::Rc;

use gtk4::gdk::RGBA;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, ColorDialog, ColorDialogButton, GestureDrag, Image, Label, Orientation,
    PropagationPhase, Scale, Separator, ToggleButton,
};

use crate::state::{SharedState, Tool};

/// Build the floating pen toolbar.
///
/// The returned widget is a `GtkBox` positioned via `margin_start` / `margin_top`
/// inside a `gtk4::Overlay` with `halign(Start)` + `valign(Start)`.  A
/// `GestureDrag` attached to the grip handle (left-most icon) lets the user
/// reposition it; the final position is saved to `~/.config/journal/config.toml`.
pub fn build_toolbar(state: SharedState) -> GtkBox {
    // ── Outer wrapper: positioned inside the Overlay via margins ─────────
    let cfg = crate::config::load();

    let bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::Start)
        .valign(gtk4::Align::Start)
        .build();
    bar.add_css_class("osd");
    bar.add_css_class("toolbar");

    // ── Drag handle ───────────────────────────────────────────────────────
    let handle = Image::from_icon_name("open-menu-symbolic");
    handle.set_tooltip_text(Some("Drag to move toolbar"));
    handle.set_margin_start(4);
    handle.set_margin_end(4);
    // Give the handle a pointer cursor so the user knows it's draggable.
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

    bar.append(&pen_btn);
    bar.append(&highlighter_btn);
    bar.append(&eraser_btn);
    bar.append(&partial_eraser_btn);
    bar.append(&selection_btn);

    bar.append(&Separator::new(Orientation::Vertical));

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
    bar.append(&color_btn);

    bar.append(&Label::new(Some("Width")));

    // ── Width scale ───────────────────────────────────────────────────────
    let scale = Scale::with_range(Orientation::Horizontal, 0.5, 12.0, 0.5);
    scale.set_value(state.borrow().pen.base_width);
    scale.set_width_request(160);
    scale.set_draw_value(true);
    {
        let state = state.clone();
        scale.connect_value_changed(move |s| {
            let v = s.value();
            let mut st = state.borrow_mut();
            st.pen.base_width = v;
            st.saved_pen_width = v;
        });
    }
    bar.append(&scale);

    // ── Restore saved position or default to bottom-centre ───────────────
    // Default bottom-centre is computed lazily on first map using the
    // overlay's allocated size.  If we have a saved position we apply it
    // immediately; otherwise we wait for the "map" signal.
    if let (Some(x), Some(y)) = (cfg.toolbar_x, cfg.toolbar_y) {
        bar.set_margin_start(x);
        bar.set_margin_top(y);
    } else {
        // Park at a plausible bottom-centre default.  We cannot know the
        // exact overlay size before the widget is mapped, so we connect to
        // the "map" signal to compute and apply the true default once.
        let bar_for_map = bar.clone();
        bar.connect_map(move |_| {
            // Only position if we haven't been given explicit margins yet.
            if bar_for_map.margin_start() == 0 && bar_for_map.margin_top() == 0 {
                if let Some(parent) = bar_for_map.parent() {
                    let pw = parent.allocated_width();
                    let ph = parent.allocated_height();
                    let bw = bar_for_map.allocated_width();
                    let bh = bar_for_map.allocated_height();
                    let x = ((pw - bw) / 2).max(0);
                    let y = (ph - bh - 16).max(0);
                    bar_for_map.set_margin_start(x);
                    bar_for_map.set_margin_top(y);
                }
            }
        });
    }

    // ── GestureDrag on the handle only ────────────────────────────────────
    let drag = GestureDrag::builder()
        .propagation_phase(PropagationPhase::Capture)
        .build();

    // Shared state for drag origin (margin at drag start).
    let origin_x: Rc<Cell<i32>> = Rc::new(Cell::new(0));
    let origin_y: Rc<Cell<i32>> = Rc::new(Cell::new(0));

    {
        let bar_ref = bar.clone();
        let ox = origin_x.clone();
        let oy = origin_y.clone();
        drag.connect_drag_begin(move |_gesture, _start_x, _start_y| {
            ox.set(bar_ref.margin_start());
            oy.set(bar_ref.margin_top());
        });
    }

    {
        let bar_ref = bar.clone();
        let ox = origin_x.clone();
        let oy = origin_y.clone();
        drag.connect_drag_update(move |_gesture, dx, dy| {
            let new_x = (ox.get() + dx as i32).max(0);
            let new_y = (oy.get() + dy as i32).max(0);

            // Clamp so at least the handle stays on-screen.
            let (new_x, new_y) = if let Some(parent) = bar_ref.parent() {
                let pw = parent.allocated_width();
                let ph = parent.allocated_height();
                let bw = bar_ref.allocated_width().max(48);
                let bh = bar_ref.allocated_height().max(32);
                let max_x = (pw - bw.min(pw)).max(0);
                let max_y = (ph - bh.min(ph)).max(0);
                (new_x.min(max_x), new_y.min(max_y))
            } else {
                (new_x, new_y)
            };

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
