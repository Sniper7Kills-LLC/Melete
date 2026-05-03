use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, DrawingArea, HeaderBar, Label, Orientation, Overlay,
    Stack, StackTransitionType,
};
use journal_core::NotebookId;
use journal_storage::notebook_store;

use crate::canvas_widget;
use crate::input;
use crate::state::SharedState;
use crate::toolbar;
use crate::views::{home, notebook as notebook_view};

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
    title_label: Label,
    state: SharedState,
    parent: ApplicationWindow,
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
        title_label,
        state: state.clone(),
        parent: parent.clone(),
    }));

    {
        let win = win.clone();
        back_btn.connect_clicked(move |_| show_home(&win));
    }

    build_home_into(&win);
    show_home(&win);

    win
}

pub fn show_home(win: &SharedWindow) {
    let w = win.borrow();
    w.stack.set_visible_child_name(HOME_NAME);
    w.back_btn.set_visible(false);
    w.title_label.set_text("Journal");
}

pub fn show_notebook(win: &SharedWindow, notebook_id: NotebookId) {
    // Detach overlay from any previous parent (the previous notebook view).
    let overlay = win.borrow().canvas_overlay.clone();
    if overlay.parent().is_some() {
        overlay.unparent();
    }

    // Clear the notebook container.
    let container = win.borrow().notebook_container.clone();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    // Build a fresh notebook view containing the persistent overlay.
    let parent = win.borrow().parent.clone();
    let state = win.borrow().state.clone();
    let canvas = win.borrow().canvas.clone();
    let view = notebook_view::build_notebook_view(
        &parent,
        state.clone(),
        notebook_id,
        canvas,
        overlay,
    );
    container.append(&view.root);

    // Reset canvas: clear strokes + page until user picks one.
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

    // Update header.
    let title = match notebook_store::get_notebook(state.borrow().db.borrow().conn(), notebook_id) {
        Ok(nb) => nb.name,
        Err(_) => "Notebook".to_string(),
    };
    win.borrow().title_label.set_text(&title);
    win.borrow().back_btn.set_visible(true);
    win.borrow().stack.set_visible_child_name(NOTEBOOK_NAME);
}

fn build_home_into(win: &SharedWindow) {
    let container = win.borrow().home_container.clone();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let parent = win.borrow().parent.clone();
    let db = win.borrow().state.borrow().db.clone();
    let win_for_open = win.clone();
    let on_open: Rc<dyn Fn(NotebookId)> = Rc::new(move |id| {
        show_notebook(&win_for_open, id);
    });

    let home = home::build_home(&parent, db, on_open);
    container.append(&home);
}
