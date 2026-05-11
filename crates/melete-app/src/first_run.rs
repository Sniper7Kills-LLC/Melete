//! First-run sign-in / skip welcome window.
//!
//! Shown once on the very first launch after install (or any boot where
//! `AppConfig::first_run_completed` is still `false`) before the main
//! application window is presented. The user picks one of two paths:
//!
//! - **Sign In** — placeholder for now. Real Cognito-backed sign-in
//!   ships with the `RemoteTemplateStore` work (issues #6 / #7); this
//!   button currently shows a "coming soon" notice and returns to the
//!   welcome window. The button is intentionally left in place so a
//!   future PR can swap the placeholder for the real login modal.
//! - **Skip** — closes the welcome window and lets Melete proceed in
//!   built-ins-only / offline mode.
//!
//! Either path persists `first_run_completed = true` so subsequent
//! launches go straight to the main window.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box as GtkBox, Button, Label, Orientation};
use libadwaita as adw;
use libadwaita::prelude::AdwWindowExt;

/// Pure boot-path predicate: should the welcome window be shown given
/// the current config? Split out so the boot logic is unit-testable
/// without touching GTK.
pub fn should_show(cfg: &crate::config::AppConfig) -> bool {
    !cfg.first_run_completed
}

/// Mark the welcome window as dispatched and persist the change.
fn mark_completed() {
    let mut cfg = crate::config::load();
    if cfg.first_run_completed {
        return;
    }
    cfg.first_run_completed = true;
    if let Err(e) = crate::config::save(&cfg) {
        tracing::warn!("failed to persist first_run_completed: {}", e);
    }
}

/// Show the first-run welcome window if the user has not yet dispatched
/// it, then invoke `on_complete` once the user picks a path. When the
/// flag is already set the callback fires synchronously and no window is
/// presented — call sites can unconditionally pass control through this
/// helper.
///
/// Also auto-skips (and persists `first_run_completed`) when the user
/// is already signed in via the keyring/file token store: the welcome
/// copy is sign-in-centric, so showing it post-auth is just noise.
pub fn show_if_needed<F: FnOnce() + 'static>(parent: &ApplicationWindow, on_complete: F) {
    let cfg = crate::config::load();
    if !should_show(&cfg) {
        on_complete();
        return;
    }
    #[cfg(feature = "remote")]
    {
        if crate::sign_in_modal::is_signed_in() {
            mark_completed();
            on_complete();
            return;
        }
    }
    show(parent, on_complete);
}

fn show<F: FnOnce() + 'static>(parent: &ApplicationWindow, on_complete: F) {
    let win = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Welcome to Melete")
        .default_width(480)
        .default_height(360)
        .resizable(false)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .margin_top(24)
        .margin_bottom(20)
        .margin_start(28)
        .margin_end(28)
        .build();

    let header = Label::builder()
        .label("Welcome to Melete")
        .halign(gtk4::Align::Center)
        .build();
    header.add_css_class("title-2");
    body.append(&header);

    let copy_signin = Label::builder()
        .label(
            "Sign in to download planner templates, brush packs, and \
             sync your work across devices.",
        )
        .wrap(true)
        .xalign(0.0)
        .build();
    body.append(&copy_signin);

    let copy_skip = Label::builder()
        .label(
            "Or skip and use Melete offline with the basic templates \
             and tools that ship with the app.",
        )
        .wrap(true)
        .xalign(0.0)
        .build();
    body.append(&copy_skip);

    let spacer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .vexpand(true)
        .build();
    body.append(&spacer);

    let btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(12)
        .halign(gtk4::Align::Center)
        .build();

    let sign_in_btn = Button::builder().label("Sign In").build();
    sign_in_btn.set_size_request(140, 36);

    let skip_btn = Button::builder().label("Skip").build();
    skip_btn.set_size_request(140, 36);
    // Skip is the lower-friction default per the spec.
    skip_btn.add_css_class("suggested-action");

    btn_row.append(&sign_in_btn);
    btn_row.append(&skip_btn);
    body.append(&btn_row);

    let header_bar = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header_bar);
    toolbar_view.set_content(Some(&body));
    win.set_content(Some(&toolbar_view));

    // Single-shot wrapper so headerbar X, Skip, and Sign-In success all
    // converge on exactly one `mark_completed` + one `on_complete` call.
    let on_complete: Rc<Cell<Option<Box<dyn FnOnce()>>>> =
        Rc::new(Cell::new(Some(Box::new(on_complete))));
    let finish = {
        let on_complete = on_complete.clone();
        let win = win.clone();
        move || {
            mark_completed();
            if let Some(cb) = on_complete.replace(None) {
                cb();
            }
            win.close();
        }
    };

    {
        let finish = finish.clone();
        skip_btn.connect_clicked(move |_| finish());
    }
    {
        let parent = parent.clone();
        let finish = finish.clone();
        sign_in_btn.connect_clicked(move |_| {
            #[cfg(feature = "remote")]
            {
                let finish = finish.clone();
                crate::sign_in_modal::open(
                    &parent,
                    Box::new(move |signed_in| {
                        if signed_in {
                            finish();
                        }
                    }),
                );
            }
            #[cfg(not(feature = "remote"))]
            {
                let _ = (&parent, &finish);
            }
        });
    }
    {
        let on_complete = on_complete.clone();
        win.connect_close_request(move |_| {
            // Closing via the headerbar X is treated like Skip — we still
            // record the flag so the welcome window doesn't reappear.
            mark_completed();
            if let Some(cb) = on_complete.replace(None) {
                cb();
            }
            gtk4::glib::Propagation::Proceed
        });
    }

    win.present();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[test]
    fn should_show_when_flag_unset() {
        let cfg = AppConfig::default();
        assert!(!cfg.first_run_completed);
        assert!(should_show(&cfg));
    }

    #[test]
    fn should_skip_when_flag_set() {
        let cfg = AppConfig {
            first_run_completed: true,
            ..AppConfig::default()
        };
        assert!(!should_show(&cfg));
    }
}
