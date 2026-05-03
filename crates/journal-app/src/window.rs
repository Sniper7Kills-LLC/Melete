use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, DrawingArea, HeaderBar, Label,
    MenuButton, Orientation, Overlay, Popover, Stack, StackTransitionType,
};
use journal_core::NotebookId;
use journal_storage::notebook_store;

use crate::canvas_widget;
use crate::input;
use crate::state::SharedState;
use crate::toolbar;
use crate::views::{home, notebook as notebook_view, planner_nav};

const HOME_NAME: &str = "home";
const NOTEBOOK_NAME: &str = "notebook";

pub struct AppWindow {
    pub root: GtkBox,
    pub canvas: DrawingArea,
    stack: Stack,
    home_container: GtkBox,
    notebook_container: GtkBox,
    canvas_overlay: Overlay,
    back_btn: Button,
    notebook_settings_btn: Button,
    title_label: Label,
    state: SharedState,
    parent: ApplicationWindow,
    current_notebook: Rc<RefCell<Option<NotebookId>>>,
}

pub type SharedWindow = Rc<RefCell<AppWindow>>;

pub fn build(parent: &ApplicationWindow, state: SharedState) -> SharedWindow {
    let header = HeaderBar::new();
    let title_label = Label::new(Some("Journal"));
    header.set_title_widget(Some(&title_label));

    let back_btn = Button::from_icon_name("go-previous-symbolic");
    back_btn.set_tooltip_text(Some("Back to notebooks"));
    back_btn.set_visible(false);
    header.pack_start(&back_btn);

    let notebook_settings_btn = Button::from_icon_name("emblem-system-symbolic");
    notebook_settings_btn.set_tooltip_text(Some("Notebook settings"));
    notebook_settings_btn.set_visible(false);
    header.pack_end(&notebook_settings_btn);

    let menu_btn = build_menu_button(parent, state.clone());
    header.pack_end(&menu_btn);

    let canvas = canvas_widget::build_canvas(state.clone());
    input::attach_stylus(&canvas, state.clone());
    input::attach_mouse(&canvas, state.clone());
    input::attach_pan_zoom(&canvas, state.clone());
    let bar = toolbar::build_toolbar(state.clone());
    let canvas_overlay = Overlay::new();
    canvas_overlay.set_child(Some(&canvas));
    canvas_overlay.add_overlay(&bar);

    let home_container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();
    let notebook_container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let stack = Stack::new();
    stack.set_transition_type(StackTransitionType::SlideLeftRight);
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.add_named(&home_container, Some(HOME_NAME));
    stack.add_named(&notebook_container, Some(NOTEBOOK_NAME));

    parent.set_titlebar(Some(&header));

    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .build();
    root.append(&stack);

    let win = Rc::new(RefCell::new(AppWindow {
        root,
        canvas: canvas.clone(),
        stack,
        home_container,
        notebook_container,
        canvas_overlay,
        back_btn: back_btn.clone(),
        notebook_settings_btn: notebook_settings_btn.clone(),
        title_label,
        state: state.clone(),
        parent: parent.clone(),
        current_notebook: Rc::new(RefCell::new(None)),
    }));

    {
        let win = win.clone();
        back_btn.connect_clicked(move |_| show_home(&win));
    }

    {
        let win = win.clone();
        notebook_settings_btn.connect_clicked(move |_| {
            let (parent, state, nb_id) = {
                let w = win.borrow();
                let nb = *w.current_notebook.borrow();
                (w.parent.clone(), w.state.clone(), nb)
            };
            if let Some(nid) = nb_id {
                crate::settings_dialogs::open_notebook_settings(
                    &parent,
                    state,
                    nid,
                    Box::new(|| {}),
                );
            }
        });
    }

    build_home_into(&win);
    show_home(&win);

    win
}

fn build_menu_button(parent: &ApplicationWindow, state: SharedState) -> MenuButton {
    let popover = Popover::new();
    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let export_btn = Button::with_label("Export page as PDF…");
    {
        let state = state.clone();
        let parent = parent.clone();
        let popover_clone = popover.clone();
        export_btn.connect_clicked(move |_| {
            popover_clone.popdown();
            do_pdf_export(&parent, state.clone());
        });
    }
    vbox.append(&export_btn);

    popover.set_child(Some(&vbox));

    let menu_btn = MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .popover(&popover)
        .tooltip_text("Menu")
        .build();
    menu_btn
}

