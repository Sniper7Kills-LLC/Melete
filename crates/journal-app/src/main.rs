mod canvas_widget;
mod dialogs;
mod input;
mod state;
mod toolbar;
mod views;
mod window;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, Result};
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow};
use journal_storage::Db;
use journal_templates::TemplateRegistry;
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
    let db = Rc::new(RefCell::new(open_db()?));
    let templates = Rc::new(RefCell::new(load_templates()));
    let state = state::new_shared_state(db, templates);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Journal")
        .default_width(1280)
        .default_height(800)
        .build();

    let app_win = window::build(&window, state);
    window.set_child(Some(&app_win.borrow().root));
    window.present();
    Ok(())
}
