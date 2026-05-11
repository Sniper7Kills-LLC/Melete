//! Subscription group for the App Settings preferences window.
//!
//! Three states, mirroring the account group:
//!   1. Remote not configured → static row pointing at `npx ampx sandbox`.
//!   2. Signed out → "Sign in to manage your subscription" hint.
//!   3. Signed in → tier badge + a "Manage subscription" button that
//!      mints a Stripe Customer Portal URL via the
//!      `createPortalSession` mutation and opens it in the user's
//!      default browser. Never-subscribed users get an "Upgrade"
//!      button instead which routes to Stripe Checkout.
//!
//! Network calls happen synchronously on the UI thread, matching the
//! existing `remote_browser` pattern — Stripe link generation is one
//! short HTTPS roundtrip and runs at user-initiated cadence.
//!
//! Gated on the `remote` feature.

use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Button};
use libadwaita as adw;
use libadwaita::prelude::*;

use journal_storage::entitlement::Entitlement;
use journal_storage::remote_template_store::store::{RemoteError, RemoteTemplateStore};

pub fn populate_subscription_group(
    parent: &ApplicationWindow,
    group: &adw::PreferencesGroup,
) {
    while let Some(row) = first_row(group) {
        group.remove(&row);
    }

    match RemoteTemplateStore::connect() {
        Err(RemoteError::Config(_)) => {
            add_static_row(group, "Remote backend not configured", None);
        }
        Err(e) => {
            add_static_row(group, "Remote backend error", Some(&format!("{e}")));
        }
        Ok(mut store) => {
            if !store.is_signed_in() {
                add_static_row(
                    group,
                    "Sign in to manage your subscription",
                    Some("Subscription settings are tied to your account."),
                );
                return;
            }
            match store.fetch_my_entitlement() {
                Ok(ent) => add_signed_in_rows(parent, group, &ent),
                Err(e) => {
                    tracing::warn!("fetch_my_entitlement: {e}");
                    add_static_row(
                        group,
                        "Could not load subscription",
                        Some(&format!("{e}")),
                    );
                }
            }
        }
    }
}

fn first_row(group: &adw::PreferencesGroup) -> Option<gtk4::Widget> {
    let mut child = group.first_child();
    while let Some(c) = child {
        if let Some(lb) = c.downcast_ref::<gtk4::ListBox>() {
            return lb.first_child().map(|x| x.upcast::<gtk4::Widget>());
        }
        child = c.next_sibling();
    }
    None
}

fn add_static_row(group: &adw::PreferencesGroup, title: &str, subtitle: Option<&str>) {
    let row = adw::ActionRow::builder().title(title).build();
    if let Some(s) = subtitle {
        row.set_subtitle(s);
    }
    group.add(&row);
}

fn add_signed_in_rows(
    parent: &ApplicationWindow,
    group: &adw::PreferencesGroup,
    ent: &Entitlement,
) {
    // Tier badge row. Title = tier name + status; subtitle = caps
    // summary so the user sees the headline numbers at a glance
    // without opening another page.
    let tier_label = tier_display(&ent.tier);
    let status_label = status_display(&ent.status);
    let title = format!("{tier_label} ({status_label})");
    let subtitle = format!(
        "{} notebooks · {} strokes/day · {} live sync",
        ent.notebook_cap,
        ent.daily_write_cap,
        if ent.live_sync_enabled { "✓" } else { "✗" },
    );
    let summary = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    group.add(&summary);

    let action_row = adw::ActionRow::builder().title("Billing").build();
    let button = if ent.tier == "free" {
        let b = Button::with_label("Upgrade…");
        b.add_css_class("suggested-action");
        b.set_valign(gtk4::Align::Center);
        // Native app delegates plan picking + add-on selection to the
        // web /billing page — the desktop only renders the
        // tier-summary card. Opens the user's default browser at the
        // configured billing URL.
        b.connect_clicked(move |_| open_url(&billing_url("/billing")));
        b
    } else {
        let b = Button::with_label("Manage subscription");
        b.set_valign(gtk4::Align::Center);
        // Web /billing also handles the "Manage subscription" flow
        // (mints the Stripe Customer Portal URL server-side), keeping
        // the native UX consistent across tiers.
        b.connect_clicked(move |_| open_url(&billing_url("/billing")));
        b
    };
    action_row.add_suffix(&button);
    group.add(&action_row);
}

fn tier_display(tier: &str) -> String {
    match tier {
        "free" => "Free".into(),
        "pro" => "Pro".into(),
        "studio" => "Studio".into(),
        other => other.to_string(),
    }
}

fn status_display(status: &str) -> String {
    match status {
        "active" => "active".into(),
        "trialing" => "trial".into(),
        "past_due" => "past due — payment failed".into(),
        "canceled" => "canceled".into(),
        "paused" => "paused".into(),
        other => other.to_string(),
    }
}

/// Resolve the base URL for the web billing portal. `JOURNAL_WEB_URL`
/// override exists for staging / prod deploys; otherwise defaults to
/// the same `http://localhost:3000` value baked into the sandbox
/// Lambda env so dev flows stay consistent.
fn billing_url(path: &str) -> String {
    let base = std::env::var("JOURNAL_WEB_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn open_url(url: &str) {
    if let Err(e) = gtk4::gio::AppInfo::launch_default_for_uri(
        url,
        None::<&gtk4::gio::AppLaunchContext>,
    ) {
        tracing::warn!("launch_default_for_uri({url}): {e}");
    }
}

