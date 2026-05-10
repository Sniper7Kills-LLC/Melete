// GTK-style apps lean on `Rc<RefCell<Option<Rc<dyn Fn(...)>>>>` as a
// standard handle-deferred-closure pattern, and the renderer surface fns
// that thread transform/state/canvas all happen to take 8–11 params.
// Both clippy lints flag stylistic noise rather than bugs in this crate;
// silence at the crate level so real findings stay readable.
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

#[cfg(feature = "remote")]
mod account_settings;
mod asset_resolver;
mod brush_library;
mod canvas_widget;
mod config;
mod dialogs;
mod fetcher;
mod first_run;
mod history;
mod input;
mod notebook_template_creator;
mod onboarding;
mod pdf_export;
mod settings_dialogs;
mod shortcuts;
mod state;
mod template_creator;
mod template_io;
mod template_manager;
#[cfg(feature = "vello")]
mod template_preview;
mod thumbnail;
mod tool_editor;
mod tool_options_popup;
mod tool_settings;
mod toolbar;
#[cfg(feature = "vello")]
mod vello_glarea;
mod views;
mod window;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, Result};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, CssProvider};
use journal_storage::{JournalBackend, MultiFileSqliteBackend};
use journal_templates::{NotebookTemplateRegistry, TemplateRegistry};
use libadwaita as adw;
use tracing_subscriber::EnvFilter;

const APP_ID: &str = "dev.s7k.journal";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        if let Err(e) = build_ui(app) {
            tracing::error!("failed to build UI: {:#}", e);
        }
    });
    let exit_code = app.run();
    std::process::exit(exit_code.value());
}

const APP_CSS_TEMPLATE: &str = r#"
/* ────────────────────────────────────────────────────────────────────
   Journal — visual identity (paper-journal mood: deep indigo + amber)
   ──────────────────────────────────────────────────────────────────── */

/* Editorial fieldbook palette — deep indigo + amber on cream / dim teal. */
@define-color accent_bg_color #3a3d6e;
@define-color accent_color    #5a5e96;
@define-color accent_fg_color #ffffff;
@define-color amber_accent    #d6a83a;
@define-color page_cream      #f4efe2;
@define-color page_teal       #1c2a30;

.drag-target {
    background-color: alpha(@accent_bg_color, 0.2);
    transition: background-color 120ms ease;
}

/* Editorial display font — serif fallback chain, no bundling. Hits
   common Linux serifs in priority order: editorial-grade first, then
   ubiquitous Liberation/DejaVu fallbacks for a clean install. */
.display-font,
.wordmark,
.notebook-card .card-title,
.empty-state-title,
.title-1, .title-2, .title-3, .title-4 {
    font-family: __DISPLAY_FONT__;
}

.wordmark {
    font-weight: 700;
    letter-spacing: 0.10em;
    text-transform: uppercase;
    font-size: 1.05em;
}

.section-header-label {
    font-family: __DISPLAY_FONT__;
    font-weight: 700;
    font-size: 1.10em;
    letter-spacing: 0.01em;
}
.section-nested {
    border-left: 2px solid alpha(@accent_color, 0.35);
    padding-left: 8px;
    margin-left: 4px;
}

.page-row {
    border-radius: 6px;
    padding: 2px 6px;
    min-height: 36px;
    transition: background-color 120ms ease;
}
.page-row:hover   { background-color: alpha(@accent_color, 0.15); }
.page-row:active  { background-color: alpha(@accent_color, 0.30); }
.page-row.current {
    background-color: alpha(@amber_accent, 0.18);
    box-shadow: inset 3px 0 0 @amber_accent;
}

/* Subtle flag/bookmark toggle on the trailing edge of each page row.
   Transparent, no chrome, soft hover that reads as a tap target without
   competing with the row's own hover. Filled-star (.starred-symbolic)
   takes the amber accent; the hollow state stays muted. */
