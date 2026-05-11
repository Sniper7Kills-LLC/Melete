//! Admin / superadmin panel. Gated visibility — the menu entry is
//! only added when the signed-in JWT carries the `admin` or
//! `superadmin` Cognito group. Beyond visibility, every server call
//! re-checks the group server-side via `@aws_auth`, so a client tamper
//! gets a `FORBIDDEN` instead of unauthorized access.
//!
//! Initial scope (intentionally lean — fuller user-list / detail /
//! audit views land in a follow-up):
//!   * Stats dashboard: read AdminStats singleton, render counts +
//!     MRR + last-updated.
//!   * User search by email: calls `adminSearchUsers` mutation,
//!     renders matching user rows.
//!   * Superadmin only: a "Mutate" row offers grantTier / setStatus
//!     / markEducation / resetDailyUsage with a `reason` prompt.
//!
//! Gated on the `remote` feature so non-cloud builds drop the entry.

use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box as GBox, Button, Entry, Label, ListBox, Orientation};
use libadwaita as adw;
use libadwaita::prelude::*;

use journal_storage::remote_template_store::store::{RemoteError, RemoteTemplateStore};
use serde_json::{json, Value};

const Q_ADMIN_STATS: &str = r#"
query AdminStats {
  getAdminStats(id: "global") {
    totalUsers
    freeUsers
    proUsers
    studioUsers
    trialingUsers
    pastDueUsers
    canceledUsers
    totalNotebooks
    mrrCents
    lastUpdatedIso
  }
}
"#;

const M_SEARCH_USERS: &str = r#"
mutation AdminSearchUsers($email: String!) {
  adminSearchUsers(email: $email) {
    items { userId email enabled status createdAtIso }
  }
}
"#;

const M_ADMIN_MUTATE: &str = r#"
mutation AdminMutate($action: String!, $targetUserId: String!, $payload: AWSJSON, $reason: String!) {
  adminMutate(action: $action, targetUserId: $targetUserId, payload: $payload, reason: $reason) {
    after
  }
}
"#;

/// Returns `true` if the signed-in user has admin or superadmin
/// rights. Cheap (decodes one JWT) — call from the header builder to
/// gate the menu entry.
pub fn current_user_is_admin() -> bool {
    let Ok(mut store) = RemoteTemplateStore::connect() else {
        return false;
    };
    if !store.is_signed_in() {
        return false;
    }
    let Ok(groups) = store.user_groups() else {
        return false;
    };
    groups.iter().any(|g| g == "admin" || g == "superadmin")
}

fn user_is_superadmin() -> bool {
    let Ok(mut store) = RemoteTemplateStore::connect() else {
        return false;
    };
    let Ok(groups) = store.user_groups() else {
        return false;
    };
    groups.iter().any(|g| g == "superadmin")
}

pub fn open(parent: &ApplicationWindow) {
    let window = gtk4::Window::builder()
        .transient_for(parent)
        .modal(false)
        .default_width(720)
        .default_height(640)
        .title("Admin")
        .build();

    let root = GBox::new(Orientation::Vertical, 12);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(16);
    root.set_margin_end(16);

    root.append(&build_stats_section());
    root.append(&build_search_section(&window));

    window.set_child(Some(&root));
    window.present();
}

fn build_stats_section() -> gtk4::Widget {
    let group = adw::PreferencesGroup::builder()
        .title("Platform stats")
        .description("Singleton AdminStats row, maintained by DDB streams.")
        .build();

    let stats_label = Label::builder()
        .label("Loading…")
        .wrap(true)
        .xalign(0.0)
        .build();
    stats_label.add_css_class("monospace");
    let row = adw::ActionRow::new();
    row.set_child(Some(&stats_label));
    group.add(&row);

    // Fire synchronously — single GraphQL request, runs at panel-open
    // cadence. Stays in line with the project's blocking-on-UI
    // convention (see `remote_browser`).
    let body = match fetch_admin_stats() {
        Ok(s) => s,
        Err(e) => format!("Failed: {e}"),
    };
    stats_label.set_text(&body);

    group.upcast()
}

