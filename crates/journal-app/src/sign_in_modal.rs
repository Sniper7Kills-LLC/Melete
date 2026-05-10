//! Reusable Cognito sign-in modal. Used by:
//!   * `first_run::show` — welcome window's "Sign in" path
//!   * `account_settings` — Account preferences group's "Sign in"
//!
//! Behaviour:
//!   * Email + password entries, "Sign in" + "Cancel" buttons
//!   * On Cancel or window-close: `on_close` fires (with `signed_in=false`)
//!   * On successful sign-in: tokens persist via `RemoteTemplateStore::sign_in`,
//!     modal closes, `on_close` fires (with `signed_in=true`)
//!   * Failures stay in the modal with an inline status line — the user
//!     can retry without losing what they typed
//!
//! Gated on `feature = "remote"`; non-cloud builds drop the file via
//! the parent module's cfg.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box as GtkBox, Button, Entry, Label, Orientation, Window};

use journal_storage::remote_template_store::store::RemoteTemplateStore;

/// Open the modal. `on_close(signed_in)` is invoked exactly once:
///   * `signed_in == true` after a successful sign-in
///   * `signed_in == false` on Cancel / window close without sign-in
pub fn open(parent: &ApplicationWindow, on_close: Box<dyn FnOnce(bool)>) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Sign in to Journal")
        .default_width(360)
        .resizable(false)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();

    let header = Label::builder()
        .label("Sign in to your Cognito User Pool account")
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    header.add_css_class("dim-label");
    body.append(&header);

    let email_lbl = Label::builder()
        .label("Email")
        .halign(gtk4::Align::Start)
        .build();
    let email = Entry::builder()
        .placeholder_text("you@example.com")
        .activates_default(true)
        .build();
    let pw_lbl = Label::builder()
        .label("Password")
        .halign(gtk4::Align::Start)
        .build();
    let pw = Entry::builder()
        .visibility(false)
        .activates_default(true)
        .build();

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
        .margin_top(4)
        .build();
    let cancel_btn = Button::with_label("Cancel");
    let signin_btn = Button::with_label("Sign in");
    signin_btn.add_css_class("suggested-action");
    signin_btn.set_receives_default(true);
    btn_row.append(&cancel_btn);
    btn_row.append(&signin_btn);
    body.append(&btn_row);

    win.set_child(Some(&body));
    win.set_default_widget(Some(&signin_btn));

    // Single-shot callback so Cancel, X, and successful sign-in all
    // converge on exactly one invocation.
    let on_close: Rc<RefCell<Option<Box<dyn FnOnce(bool)>>>> = Rc::new(RefCell::new(Some(on_close)));
    let fire = {
        let on_close = on_close.clone();
        move |signed_in: bool| {
            if let Some(cb) = on_close.borrow_mut().take() {
                cb(signed_in);
            }
        }
    };

    {
        let win = win.clone();
        let fire = fire.clone();
        cancel_btn.connect_clicked(move |_| {
            fire(false);
            win.close();
        });
    }

    {
        let win = win.clone();
        let fire = fire.clone();
        win.connect_close_request(move |_| {
            fire(false);
            gtk4::glib::Propagation::Proceed
        });
    }

    let in_flight = Rc::new(RefCell::new(false));
    {
        let win = win.clone();
        let signin_btn_inner = signin_btn.clone();
        let email = email.clone();
        let pw = pw.clone();
        let status = status.clone();
        let in_flight = in_flight.clone();
        let fire = fire.clone();
        signin_btn.connect_clicked(move |_| {
            if *in_flight.borrow() {
                return;
            }
            let signin_btn = signin_btn_inner.clone();
            *in_flight.borrow_mut() = true;
            signin_btn.set_sensitive(false);
            status.remove_css_class("error");
            status.set_label("Signing in…");

            let username = email.text().to_string();
            let password = pw.text().to_string();
            // Blocking HTTPS call on the GTK main thread. Sub-second
            // over a healthy connection; the user is intentionally
            // waiting on it. If this becomes a UX issue, hoist into a
            // worker thread + glib::idle_add.
            let result = RemoteTemplateStore::connect()
                .and_then(|mut s| s.sign_in(&username, &password));
            *in_flight.borrow_mut() = false;
            signin_btn.set_sensitive(true);
            match result {
                Ok(()) => {
                    fire(true);
                    win.close();
                }
                Err(e) => {
                    status.add_css_class("error");
                    status.set_label(&format!("Sign-in failed: {e}"));
                }
            }
        });
    }

    win.present();
}

/// Cheap probe: is anyone currently signed in? Reads the persisted
/// token bundle (keyring → file fallback). Doesn't touch the network.
pub fn is_signed_in() -> bool {
    RemoteTemplateStore::connect()
        .map(|s| s.is_signed_in())
        .unwrap_or(false)
}

/// Best-effort: return the current user's email, decoded from the
/// `id_token` JWT's `email` claim. `None` if not signed in or if the
/// claim is missing. Doesn't validate the JWT signature — we only use
/// it for display.
pub fn current_email() -> Option<String> {
    let store = RemoteTemplateStore::connect().ok()?;
    if !store.is_signed_in() {
        return None;
    }
    let tokens = journal_storage::remote_template_store::auth::load_tokens().ok().flatten()?;
    decode_jwt_email(&tokens.id_token)
}

fn decode_jwt_email(jwt: &str) -> Option<String> {
    use base64::Engine;
    let payload_b64 = jwt.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("email").and_then(|x| x.as_str()).map(|s| s.to_string())
}