.page-flag-toggle {
    background: transparent;
    border: none;
    box-shadow: none;
    min-width: 22px;
    min-height: 22px;
    padding: 2px;
    -gtk-icon-size: 14px;
    opacity: 0.55;
    transition: opacity 120ms ease, background-color 120ms ease;
}
.page-flag-toggle:hover  { opacity: 1.0; background-color: alpha(@accent_color, 0.10); }
.page-flag-toggle:active { background-color: alpha(@accent_color, 0.18); }
.page-row .page-flag-toggle image { color: @amber_accent; }

/* Bookmarks panel at the top of the notebook sidebar. The expander itself
   keeps GTK defaults; we just give the wrapper a hair of breathing room
   beneath so it doesn't crash into the first section. */
.bookmarks-panel {
    margin-bottom: 6px;
}

/* Horizontal bookmarks strip rendered above the canvas when the sidebar is
   collapsed. Thin separator beneath so it reads as a chrome row, not page
   content. Chips are pill-shaped with amber-tinted hover. */
.bookmarks-top-strip {
    border-bottom: 1px solid alpha(@accent_color, 0.25);
    background-color: alpha(@accent_bg_color, 0.05);
}
.bookmark-chip {
    padding: 2px 6px;
    border-radius: 999px;
}
.bookmark-chip image { color: @amber_accent; }

/* Section row: same hover, looks live ──────────────────────────────── */
.section-row {
    border-radius: 6px;
    padding: 2px 4px;
    transition: background-color 120ms ease;
}
.section-row:hover { background-color: alpha(@accent_color, 0.10); }

/* Inline-rename Entry sits flush in the row, no chrome ─────────────── */
.inline-rename {
    background: transparent;
    border: none;
    box-shadow: none;
    padding: 0 2px;
    min-height: 0;
}
.inline-rename:focus {
    outline: 1px solid alpha(@accent_color, 0.6);
    background: alpha(@card_bg_color, 0.8);
}

.drag-handle {
    min-width: 36px;
    min-height: 44px;
    border-radius: 6px;
    transition: background-color 120ms ease;
}
.drag-handle:hover { background-color: alpha(@accent_color, 0.18); }

.toolbar button:checked {
    background-color: alpha(@amber_accent, 0.40);
    box-shadow: inset 0 -2px 0 @amber_accent;
}

/* ── Editorial fieldbook accents — pull amber into shared chrome ──── */
switch:checked > slider { background-color: @amber_accent; }
switch:checked > image  { color: @amber_accent; }
switch:checked          { background-color: alpha(@amber_accent, 0.55); }

scrollbar slider {
    background-color: alpha(@amber_accent, 0.55);
    border-radius: 999px;
}
scrollbar slider:hover { background-color: alpha(@amber_accent, 0.78); }

*:focus-visible {
    outline: 2px solid alpha(@amber_accent, 0.65);
    outline-offset: 1px;
}

/* ── Compact floating toolbar — single row, ~36px tall ──────────────── */
.floating-toolbar {
    padding: 4px 6px;
    border-radius: 10px;
}
.floating-toolbar .compact-tool {
    min-width: 28px;
    min-height: 28px;
    padding: 2px;
    transition: transform 120ms cubic-bezier(0.2, 0.8, 0.4, 1.2),
                background-color 120ms ease;
}
/* Selected tool slot pops; un-selected slots settle back. Keeps the
   active tool visually emphasized without an extra ring. */
.floating-toolbar .compact-tool:checked { transform: scale(1.08); }
.floating-toolbar .compact-tool image { -gtk-icon-size: 16px; }
.floating-toolbar .compact-scale { min-height: 22px; }
.floating-toolbar .compact-scale trough { min-height: 4px; }
.floating-toolbar separator { margin: 4px 2px; }

.drag-handle-compact {
    border-radius: 6px;
    opacity: 0.55;
    transition: opacity 120ms, background-color 120ms;
}
.drag-handle-compact:hover {
    opacity: 1.0;
    background-color: alpha(@amber_accent, 0.22);
}
.drag-handle-compact:active {
    background-color: alpha(@amber_accent, 0.45);
}

