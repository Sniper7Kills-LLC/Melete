//! Account group for the App Settings preferences window. Renders
//! "Not configured" / "Signed in as …" / "Sign in" rows backed by
//! `journal_storage::remote_template_store::RemoteTemplateStore`.
//!
//! Gated on the `remote` feature so non-cloud builds drop the entry
//! entirely (no dead UI shown).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box as GtkBox, Button, Entry, Label, Orientation, Window};
use libadwaita as adw;
use libadwaita::prelude::*;

use journal_storage::remote_template_store::store::{RemoteError, RemoteTemplateStore};

/// Populate `group` with one of three states:
///   1. *Not configured*: `amplify_outputs.json` missing required fields.
///      Show a hint pointing the user at `npx ampx sandbox`.
///   2. *Signed out*: render a "Sign In" action that pops a modal.
///   3. *Signed in*: render a "Signed in" status row + "Sign Out".
///
/// Each interaction reconnects via `RemoteTemplateStore::connect`,
/// so the persisted tokens on disk are the source of truth.
pub fn populate_account_group(parent: &ApplicationWindow, group: &adw::PreferencesGroup) {
    rebuild(parent, group);
}

fn rebuild(parent: &ApplicationWindow, group: &adw::PreferencesGroup) {
    // adw::PreferencesGroup doesn't expose a "remove all children"
    // helper, so we reset by walking and removing.
    while let Some(row) = first_row(group) {
        group.remove(&row);
    }

    match RemoteTemplateStore::connect() {
        Err(RemoteError::Config(_)) => add_not_configured_row(group),
        Err(e) => add_error_row(group, &format!("{e}")),
        Ok(store) => {
            if store.is_signed_in() {
                add_signed_in_rows(parent, group);
            } else {
                add_signed_out_rows(parent, group);
            }
        }
    }
}

fn first_row(group: &adw::PreferencesGroup) -> Option<gtk4::Widget> {
    // adw::PreferencesGroup wraps a ListBox; iterate via the underlying
    // GtkWidget tree until we find any added row.
    let mut child = group.first_child();
    while let Some(c) = child {
        if let Some(lb) = c.downcast_ref::<gtk4::ListBox>() {
            return lb.first_child().map(|x| x.upcast::<gtk4::Widget>());
        }
        child = c.next_sibling();
    }
    None
}

fn add_not_configured_row(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Remote backend not configured")
        .subtitle(
            "Run `npx ampx sandbox` from the project root and rebuild \
             to enable sign-in and template publishing.",
        )
        .build();
    group.add(&row);
}

fn add_error_row(group: &adw::PreferencesGroup, msg: &str) {
    let row = adw::ActionRow::builder()
        .title("Remote backend error")
        .subtitle(msg)
        .build();
    group.add(&row);
}

fn add_signed_in_rows(parent: &ApplicationWindow, group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Signed in")
        .subtitle("Tokens stored at ~/.config/journal/auth.toml")
        .build();
    let sign_out = Button::with_label("Sign out");
    sign_out.add_css_class("destructive-action");
    sign_out.set_valign(gtk4::Align::Center);
    {
        let parent = parent.clone();
        let group = group.clone();
        sign_out.connect_clicked(move |_| {
            match RemoteTemplateStore::connect().and_then(|mut s| s.sign_out()) {
                Ok(()) => rebuild(&parent, &group),
                Err(e) => tracing::warn!("sign_out: {e}"),
            }
        });
    }
    row.add_suffix(&sign_out);
    group.add(&row);
}

fn add_signed_out_rows(parent: &ApplicationWindow, group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Not signed in")
        .subtitle("Sign in to your Cognito User Pool account.")
        .build();
    let sign_in = Button::with_label("Sign in…");
    sign_in.add_css_class("suggested-action");
    sign_in.set_valign(gtk4::Align::Center);
    {
        let parent = parent.clone();
        let group = group.clone();
        sign_in.connect_clicked(move |_| {
            prompt_sign_in(&parent, group.clone());
        });
    }
    row.add_suffix(&sign_in);
    group.add(&row);
}

fn prompt_sign_in(parent: &ApplicationWindow, group: adw::PreferencesGroup) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Sign in")
        .default_width(360)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    let email_lbl = Label::builder()
        .label("Email")
        .halign(gtk4::Align::Start)
        .build();
    let email = Entry::builder().placeholder_text("you@example.com").build();
    let pw_lbl = Label::builder()
        .label("Password")
        .halign(gtk4::Align::Start)
        .build();
    let pw = Entry::builder().visibility(false).build();

    let status = Label::builder()
        .label("")
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    status.add_css_class("dim-label");

    body.append(&email_lbl);
    body.append(&email);
    body.append(&pw_lbl);
    body.append(&pw);
    body.append(&status);

    let btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build();
    let cancel_btn = Button::with_label("Cancel");
    let signin_btn = Button::with_label("Sign in");
    signin_btn.add_css_class("suggested-action");
    btn_row.append(&cancel_btn);
    btn_row.append(&signin_btn);
    body.append(&btn_row);

    {
        let win = win.clone();
        cancel_btn.connect_clicked(move |_| win.close());
    }

    let in_flight = Rc::new(RefCell::new(false));
    {
        let win = win.clone();
        let parent = parent.clone();
        let group = group.clone();
        let email = email.clone();
        let pw = pw.clone();
        let status = status.clone();
        let signin_btn_inner = signin_btn.clone();
        let in_flight = in_flight.clone();
        signin_btn.connect_clicked(move |_| {
            let signin_btn = signin_btn_inner.clone();
            if *in_flight.borrow() {
                return;
            }
            *in_flight.borrow_mut() = true;
            signin_btn.set_sensitive(false);
            status.set_label("Signing in…");

            // Sign-in is a blocking HTTPS call that can take seconds.
            // Rather than pulling in a futures runtime, run it
            // synchronously and accept a brief UI hiccup — the user
            // is intentionally waiting on it. If this becomes an
            // issue, hoist into a worker thread + glib::idle_add.
            let username = email.text().to_string();
            let password = pw.text().to_string();
            let result = RemoteTemplateStore::connect()
                .and_then(|mut s| s.sign_in(&username, &password));
            *in_flight.borrow_mut() = false;
            signin_btn.set_sensitive(true);
            match result {
                Ok(()) => {
                    win.close();
                    rebuild(&parent, &group);
                }
                Err(e) => {
                    status.set_label(&format!("Sign-in failed: {e}"));
                }
            }
        });
    }

    win.set_child(Some(&body));
    win.present();
}
