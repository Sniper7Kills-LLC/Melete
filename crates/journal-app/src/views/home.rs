use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, Label, Orientation, ScrolledWindow,
};
use journal_core::{Notebook, NotebookId, NotebookKind};
use journal_storage::{notebook_store, Db};
use uuid::Uuid;

use crate::dialogs;
use crate::state::SharedState;
use crate::template_manager;

/// Build the home screen widget. `on_open` is called when a notebook is selected.
/// Returns the root widget — caller is responsible for placing it in the window.
pub fn build_home(
    parent: &ApplicationWindow,
    state: SharedState,
    db: Rc<RefCell<Db>>,
    on_open: Rc<dyn Fn(NotebookId)>,
) -> GtkBox {
    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(0)
        .build();

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(8)
        .margin_start(16)
        .margin_end(16)
        .build();
    let title = Label::builder()
        .label("Notebooks")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    title.add_css_class("title-1");
    header.append(&title);

    let settings_btn = Button::from_icon_name("emblem-system-symbolic");
    settings_btn.set_tooltip_text(Some("App settings"));
    header.append(&settings_btn);

    let templates_btn = Button::with_label("Templates");
    header.append(&templates_btn);

    let nb_template_btn = Button::with_label("New notebook template");
    header.append(&nb_template_btn);

    let new_planner_btn = Button::with_label("New planner");
    header.append(&new_planner_btn);

    let new_btn = Button::with_label("New notebook");
    new_btn.add_css_class("suggested-action");
    header.append(&new_btn);
    root.append(&header);

    {
        let parent = parent.clone();
        let state = state.clone();
        templates_btn.connect_clicked(move |_| {
            template_manager::open(&parent, state.clone());
        });
    }

    {
        let parent = parent.clone();
        let state = state.clone();
        settings_btn.connect_clicked(move |_| {
            crate::settings_dialogs::open_app_settings(
                &parent,
                state.clone(),
                Box::new(|| {}),
            );
        });
    }

    {
        let parent = parent.clone();
        let state = state.clone();
        nb_template_btn.connect_clicked(move |_| {
            dialogs::prompt_new_notebook_template(
                &parent,
                state.clone(),
                Box::new(|_id| {}),
            );
        });
    }

    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let list_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();
    scroller.set_child(Some(&list_box));
    root.append(&scroller);

    let list_box_rc = Rc::new(list_box);
    refresh_list(&list_box_rc, db.clone(), on_open.clone());

    {
        let parent = parent.clone();
        let db = db.clone();
        let list_box = list_box_rc.clone();
        let on_open = on_open.clone();
        new_btn.connect_clicked(move |_| {
            let db_inner = db.clone();
            let list_box = list_box.clone();
            let on_open = on_open.clone();
            dialogs::prompt_new_notebook(
                &parent,
                Box::new(move |name| {
                    let nb = Notebook {
                        id: NotebookId(Uuid::new_v4()),
                        name,
                        kind: NotebookKind::Standard,
                        assigned_templates: Vec::new(),
                    };
                    if let Err(e) = notebook_store::insert_notebook(db_inner.borrow().conn(), &nb) {
                        tracing::error!("failed to insert notebook: {}", e);
                        return;
                    }
                    refresh_list(&list_box, db_inner.clone(), on_open.clone());
                }),
            );
        });
    }

    {
        let parent = parent.clone();
        let state = state.clone();
        let db = db.clone();
        let list_box = list_box_rc.clone();
        let on_open = on_open.clone();
        new_planner_btn.connect_clicked(move |_| {
            let db_inner = db.clone();
            let list_box = list_box.clone();
            let on_open = on_open.clone();
            dialogs::prompt_new_planner(
                &parent,
                state.clone(),
                Box::new(move |choice| {
                    let nb = Notebook {
                        id: NotebookId(Uuid::new_v4()),
                        name: choice.name,
                        kind: NotebookKind::Planner {
                            template_id: choice.template_id,
                            creation_date: choice.creation_date,
                        },
                        assigned_templates: Vec::new(),
                    };
                    if let Err(e) = notebook_store::insert_notebook(db_inner.borrow().conn(), &nb) {
                        tracing::error!("failed to insert planner notebook: {}", e);
                        return;
                    }
                    refresh_list(&list_box, db_inner.clone(), on_open.clone());
                }),
            );
        });
    }

    root
}

fn refresh_list(
    list_box: &Rc<GtkBox>,
    db: Rc<RefCell<Db>>,
    on_open: Rc<dyn Fn(NotebookId)>,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let notebooks = match notebook_store::list_notebooks(db.borrow().conn()) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("failed to list notebooks: {}", e);
            return;
        }
    };

    if notebooks.is_empty() {
        let empty = Label::new(Some("No notebooks yet — create one to get started."));
        empty.add_css_class("dim-label");
        empty.set_halign(gtk4::Align::Start);
        list_box.append(&empty);
        return;
    }

    for nb in notebooks {
        let btn = Button::builder()
            .label(&nb.name)
            .hexpand(true)
            .halign(gtk4::Align::Fill)
            .build();
        let id = nb.id;
        let on_open = on_open.clone();
        btn.connect_clicked(move |_| on_open(id));
        list_box.append(&btn);
    }
}