.notebook-card {
    border: 1px solid alpha(@borders, 0.6);
    border-radius: 12px;
    padding: 16px;
    min-width: 200px;
    min-height: 130px;
    transition: all 150ms ease;
}
.notebook-card:hover {
    border-color: @accent_color;
    box-shadow: 0 4px 12px alpha(black, 0.15);
}
/* FlowBox stagger reveal — first ~12 children fade-in with increasing
   delay so the home grid lands instead of popping. The grid itself is
   already double-buffered by GTK; this is purely cosmetic. */
@keyframes card-rise {
    from { opacity: 0; transform: translateY(8px); }
    to   { opacity: 1; transform: translateY(0);    }
}
flowbox > flowboxchild .notebook-card { animation: card-rise 240ms ease-out both; }
flowbox > flowboxchild:nth-child(1)  .notebook-card { animation-delay: 0ms;   }
flowbox > flowboxchild:nth-child(2)  .notebook-card { animation-delay: 30ms;  }
flowbox > flowboxchild:nth-child(3)  .notebook-card { animation-delay: 60ms;  }
flowbox > flowboxchild:nth-child(4)  .notebook-card { animation-delay: 90ms;  }
flowbox > flowboxchild:nth-child(5)  .notebook-card { animation-delay: 120ms; }
flowbox > flowboxchild:nth-child(6)  .notebook-card { animation-delay: 150ms; }
flowbox > flowboxchild:nth-child(7)  .notebook-card { animation-delay: 180ms; }
flowbox > flowboxchild:nth-child(8)  .notebook-card { animation-delay: 210ms; }
flowbox > flowboxchild:nth-child(9)  .notebook-card { animation-delay: 240ms; }
flowbox > flowboxchild:nth-child(10) .notebook-card { animation-delay: 270ms; }
flowbox > flowboxchild:nth-child(11) .notebook-card { animation-delay: 300ms; }
flowbox > flowboxchild:nth-child(12) .notebook-card { animation-delay: 330ms; }
.notebook-card .card-title    { font-weight: 700; font-size: 1.1em; }
.notebook-card .card-subtitle { opacity: 0.6; font-size: 0.85em; }
.notebook-card .card-kind     {
    color: @accent_color;
    font-size: 0.75em;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
}

.kbd {
    font-family: "Source Code Pro", "Cantarell", monospace;
    background-color: alpha(@card_bg_color, 0.8);
    border: 1px solid alpha(@borders, 0.7);
    border-radius: 4px;
    padding: 1px 6px;
    font-size: 0.85em;
    min-width: 28px;
}
.cheatsheet-grid { padding: 12px; }
.cheatsheet-grid label { margin: 4px 8px; }

.var-preview {
    font-family: "Source Code Pro", monospace;
    font-size: 0.9em;
    padding: 6px 8px;
    border-radius: 4px;
    background-color: alpha(@accent_color, 0.12);
    margin-bottom: 6px;
}
.var-group-header {
    font-size: 0.75em;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    opacity: 0.55;
    margin: 6px 4px 2px 4px;
}

.empty-state         { padding: 48px 24px; }
.empty-state-icon    { -gtk-icon-size: 96px; color: alpha(@accent_color, 0.55); margin-bottom: 12px; }
.empty-state-title   { font-size: 1.6em; font-weight: 700; margin-bottom: 6px; }
.empty-state-subtitle{ opacity: 0.6; font-size: 1.0em; margin-bottom: 24px; }

/* ── Template manager: category dividers + preview frames ─────────── */
.template-category-header {
    font-size: 0.75em;
    font-weight: 700;
    letter-spacing: 0.10em;
    text-transform: uppercase;
    opacity: 0.55;
    padding: 12px 8px 4px 8px;
    border-bottom: 1px solid alpha(@borders, 0.5);
    margin-bottom: 4px;
}
.template-preview-frame {
    background: white;
    border-radius: 6px;
    border: 1px solid alpha(@borders, 0.7);
    box-shadow: 0 1px 2px alpha(black, 0.08);
}

