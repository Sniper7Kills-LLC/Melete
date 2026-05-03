mod canvas_widget;
mod config;
mod dialogs;
mod history;
mod input;
mod pdf_export;
mod settings_dialogs;
mod shortcuts;
mod state;
mod template_creator;
mod template_manager;
mod thumbnail;
mod toolbar;
mod views;
mod window;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, Result};
use gtk4::glib;
use gtk4::{ApplicationWindow, CssProvider};
use gtk4::prelude::*;
use libadwaita as adw;
use journal_storage::Db;
use journal_templates::{NotebookTemplateRegistry, TemplateRegistry};
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

const APP_CSS: &str = r#"
/* ────────────────────────────────────────────────────────────────────
   Journal — visual identity (paper-journal mood: deep indigo + amber)
   ──────────────────────────────────────────────────────────────────── */

@define-color accent_bg_color #3a3d6e;
@define-color accent_color    #5a5e96;
@define-color accent_fg_color #ffffff;
@define-color amber_accent    #d6a83a;

.drag-target {
    background-color: alpha(@accent_bg_color, 0.2);
    transition: background-color 120ms ease;
}

.wordmark {
    font-family: "Cantarell", "Source Sans 3", sans-serif;
    font-weight: 700;
    letter-spacing: 0.10em;
    text-transform: uppercase;
    font-size: 1.05em;
}

.section-header-label {
    font-weight: 700;
    font-size: 1.05em;
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

/* ── Compact floating toolbar — single row, ~36px tall ──────────────── */
.floating-toolbar {
    padding: 4px 6px;
    border-radius: 10px;
}
.floating-toolbar .compact-tool {
    min-width: 28px;
    min-height: 28px;
    padding: 2px;
}
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
    background-color: alpha(@accent_color, 0.18);
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
"#;

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_string(APP_CSS);
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
        .context("could not resolve data directory")?;
    Ok(base.join("journal"))
}

fn open_db() -> Result<Db> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create data dir {:?}", dir))?;
    let path = dir.join("journal.db");
    Db::open(&path).with_context(|| format!("open db {:?}", path))
}

fn load_templates() -> TemplateRegistry {
    let mut reg = TemplateRegistry::with_builtins();
    if let Ok(dir) = data_dir() {
        let tdir = dir.join("templates");
        if tdir.exists() {
            match reg.load_dir(&tdir) {
                Ok(n) => tracing::info!("loaded {} user templates from {:?}", n, tdir),
                Err(e) => tracing::warn!("failed to load templates from {:?}: {}", tdir, e),
            }
        }
    }
    reg
}

fn build_ui(app: &adw::Application) -> Result<()> {
    load_css();

    gtk4::Window::set_default_icon_name(APP_ID);
    if let Some(display) = gtk4::gdk::Display::default() {
        let icon_theme = gtk4::IconTheme::for_display(&display);
        if let Ok(exe) = std::env::current_exe() {
            if let Some(repo_root) = exe
                .ancestors()
                .find(|p| p.join("resources/icons").exists())
            {
                icon_theme.add_search_path(repo_root.join("resources/icons"));
            }
        }
    }

    let db = Rc::new(RefCell::new(open_db()?));
    let templates = Rc::new(RefCell::new(load_templates()));
    let mut nb_reg = NotebookTemplateRegistry::with_builtins();
    if let Ok(dir) = data_dir() {
        let nbtdir = dir.join("notebook_templates");
        match nb_reg.load_dir(&nbtdir) {
            Ok(n) if n > 0 => tracing::info!("loaded {} notebook templates from {:?}", n, nbtdir),
            Ok(_) => {}
            Err(e) => tracing::warn!("load notebook templates failed: {}", e),
        }
    }
    let notebook_templates = Rc::new(RefCell::new(nb_reg));
    let state = state::new_shared_state(db, templates, notebook_templates);
    state::reload_placeholder(&state);

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
    bind_system_dark_mode(state.clone(), canvas);

    window.present();
    Ok(())
}

fn bind_system_dark_mode(state: state::SharedState, canvas: gtk4::DrawingArea) {
    let style_manager = adw::StyleManager::default();
    // Follow the system color scheme; do not force light or dark.
    style_manager.set_color_scheme(adw::ColorScheme::Default);

    let apply = {
        let state = state.clone();
        let canvas = canvas.clone();
        let style_manager = style_manager.clone();
        move || {
            let dark = style_manager.is_dark();
            state.borrow_mut().dark_mode = dark;
            canvas.queue_draw();
        }
    };
    apply();
    style_manager.connect_dark_notify(move |sm| {
        let dark = sm.is_dark();
        state.borrow_mut().dark_mode = dark;
        canvas.queue_draw();
    });
}