fn build_search_section(window: &gtk4::Window) -> gtk4::Widget {
    let group = adw::PreferencesGroup::builder()
        .title("Find user by email")
        .build();

    let input_row = adw::EntryRow::new();
    input_row.set_title("Email or prefix");
    group.add(&input_row);

    let list = ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.add_css_class("boxed-list");

    let search_btn = Button::with_label("Search");
    search_btn.add_css_class("suggested-action");
    {
        let input_row = input_row.clone();
        let list = list.clone();
        let window = window.clone();
        search_btn.connect_clicked(move |_| {
            let email = input_row.text().to_string();
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            match search_users(&email) {
                Ok(items) => {
                    if items.is_empty() {
                        let r = adw::ActionRow::builder().title("No matches").build();
                        list.append(&r);
                    } else {
                        for it in items {
                            list.append(&user_search_row(&window, &it));
                        }
                    }
                }
                Err(e) => {
                    let r = adw::ActionRow::builder()
                        .title("Search failed")
                        .subtitle(format!("{e}"))
                        .build();
                    list.append(&r);
                }
            }
        });
    }

    let action_row = adw::ActionRow::new();
    action_row.add_suffix(&search_btn);
    group.add(&action_row);
    group.add(&list);

    group.upcast()
}

#[derive(Clone)]
struct UserSummary {
    user_id: String,
    email: String,
    enabled: bool,
    status: String,
}

fn user_search_row(window: &gtk4::Window, u: &UserSummary) -> gtk4::Widget {
    let row = adw::ActionRow::builder()
        .title(&u.email)
        .subtitle(format!(
            "{} · {}",
            if u.enabled { "enabled" } else { "disabled" },
            u.status,
        ))
        .build();

    if user_is_superadmin() {
        let grant_pro = Button::with_label("Grant Pro");
        grant_pro.set_valign(gtk4::Align::Center);
        {
            let user_id = u.user_id.clone();
            let window = window.clone();
            grant_pro.connect_clicked(move |_| {
                prompt_then_mutate(
                    &window,
                    "grantTier",
                    &user_id,
                    json!({ "tier": "pro" }),
                );
            });
        }
        row.add_suffix(&grant_pro);

        let reset = Button::with_label("Reset daily");
        reset.set_valign(gtk4::Align::Center);
        {
            let user_id = u.user_id.clone();
            let window = window.clone();
            reset.connect_clicked(move |_| {
                prompt_then_mutate(&window, "resetDailyUsage", &user_id, json!({}));
            });
        }
        row.add_suffix(&reset);
    }

    row.upcast()
}

fn prompt_then_mutate(
    parent: &gtk4::Window,
    action: &'static str,
    target_user_id: &str,
    payload: Value,
) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .heading(format!("Reason for {action}"))
        .body(format!("Recorded to AdminAuditLog with action={action}."))
        .build();
    let reason_entry = Entry::builder().placeholder_text("Reason").build();
    dialog.set_extra_child(Some(&reason_entry));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("apply", "Apply");
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("apply"));

    let target = target_user_id.to_string();
    let reason_entry_clone = reason_entry.clone();
    let parent_clone = parent.clone();
    dialog.connect_response(None, move |dlg, resp| {
        if resp != "apply" {
            dlg.close();
            return;
        }
        let reason = reason_entry_clone.text().to_string();
        if reason.trim().is_empty() {
            return;
        }
        dlg.close();
        match run_admin_mutate(action, &target, &payload, &reason) {
            Ok(_) => {
                let ok = adw::MessageDialog::builder()
                    .transient_for(&parent_clone)
                    .modal(true)
                    .heading("Applied")
                    .body(format!("{action} on {target} succeeded."))
                    .build();
                ok.add_response("ok", "OK");
                ok.present();
            }
            Err(e) => {
                let err = adw::MessageDialog::builder()
                    .transient_for(&parent_clone)
                    .modal(true)
                    .heading("Admin mutate failed")
                    .body(format!("{e}"))
                    .build();
                err.add_response("ok", "OK");
                err.present();
            }
        }
    });
    dialog.present();
}