/* ── Pen preset chips on the floating toolbar ─────────────────────── */
.pen-preset {
    min-width: 28px;
    min-height: 28px;
    padding: 2px;
    border-radius: 50%;
    border: 1px solid alpha(@borders, 0.6);
    transition: transform 100ms;
}
.pen-preset:hover { transform: scale(1.1); }

/* ── Zoom badge in the canvas overlay corner ──────────────────────── */
.zoom-badge {
    font-family: "Source Code Pro", monospace;
    font-size: 0.85em;
    font-weight: 600;
    padding: 4px 10px;
    border-radius: 999px;
    min-width: 56px;
    opacity: 0.85;
    transition: opacity 120ms ease, background-color 120ms ease;
}
.zoom-badge:hover {
    opacity: 1.0;
    background-color: alpha(@accent_color, 0.20);
}
/* Brief pulse when the zoom value changes — the badge reads as "I just
   updated" without a numeric tween. The class is added by the zoom
   handler and removed after ~140ms. */
@keyframes zoom-pulse {
    0%   { background-color: alpha(@amber_accent, 0.55); transform: scale(1.06); }
    100% { background-color: transparent;                 transform: scale(1.0);  }
}
.zoom-badge.pulse { animation: zoom-pulse 140ms ease-out; }

/* ── Notebook-template editor: drop zones, hints, preview ───────────── */
.nbtc-drop-zone {
    border: 1.5px dashed alpha(@accent_color, 0.45);
    border-radius: 8px;
    padding: 6px 8px;
    transition: background-color 120ms ease, border-color 120ms ease;
}
.nbtc-drop-zone:hover {
    border-color: alpha(@accent_color, 0.70);
}
.nbtc-empty-hint {
    opacity: 0.55;
    font-style: italic;
}
.nbtc-preview {
    background-color: alpha(@accent_color, 0.06);
    border-radius: 8px;
    padding: 6px 8px;
    min-height: 80px;
    max-height: 84px;
}
.nbtc-preview-strip { padding: 2px 4px; }
.nbtc-preview-scroll {
    min-height: 60px;
}
.nbtc-preview-section-label {
    font-size: 0.78em;
    opacity: 0.78;
    margin-right: 2px;
}
.nbtc-preview-chip-frame {
    background: white;
    border-radius: 4px;
    border: 1px solid alpha(@borders, 0.7);
    box-shadow: 0 1px 1px alpha(black, 0.05);
    transition: border-color 120ms ease, box-shadow 120ms ease;
}
.nbtc-preview-chip-clickable {
    cursor: pointer;
}
.nbtc-preview-chip-clickable:hover {
    border-color: @amber_accent;
    box-shadow: 0 2px 4px alpha(@amber_accent, 0.25);
}
.nbtc-preview-chip-empty {
    border: 1px dashed alpha(@accent_color, 0.45);
    border-radius: 4px;
    background: alpha(@accent_color, 0.04);
}
.nbtc-preview-row {
    font-family: "Source Code Pro", monospace;
    font-size: 0.88em;
    opacity: 0.85;
}
.nbtc-preview-card {
    border: 1px solid alpha(@accent_color, 0.45);
    border-radius: 8px;
    padding: 6px 8px;
    background-color: alpha(@accent_color, 0.04);
}
.nbtc-preview-title {
    font-weight: 700;
    font-size: 0.95em;
}
.nbtc-preview-multiplier {
    font-family: "Source Code Pro", monospace;
    font-weight: 700;
    color: @amber_accent;
    font-size: 0.95em;
}
.nbtc-preview-chip {
    background-color: alpha(@accent_color, 0.18);
    border-radius: 4px;
    padding: 1px 6px;
    font-size: 0.85em;
}
.nbtc-preview-day {
    font-family: "Source Code Pro", monospace;
    font-weight: 700;
    min-width: 28px;
    opacity: 0.7;
}
.nbtc-preview-prelabel {
    font-size: 0.65em;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    opacity: 0.55;
}
.nbtc-preview-day-card {
    border-color: alpha(@accent_color, 0.3);
    background-color: alpha(@accent_color, 0.03);
}
.nbtc-preview-chip {
    font-size: 0.78em;
}
.nbtc-palette-chip {
    border: 1px solid alpha(@borders, 0.45);
    border-radius: 6px;
    padding: 4px 8px;
    transition: background-color 120ms ease, border-color 120ms ease;
}
.nbtc-palette-chip:hover {
    border-color: @accent_color;
    background-color: alpha(@accent_color, 0.08);
}
.nbtc-palette-cat {
    font-size: 0.72em;
    font-weight: 700;
    letter-spacing: 0.10em;
    text-transform: uppercase;
    opacity: 0.55;
    margin-top: 8px;
    margin-bottom: 2px;
}

