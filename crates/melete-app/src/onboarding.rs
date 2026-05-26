//! First-launch guided tour + post-upgrade "What's new" panes.
//!
//! Both surfaces are gated by `AppConfig` flags and shown by `build_ui`
//! after the main window is presented:
//!
//! - [`show_first_launch_tour`] runs while `AppConfig::tour_dismissed`
//!   is `false` and writes the flag back to `true` on dismiss. The
//!   tour is a 4-card `adw::Carousel` covering the basics: pick a
//!   notebook template → draw → switch tools → save palette colors.
//! - [`show_whats_new_if_upgraded`] compares
//!   `AppConfig::last_seen_version` to the compiled
//!   `CARGO_PKG_VERSION` and, on mismatch, opens a dialog listing the
//!   user-visible changes since the user last saw the pane. Overwrites
//!   `last_seen_version` on dismiss so each version shows at most once.
//!
//! These are deliberately small — the goal is a discoverability nudge,
//! not a tutorial. Audit §11.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box as GtkBox, Button, Label, Orientation};
use libadwaita as adw;
use libadwaita::prelude::AdwWindowExt;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

const TOUR_CARDS: &[(&str, &str)] = &[
    (
        "Pick a notebook template",
        "From the Home view, choose Templates → New notebook. The Notebook \
         Template editor lets you compose Year / Quarter / Month / Week / \
         Day pages out of page templates. Pre-built planners are in the \
         template gallery.",
    ),
    (
        "Draw with stylus, finger, or mouse",
        "Pen, Highlighter, Pencil, Paintbrush, Spray Can, and Calligraphy \
         brushes all live in the floating toolbar. The toolbar drag handle \
         is amber on touch — grab it with one finger to reposition.",
    ),
    (
        "Switch tools without breaking flow",
        "Tap a tool in the floating toolbar, or press B (Pen), H (Highlighter), \
         E (Eraser cycle), V (Selection). The active tool pops out — the \
         previously-selected slot drops back. Per-tool brush recipes are \
         editable from the hamburger menu's Tools… entry.",
    ),
    (
        "Save your palette",
        "Long-press any color slot in the toolbar to open the per-tool \
         palette. Tap \"Save to palette\" to keep the current color, then \
         tap a swatch later to bring it back. Each drawing tool has its \
         own palette.",
    ),
];

/// Show the 4-card first-launch tour as a modal `adw::Window` if the
/// user has not yet dismissed it. Writes
/// `AppConfig::tour_dismissed = true` on close.
pub fn show_first_launch_tour(parent: &ApplicationWindow) {
    let cfg = crate::config::load();
    if cfg.tour_dismissed {
        return;
    }

    let win = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Welcome to Melete")
        .default_width(560)
        .default_height(420)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(16)
        .margin_start(20)
        .margin_end(20)
        .build();

    let header = Label::builder()
        .label("<b>Welcome to Melete</b>")
        .use_markup(true)
        .halign(gtk4::Align::Start)
        .build();
    header.add_css_class("title-3");
    body.append(&header);

    let carousel = adw::Carousel::new();
    carousel.set_vexpand(true);
    for (title, blurb) in TOUR_CARDS {
        let card = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(24)
            .margin_end(24)
            .valign(gtk4::Align::Center)
            .build();
        let t = Label::builder()
            .label(*title)
            .halign(gtk4::Align::Start)
            .build();
        t.add_css_class("title-4");
        card.append(&t);
        let b = Label::builder()
            .label(*blurb)
            .wrap(true)
            .xalign(0.0)
            .build();
        card.append(&b);
        carousel.append(&card);
    }
    body.append(&carousel);

    let dots = adw::CarouselIndicatorDots::builder()
        .carousel(&carousel)
        .build();
    body.append(&dots);

    let btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build();
    let next_btn = Button::with_label("Next");
    next_btn.add_css_class("suggested-action");
    let dismiss_btn = Button::with_label("Skip tour");
    btn_row.append(&dismiss_btn);
    btn_row.append(&next_btn);
    body.append(&btn_row);

    let total = TOUR_CARDS.len() as u32;

    {
        let carousel = carousel.clone();
        let next_btn = next_btn.clone();
        let win = win.clone();
        next_btn.connect_clicked(move |b| {
            let pos = carousel.position().round() as u32;
            if pos + 1 >= total {
                let mut cfg = crate::config::load();
                cfg.tour_dismissed = true;
                if let Err(e) = crate::config::save(&cfg) {
                    tracing::warn!("onboarding: tour_dismissed save: {e:#}");
                    crate::notify::toast(
                        format!("Couldn't save onboarding state: {e}"),
                        crate::notify::Severity::Warn,
                    );
                }
                win.close();
                return;
            }
            let child = carousel.nth_page(pos + 1);
            carousel.scroll_to(&child, true);
            let _ = b;
            // Update label below in the carousel notify hook.
        });
    }
    {
        let win = win.clone();
        dismiss_btn.connect_clicked(move |_| {
            let mut cfg = crate::config::load();
            cfg.tour_dismissed = true;
            if let Err(e) = crate::config::save(&cfg) {
                tracing::warn!("onboarding: tour_dismissed save: {e:#}");
                crate::notify::toast(
                    format!("Couldn't save onboarding state: {e}"),
                    crate::notify::Severity::Warn,
                );
            }
            win.close();
        });
    }
    // Update the next button label as the user advances — last card
    // calls it "Done" so the dismiss path isn't surprising.
    {
        let next_btn = next_btn.clone();
        carousel.connect_position_notify(move |c| {
            let pos = c.position().round() as u32;
            if pos + 1 >= total {
                next_btn.set_label("Done");
            } else {
                next_btn.set_label("Next");
            }
        });
    }
    // Also write tour_dismissed if the user closes via the headerbar X.
    {
        win.connect_close_request(|_| {
            let mut cfg = crate::config::load();
            cfg.tour_dismissed = true;
            if let Err(e) = crate::config::save(&cfg) {
                tracing::warn!("onboarding: tour_dismissed save (close): {e:#}");
                crate::notify::toast(
                    format!("Couldn't save onboarding state: {e}"),
                    crate::notify::Severity::Warn,
                );
            }
            gtk4::glib::Propagation::Proceed
        });
    }

    let header_bar = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header_bar);
    toolbar_view.set_content(Some(&body));
    win.set_content(Some(&toolbar_view));

    win.present();
}