fn do_pdf_export(parent: &ApplicationWindow, state: SharedState) {
    let dialog = gtk4::FileDialog::builder()
        .title("Export page as PDF")
        .modal(true)
        .initial_name("page.pdf")
        .build();

    let filter = gtk4::FileFilter::new();
    filter.set_name(Some("PDF files"));
    filter.add_pattern("*.pdf");
    filter.add_mime_type("application/pdf");
    let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));

    let parent_clone = parent.clone();
    dialog.save(Some(parent), gtk4::gio::Cancellable::NONE, move |result| {
        let file = match result {
            Ok(f) => f,
            Err(_) => return,
        };
        let path = match file.path() {
            Some(p) => p,
            None => {
                tracing::warn!("PDF export: no path from file dialog");
                return;
            }
        };
        let path = if path.extension().map(|e| e != "pdf").unwrap_or(true) {
            path.with_extension("pdf")
        } else {
            path
        };
        if let Err(e) = crate::pdf_export::export_page_to_pdf(&state, &path) {
            tracing::error!("PDF export failed: {:#}", e);
            let dialog = gtk4::AlertDialog::builder()
                .message("Export failed")
                .detail(format!("{:#}", e).as_str())
                .build();
            dialog.show(Some(&parent_clone));
        } else {
            tracing::info!("PDF exported to {:?}", path);
        }
    });
}

pub fn show_home(win: &SharedWindow) {
    let w = win.borrow();
    w.stack.set_visible_child_name(HOME_NAME);
    w.back_btn.set_visible(false);
    w.notebook_settings_btn.set_visible(false);
    *w.current_notebook.borrow_mut() = None;
    w.title_label.set_text("Journal");
}

pub fn show_notebook(win: &SharedWindow, notebook_id: NotebookId) {
    {
        let mut cfg = crate::config::load();
        cfg.recent_notebook_ids.retain(|id| *id != notebook_id.0);
        cfg.recent_notebook_ids.insert(0, notebook_id.0);
        cfg.recent_notebook_ids.truncate(5);
        if let Err(e) = crate::config::save(&cfg) {
            tracing::warn!("failed to save recent notebooks: {}", e);
        }
    }

    let overlay = win.borrow().canvas_overlay.clone();
    if overlay.parent().is_some() {
        overlay.unparent();
    }

    let container = win.borrow().notebook_container.clone();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let parent = win.borrow().parent.clone();
    let state = win.borrow().state.clone();
    let canvas = win.borrow().canvas.clone();

    {
        let mut s = state.borrow_mut();
        s.strokes.clear();
        s.current_stroke = None;
        s.current_page_id = None;
        s.current_template = None;
        s.background = crate::state::default_background();
        s.page_rect = crate::state::default_page_rect();
    }
    win.borrow().canvas.queue_draw();

    let planner_template = planner_nav::resolve_planner_template(&state, notebook_id);
    if let Some(ref template) = planner_template {
        let today = chrono::Local::now().date_naive();
        let _ = planner_nav::goto_date(&state, &canvas, notebook_id, template, today);
    }

    let view = notebook_view::build_notebook_view(
        &parent,
        state.clone(),
        notebook_id,
        canvas,
        overlay,
    );
    container.append(&view.root);

    let title = match notebook_store::get_notebook(state.borrow().db.borrow().conn(), notebook_id) {
        Ok(nb) => nb.name,
        Err(_) => "Notebook".to_string(),
    };
    win.borrow().title_label.set_text(&title);
    win.borrow().back_btn.set_visible(true);
    win.borrow().notebook_settings_btn.set_visible(true);
    *win.borrow().current_notebook.borrow_mut() = Some(notebook_id);
    win.borrow().stack.set_visible_child_name(NOTEBOOK_NAME);
}

fn build_home_into(win: &SharedWindow) {
    let container = win.borrow().home_container.clone();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let parent = win.borrow().parent.clone();
    let state = win.borrow().state.clone();
    let db = state.borrow().db.clone();
    let win_for_open = win.clone();
    let on_open: Rc<dyn Fn(NotebookId)> = Rc::new(move |id| {
        show_notebook(&win_for_open, id);
    });

    let home = home::build_home(&parent, state, db, on_open);
    container.append(&home);
}