/* ── Dark-mode legibility overrides ────────────────────────────────────
   The amber/indigo palette above is tuned for the cream paper light
   theme; on a dark background `@accent_color` (#5a5e96) and the dim
   `opacity: 0.55` labels collapse into the chrome. These selectors
   only apply when the root window has the `.dark` class (set by
   `bind_system_dark_mode`) so they don't disturb the light theme. */
.dark .notebook-card .card-kind     { color: @amber_accent; }
.dark .notebook-card .card-subtitle { opacity: 0.85; }
.dark .empty-state-icon             { color: alpha(@amber_accent, 0.65); }
.dark .empty-state-subtitle         { opacity: 0.85; }
.dark .template-category-header     { opacity: 0.80; }
.dark .var-group-header             { opacity: 0.80; }
.dark .nbtc-palette-cat             { opacity: 0.80; }
.dark .nbtc-preview-prelabel        { opacity: 0.80; }
.dark .nbtc-empty-hint              { opacity: 0.80; }
"#;

thread_local! {
    /// Holds the application's CssProvider so that runtime settings
    /// changes (e.g. the user picking a different display font) can
    /// reload the same provider via `reload_css` instead of stacking
    /// new providers on top.
    static CSS_PROVIDER: std::cell::RefCell<Option<CssProvider>> =
        const { std::cell::RefCell::new(None) };
}

fn build_css() -> String {
    let cfg = crate::config::load();
    let chain = crate::config::display_font_chain(cfg.display_font.as_deref());
    APP_CSS_TEMPLATE.replace("__DISPLAY_FONT__", chain)
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_string(&build_css());
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    CSS_PROVIDER.with(|c| *c.borrow_mut() = Some(provider));
}

/// Re-evaluate the CSS template against the current `AppConfig` and
/// push the result through the existing CssProvider. Settings dialogs
/// call this after persisting changes that affect chrome (e.g. the
/// display-font selector) so the swap is live without a restart.
pub fn reload_css() {
    CSS_PROVIDER.with(|c| {
        if let Some(p) = c.borrow().as_ref() {
            p.load_from_string(&build_css());
        }
    });
}

fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
        .context("could not resolve data directory")?;
    Ok(base.join("journal"))
}

fn open_backend() -> Result<MultiFileSqliteBackend> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("create data dir {:?}", dir))?;
    // File-per-notebook layout: index.db at the root, journals/{id}.journal
    // per notebook. The first call after upgrade migrates any pre-existing
    // single-file `journal.db` automatically.
    MultiFileSqliteBackend::open(&dir)
        .with_context(|| format!("open multi-file backend at {:?}", dir))
}

