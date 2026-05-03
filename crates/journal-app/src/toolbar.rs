use gtk4::gdk::RGBA;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, ColorDialog, ColorDialogButton, Label, Orientation, Scale, Separator,
    ToggleButton,
};

use crate::state::{SharedState, Tool};

pub fn build_toolbar(state: SharedState) -> GtkBox {
    let bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::End)
        .margin_bottom(16)
        .build();
    bar.add_css_class("osd");
    bar.add_css_class("toolbar");

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
        let pen_btn2 = pen_btn.clone();
        let _ = pen_btn2;
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

    bar
}