/// Show the "What's new" pane if the persisted `last_seen_version`
/// differs from the running build's `CARGO_PKG_VERSION`. The pane is a
/// simple modal listing the most recent user-visible changes; the
/// content list lives inline in this file so packagers can grep
/// changelog entries to the version they ship.
pub fn show_whats_new_if_upgraded(parent: &ApplicationWindow) {
    let cfg = crate::config::load();
    if cfg.last_seen_version.as_deref() == Some(APP_VERSION) {
        return;
    }
    // First-ever launch: don't pile a "what's new" on top of the
    // first-launch tour. The tour fires from the same spot in
    // build_ui — if tour_dismissed is false, skip what's-new this boot.
    if !cfg.tour_dismissed {
        return;
    }

    let bullets: &[&str] = &[
        "Editorial fieldbook aesthetic — cream / dim-teal page surface, serif display font, amber chrome.",
        "Display font selectable in App settings (Noto, EB Garamond, Lora, Source Serif, Liberation, System).",
        "Long-press any toolbar color slot to open the per-tool palette and save the current color.",
        "Tool selection animates: active tool pops, zoom badge pulses on change, home grid staggers in.",
        "Page-template editor shows a live \"On a real page\" preview with dummy strokes.",
        "Notebook-template editor: bigger preview chips and a one-line summary status row.",
        "Tool Options popup no longer flashes on tool switch — internals page-stack with crossfade.",
        "App settings is now an adw::PreferencesWindow with auto-save.",
        "Empty-state placeholder rebuilt with branded wordmark, dot-grid, and amber underline.",
    ];

    let win = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(format!("What's new in {}", APP_VERSION))
        .default_width(560)
        .default_height(440)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .margin_top(20)
        .margin_bottom(16)
        .margin_start(20)
        .margin_end(20)
        .build();

    let header = Label::builder()
        .label(format!("<b>What's new in {}</b>", APP_VERSION))
        .use_markup(true)
        .halign(gtk4::Align::Start)
        .build();
    header.add_css_class("title-3");
    body.append(&header);

    let scroll = gtk4::ScrolledWindow::builder().vexpand(true).build();
    let list = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .margin_top(4)
        .margin_bottom(4)
        .build();
    for line in bullets {
        let row = Label::builder()
            .label(format!("•  {}", line))
            .wrap(true)
            .xalign(0.0)
            .build();
        list.append(&row);
    }
    scroll.set_child(Some(&list));
    body.append(&scroll);

    let close_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(gtk4::Align::End)
        .build();
    let close_btn = Button::with_label("Got it");
    close_btn.add_css_class("suggested-action");
    close_row.append(&close_btn);
    body.append(&close_row);

    // Single shared "mark seen" closure so headerbar X and the Got-it
    // button both update last_seen_version exactly once.
    let marked: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let mark_seen = {
        let marked = marked.clone();
        move || {
            if marked.replace(true) {
                return;
            }
            let mut cfg = crate::config::load();
            cfg.last_seen_version = Some(APP_VERSION.to_string());
            if let Err(e) = crate::config::save(&cfg) {
                tracing::warn!("onboarding: last_seen_version save: {e:#}");
                crate::notify::toast(
                    format!("Couldn't save changelog state: {e}"),
                    crate::notify::Severity::Warn,
                );
            }
        }
    };
    {
        let mark_seen = mark_seen.clone();
        let win = win.clone();
        close_btn.connect_clicked(move |_| {
            (mark_seen)();
            win.close();
        });
    }
    {
        let mark_seen = mark_seen.clone();
        win.connect_close_request(move |_| {
            (mark_seen)();
            gtk4::glib::Propagation::Proceed
        });
    }

    let header_bar = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header_bar);
    toolbar_view.set_content(Some(&body));
    win.set_content(Some(&toolbar_view));

    win.present();
}
