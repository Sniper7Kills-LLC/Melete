mod canvas_widget;
mod input;
mod state;
mod toolbar;

use anyhow::Result;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Overlay};
use tracing_subscriber::EnvFilter;

const APP_ID: &str = "dev.s7k.journal";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    let exit_code = app.run();
    std::process::exit(exit_code.value());
}

fn build_ui(app: &Application) {
    let state = state::new_shared_state();
    let canvas = canvas_widget::build_canvas(state.clone());

    input::attach_stylus(&canvas, state.clone());
    input::attach_mouse(&canvas, state.clone());
    input::attach_pan_zoom(&canvas, state.clone());

    let toolbar = toolbar::build_toolbar(state.clone());

    let overlay = Overlay::new();
    overlay.set_child(Some(&canvas));
    overlay.add_overlay(&toolbar);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Journal")
        .default_width(1280)
        .default_height(800)
        .build();
    window.set_child(Some(&overlay));
    window.present();
}
