use gtk4::gdk::RGBA;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, ColorDialog, ColorDialogButton, Label, Orientation, Scale};

use crate::state::SharedState;

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
            state.borrow_mut().pen.color = journal_core::Color {
                r: (rgba.red() * 255.0) as u8,
                g: (rgba.green() * 255.0) as u8,
                b: (rgba.blue() * 255.0) as u8,
                a: (rgba.alpha() * 255.0) as u8,
            };
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
            state.borrow_mut().pen.base_width = s.value();
        });
    }
    bar.append(&scale);

    bar
}