fn build_ui(app: &adw::Application) -> Result<()> {
    load_css();

    gtk4::Window::set_default_icon_name(APP_ID);
    if let Some(display) = gtk4::gdk::Display::default() {
        let icon_theme = gtk4::IconTheme::for_display(&display);
        if let Ok(exe) = std::env::current_exe() {
            if let Some(repo_root) = exe.ancestors().find(|p| p.join("resources/icons").exists()) {
                icon_theme.add_search_path(repo_root.join("resources/icons"));
            }
        }
    }

    let backend: Rc<RefCell<dyn JournalBackend>> = Rc::new(RefCell::new(open_backend()?));
    let templates = Rc::new(RefCell::new(TemplateRegistry::with_builtins()));
    let notebook_templates = Rc::new(RefCell::new(NotebookTemplateRegistry::with_builtins()));
    template_io::hydrate_registries_from_backend(&backend, &templates, &notebook_templates);
    let state = state::new_shared_state(backend, templates, notebook_templates);
    state::reload_placeholder(&state);
    state::load_tool_settings_from_config(&state);
    state::load_tool_brush_assignments(&state);

    let startup_cfg = config::load();
    let default_w = startup_cfg.window_width.unwrap_or(1280);
    let default_h = startup_cfg.window_height.unwrap_or(800);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Journal")
        .default_width(default_w)
        .default_height(default_h)
        .build();

    {
        let window_for_close = window.clone();
        window.connect_close_request(move |_| {
            let w = window_for_close.width();
            let h = window_for_close.height();
            let mut cfg = config::load();
            cfg.window_width = Some(w);
            cfg.window_height = Some(h);
            if let Err(e) = config::save(&cfg) {
                tracing::warn!("failed to save window size: {}", e);
            }
            glib::Propagation::Proceed
        });
    }

    let app_win = window::build(&window, state.clone());
    window.set_child(Some(&app_win.borrow().root));

    let canvas = app_win.borrow().canvas.clone();
    shortcuts::attach_keyboard_shortcuts(&window, state.clone(), canvas.clone());
    bind_system_dark_mode(state.clone(), canvas, &window);

    window.present();

    // First-run sign-in / skip welcome window. Shown modally over the
    // freshly-presented main window the very first time Journal launches
    // (or any boot where `AppConfig::first_run_completed` is still
    // false). Either button persists the flag and runs the post-welcome
    // tour + what's-new chain; if the user has already dispatched the
    // welcome window the callback fires synchronously.
    {
        let window_for_callback = window.clone();
        first_run::show_if_needed(&window, move || {
            // Discoverability nudges (audit §11). Tour fires on first launch
            // until dismissed; what's-new fires once per crate version after the
            // tour has been seen at least once.
            onboarding::show_first_launch_tour(&window_for_callback);
            onboarding::show_whats_new_if_upgraded(&window_for_callback);
        });
    }

    Ok(())
}

fn bind_system_dark_mode(
    state: state::SharedState,
    canvas: gtk4::DrawingArea,
    root: &ApplicationWindow,
) {
    let style_manager = adw::StyleManager::default();
    // Follow the system color scheme; do not force light or dark.
    style_manager.set_color_scheme(adw::ColorScheme::Default);

    // Audit §10: every render path now reads `is_dark_mode()` directly
    // from `adw::StyleManager`, so this hook only has to invalidate
    // caches, swap a `dark`/`light` class on the root window for CSS
    // chrome, and request a repaint when the scheme changes — it no
    // longer mirrors into `state.dark_mode`.
    let _ = state;
    let root_wk = root.clone();
    let apply = {
        let canvas = canvas.clone();
        let root = root_wk.clone();
        let sm = style_manager.clone();
        move || {
            let dark = sm.is_dark();
            if dark {
                root.add_css_class("dark");
                root.remove_css_class("light");
            } else {
                root.add_css_class("light");
                root.remove_css_class("dark");
            }
            canvas.queue_draw();
        }
    };
    apply();
    style_manager.connect_dark_notify(move |sm| {
        let dark = sm.is_dark();
        if dark {
            root_wk.add_css_class("dark");
            root_wk.remove_css_class("light");
        } else {
            root_wk.add_css_class("light");
            root_wk.remove_css_class("dark");
        }
        canvas.queue_draw();
    });
}

/// Single source of truth for dark mode. Every renderer in the app
/// crate reads through this helper instead of plumbing a
/// `dark_mode: bool` through the call graph; the `journal-canvas` and
/// `journal-widgets` crates can't depend on libadwaita so they keep
/// the bool param at their leaf signatures, and callers in the app
/// crate fill it from here.
pub fn is_dark_mode() -> bool {
    adw::StyleManager::default().is_dark()
}