fn fetch_admin_stats() -> Result<String, RemoteError> {
    let v = graphql(Q_ADMIN_STATS, "AdminStats", json!({}))?;
    let row = v
        .pointer("/getAdminStats")
        .cloned()
        .unwrap_or(Value::Null);
    if row.is_null() {
        return Ok("(no stats yet — DDB stream maintainer has not fired)".into());
    }
    let mrr_cents = row.get("mrrCents").and_then(|x| x.as_i64()).unwrap_or(0);
    Ok(format!(
        "Users: total {} (free {} · pro {} · studio {})\n\
         Status: trialing {} · past_due {} · canceled {}\n\
         Notebooks: {}\n\
         MRR: ${:.2}/mo\n\
         Last updated: {}",
        row.get("totalUsers").and_then(|x| x.as_i64()).unwrap_or(0),
        row.get("freeUsers").and_then(|x| x.as_i64()).unwrap_or(0),
        row.get("proUsers").and_then(|x| x.as_i64()).unwrap_or(0),
        row.get("studioUsers").and_then(|x| x.as_i64()).unwrap_or(0),
        row.get("trialingUsers").and_then(|x| x.as_i64()).unwrap_or(0),
        row.get("pastDueUsers").and_then(|x| x.as_i64()).unwrap_or(0),
        row.get("canceledUsers").and_then(|x| x.as_i64()).unwrap_or(0),
        row.get("totalNotebooks").and_then(|x| x.as_i64()).unwrap_or(0),
        mrr_cents as f64 / 100.0,
        row.get("lastUpdatedIso")
            .and_then(|x| x.as_str())
            .unwrap_or("never"),
    ))
}

fn search_users(email: &str) -> Result<Vec<UserSummary>, RemoteError> {
    let v = graphql(M_SEARCH_USERS, "AdminSearchUsers", json!({ "email": email }))?;
    let items = v
        .pointer("/adminSearchUsers/items")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(items
        .into_iter()
        .map(|x| UserSummary {
            user_id: x
                .get("userId")
                .and_then(|y| y.as_str())
                .unwrap_or("")
                .to_string(),
            email: x
                .get("email")
                .and_then(|y| y.as_str())
                .unwrap_or("")
                .to_string(),
            enabled: x
                .get("enabled")
                .and_then(|y| y.as_bool())
                .unwrap_or(false),
            status: x
                .get("status")
                .and_then(|y| y.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect())
}

fn run_admin_mutate(
    action: &str,
    target_user_id: &str,
    payload: &Value,
    reason: &str,
) -> Result<(), RemoteError> {
    let _ = graphql(
        M_ADMIN_MUTATE,
        "AdminMutate",
        json!({
            "action": action,
            "targetUserId": target_user_id,
            "payload": serde_json::to_string(payload).unwrap_or_else(|_| "{}".into()),
            "reason": reason,
        }),
    )?;
    Ok(())
}

/// Tiny GraphQL helper that constructs a fresh `RemoteTemplateStore`
/// per call — admin panel actions are infrequent enough that the
/// pool-disable cost (single TLS handshake per request) doesn't
/// matter.
fn graphql(query: &str, op: &str, variables: Value) -> Result<Value, RemoteError> {
    let mut store = RemoteTemplateStore::connect()?;
    store.graphql(query, Some(op), variables)
}

/// Deep links into the Stripe Dashboard for an admin who needs to
/// take an action the app doesn't expose (refund, invoice, dispute).
/// Test mode segment is inserted when `STRIPE_DASHBOARD_TEST=1` env
/// is set, matching Stripe's URL convention. Built into the admin
/// panel so admins can right-click a user row → "Open in Stripe".
pub mod stripe_links {
    fn base() -> &'static str {
        if std::env::var("STRIPE_DASHBOARD_TEST").as_deref() == Ok("1") {
            "https://dashboard.stripe.com/test"
        } else {
            "https://dashboard.stripe.com"
        }
    }

    pub fn customer(stripe_customer_id: &str) -> String {
        format!("{}/customers/{}", base(), stripe_customer_id)
    }

    pub fn subscription(stripe_subscription_id: &str) -> String {
        format!("{}/subscriptions/{}", base(), stripe_subscription_id)
    }

    pub fn invoice(stripe_invoice_id: &str) -> String {
        format!("{}/invoices/{}", base(), stripe_invoice_id)
    }
}
