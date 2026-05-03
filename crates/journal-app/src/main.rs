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
use gtk4::prelude::*;
use gtk4::glib;
use gtk4::{Application, ApplicationWindow, CssProvider};
use journal_storage::Db;
use journal_templates::{NotebookTemplateRegistry, TemplateRegistry};
use tracing_subscriber::EnvFilter;

const APP_ID: &str = "dev.s7k.journal";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        if let Err(e) = build_ui(app) {
            tracing::error!("failed to build UI: {:#}", e);
        }
    });
    let exit_code = app.run();
    std::process::exit(exit_code.value());
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_string(
        ".drag-target { background-color: alpha(@accent_bg_color, 0.2); }"
    );
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

fn build_ui(app: &Application) -> Result<()> {
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
    let Some(settings) = gtk4::Settings::default() else { return; };
    let apply = {
        let state = state.clone();
        let canvas = canvas.clone();
        let settings = settings.clone();
        move || {
            let dark = settings.is_gtk_application_prefer_dark_theme();
            state.borrow_mut().dark_mode = dark;
            canvas.queue_draw();
        }
    };
    apply();
    settings.connect_gtk_application_prefer_dark_theme_notify(move |s| {
        let dark = s.is_gtk_application_prefer_dark_theme();
        state.borrow_mut().dark_mode = dark;
        canvas.queue_draw();
    });
}
