//! User-visible toast helper (#28). A single `adw::ToastOverlay`
//! lives at the top of the main window's widget tree; call sites
//! anywhere in the app fire `notify::toast(...)` to surface a
//! transient banner without having to thread a `&ApplicationWindow`
//! through every function signature.
//!
//! Reasonable rule of thumb: any code path that swallows / logs an
//! error the user might wonder about ("did my action take effect?")
//! gets a toast. Internal sanity checks that should never fire stay
//! as `tracing::warn!` only.

use std::cell::RefCell;

use libadwaita as adw;

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // Info is reserved for future call sites (no current users).
pub enum Severity {
    Info,
    Warn,
    Error,
}

impl Severity {
    /// Seconds before the toast auto-dismisses. Error toasts stay
    /// longer so the user has time to read them.
    fn timeout_secs(self) -> u32 {
        match self {
            Severity::Info => 4,
            Severity::Warn => 6,
            Severity::Error => 8,
        }
    }
}

thread_local! {
    /// Single overlay handle for the active main window. Set during
    /// window build; calls before registration silently log and skip.
    /// Stored thread-local because GTK is single-threaded — every
    /// caller of `toast()` runs on the GTK main loop.
    static TOAST_OVERLAY: RefCell<Option<adw::ToastOverlay>> = const { RefCell::new(None) };
}

/// Install the application's `ToastOverlay`. Called once during
/// window build. Subsequent calls overwrite (only useful in tests).
pub fn register_overlay(overlay: adw::ToastOverlay) {
    TOAST_OVERLAY.with(|cell| *cell.borrow_mut() = Some(overlay));
}

/// Surface a short transient banner in the current main window.
/// Severity controls timeout + a CSS class hook for theming. If no
/// overlay is registered (e.g. unit tests, or a code path that
/// somehow fires before the window is built), the message falls
/// back to a `tracing::warn!` so it isn't lost.
pub fn toast(message: impl Into<String>, severity: Severity) {
    let msg = message.into();
    TOAST_OVERLAY.with(|cell| {
        let Some(overlay) = cell.borrow().clone() else {
            tracing::warn!("notify::toast (no overlay registered): {msg}");
            return;
        };
        let t = adw::Toast::new(&msg);
        t.set_timeout(severity.timeout_secs());
        overlay.add_toast(t);
    });
}
