use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, CheckButton, Label, Orientation, ScrolledWindow,
    Window,
};
use journal_core::{NotebookId, PageTemplate, SectionId, TemplateId};
use journal_storage::{notebook_store, section_store};

use crate::state::SharedState;

fn modal(parent: &ApplicationWindow, title: &str) -> Window {
    Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(420)
        .default_height(480)
        .build()
}

fn sorted_templates(state: &SharedState) -> Vec<PageTemplate> {
    let s = state.borrow();
    let reg = s.templates.borrow();
    let mut v: Vec<PageTemplate> = reg.list().iter().map(|t| (*t).clone()).collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v
}

pub fn open_notebook_settings(
    parent: &ApplicationWindow,
    state: SharedState,
    notebook_id: NotebookId,
    on_saved: Box<dyn Fn()>,
) {
    let win = modal(parent, "Notebook settings");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let title = Label::builder()
        .label("Available templates")
        .halign(gtk4::Align::Start)
        .build();
    title.add_css_class("title-3");
    body.append(&title);

    let hint = Label::builder()
        .label("Select which page templates appear when creating new pages in this notebook. Leave all unchecked to allow every template.")
        .wrap(true)
        .halign(gtk4::Align::Start)
        .build();
    hint.add_css_class("dim-label");
    body.append(&hint);

    let nb = match notebook_store::get_notebook(state.borrow().db.borrow().conn(), notebook_id) {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("failed to load notebook for settings: {}", e);
            return;
        }
    };
    let assigned: std::collections::HashSet<TemplateId> = nb.assigned_templates.iter().copied().collect();

    let scroller = ScrolledWindow::builder().hexpand(true).vexpand(true).build();
    let list = GtkBox::builder().orientation(Orientation::Vertical).spacing(2).build();
    scroller.set_child(Some(&list));
    body.append(&scroller);

    let templates = sorted_templates(&state);
    let mut checks: Vec<(TemplateId, CheckButton)> = Vec::with_capacity(templates.len());
    for t in templates {
        let cb = CheckButton::with_label(&t.name);
        cb.set_active(assigned.contains(&t.id));
        list.append(&cb);
        checks.push((t.id, cb));
    }
    let checks_rc = Rc::new(checks);

    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build();
    let cancel = Button::with_label("Cancel");
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");
    {
        let win = win.clone();
        let state = state.clone();
        let checks = checks_rc.clone();
        let on_saved = Rc::new(on_saved);
        save.connect_clicked(move |_| {
            let selected: Vec<TemplateId> = checks
                .iter()
                .filter_map(|(id, cb)| if cb.is_active() { Some(*id) } else { None })
                .collect();
            let mut updated = match notebook_store::get_notebook(
                state.borrow().db.borrow().conn(),
                notebook_id,
            ) {
                Ok(n) => n,
                Err(e) => {
                    tracing::error!("failed to load notebook for save: {}", e);
                    win.close();
                    return;
                }
            };
            updated.assigned_templates = selected;
            if let Err(e) = notebook_store::update_notebook(
                state.borrow().db.borrow().conn(),
                &updated,
            ) {
                tracing::error!("failed to update notebook: {}", e);
            }
            (on_saved)();
            win.close();
        });
    }
    row.append(&cancel);
    row.append(&save);
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}

pub fn open_section_settings(
    parent: &ApplicationWindow,
    state: SharedState,
    section_id: SectionId,
    on_saved: Box<dyn Fn()>,
) {
    let win = modal(parent, "Section settings");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let title = Label::builder()
        .label("Section template scope")
        .halign(gtk4::Align::Start)
        .build();
    title.add_css_class("title-3");
    body.append(&title);

    let section = match section_store::get_section(state.borrow().db.borrow().conn(), section_id) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to load section: {}", e);
            return;
        }
    };

    let inherit = CheckButton::with_label("Inherit from notebook");
    inherit.set_active(section.allowed_templates.is_none());
    body.append(&inherit);

    let hint = Label::builder()
        .label("If enabled, this section uses the notebook's template list. Otherwise, choose which templates are allowed here.")
        .wrap(true)
        .halign(gtk4::Align::Start)
        .build();
    hint.add_css_class("dim-label");
    body.append(&hint);

    let scroller = ScrolledWindow::builder().hexpand(true).vexpand(true).build();
    let list = GtkBox::builder().orientation(Orientation::Vertical).spacing(2).build();
    scroller.set_child(Some(&list));
    body.append(&scroller);

    let templates = sorted_templates(&state);
    let allowed: std::collections::HashSet<TemplateId> = section
        .allowed_templates
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut checks: Vec<(TemplateId, CheckButton)> = Vec::with_capacity(templates.len());
    for t in templates {
        let cb = CheckButton::with_label(&t.name);
        cb.set_active(allowed.contains(&t.id));
        cb.set_sensitive(!inherit.is_active());
        list.append(&cb);
        checks.push((t.id, cb));
    }
    let checks_rc: Rc<RefCell<Vec<(TemplateId, CheckButton)>>> = Rc::new(RefCell::new(checks));

    {
        let checks = checks_rc.clone();
        inherit.connect_toggled(move |btn| {
            let on = btn.is_active();
            for (_, cb) in checks.borrow().iter() {
                cb.set_sensitive(!on);
            }
        });
    }

    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build();
    let cancel = Button::with_label("Cancel");
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");
    {
        let win = win.clone();
        let state = state.clone();
        let checks = checks_rc.clone();
        let inherit = inherit.clone();
        let on_saved = Rc::new(on_saved);
        save.connect_clicked(move |_| {
            let allowed = if inherit.is_active() {
                None
            } else {
                let selected: Vec<TemplateId> = checks
                    .borrow()
                    .iter()
                    .filter_map(|(id, cb)| if cb.is_active() { Some(*id) } else { None })
                    .collect();
                Some(selected)
            };
            let mut updated = match section_store::get_section(
                state.borrow().db.borrow().conn(),
                section_id,
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("failed to load section for save: {}", e);
                    win.close();
                    return;
                }
            };
            updated.allowed_templates = allowed;
            if let Err(e) = section_store::update_section(
                state.borrow().db.borrow().conn(),
                &updated,
            ) {
                tracing::error!("failed to update section: {}", e);
            }
            (on_saved)();
            win.close();
        });
    }
    row.append(&cancel);
    row.append(&save);
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}

pub fn open_app_settings(
    parent: &ApplicationWindow,
    state: SharedState,
    on_saved: Box<dyn Fn()>,
) {
    use gtk4::{Entry, FileDialog, FileFilter};
    use std::path::PathBuf;

    let cfg = crate::config::load();
    let win = modal(parent, "App settings");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    body.append(&Label::new(Some("No-page placeholder")));

    let path_label = Label::new(
        cfg.placeholder_image_path
            .as_ref()
            .and_then(|p| p.to_str())
            .or(Some("(no image set)")),
    );
    path_label.set_halign(gtk4::Align::Start);
    path_label.set_wrap(true);
    body.append(&path_label);

    let path_state: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(cfg.placeholder_image_path.clone()));

    let row1 = GtkBox::builder().orientation(Orientation::Horizontal).spacing(8).build();
    let pick_btn = Button::with_label("Choose image…");
    let clear_btn = Button::with_label("Clear");
    row1.append(&pick_btn);
    row1.append(&clear_btn);
    body.append(&row1);

    {
        let parent = parent.clone();
        let path_state = path_state.clone();
        let path_label = path_label.clone();
        pick_btn.connect_clicked(move |_| {
            let dialog = FileDialog::builder().title("Pick placeholder image").build();
            let filter = FileFilter::new();
            filter.add_mime_type("image/*");
            filter.set_name(Some("Images"));
            let store = gtk4::gio::ListStore::new::<FileFilter>();
            store.append(&filter);
            dialog.set_filters(Some(&store));
            let path_state = path_state.clone();
            let path_label = path_label.clone();
            dialog.open(Some(&parent), gtk4::gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(p) = file.path() {
                        path_label.set_text(p.to_str().unwrap_or(""));
                        *path_state.borrow_mut() = Some(p);
                    }
                }
            });
        });
    }
    {
        let path_state = path_state.clone();
        let path_label = path_label.clone();
        clear_btn.connect_clicked(move |_| {
            *path_state.borrow_mut() = None;
            path_label.set_text("(no image set)");
        });
    }

    body.append(&Label::new(Some("Placeholder text (used if no image)")));
    let text_entry = Entry::builder()
        .placeholder_text("Select a page to start drawing")
        .text(cfg.placeholder_text.as_deref().unwrap_or(""))
        .build();
    body.append(&text_entry);

    let row2 = GtkBox::builder().orientation(Orientation::Horizontal).spacing(8).halign(gtk4::Align::End).build();
    let cancel = Button::with_label("Cancel");
    let save = Button::with_label("Save");
    row2.append(&cancel);
    row2.append(&save);
    body.append(&row2);

    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    {
        let win = win.clone();
        let state = state.clone();
        let path_state = path_state.clone();
        save.connect_clicked(move |_| {
            let mut new_cfg = crate::config::load();
            new_cfg.placeholder_image_path = path_state.borrow().clone();
            new_cfg.placeholder_text = {
                let t = text_entry.text().to_string();
                if t.trim().is_empty() { None } else { Some(t) }
            };
            if let Err(e) = crate::config::save(&new_cfg) {
                tracing::error!("save config: {}", e);
            }
            crate::state::reload_placeholder(&state);
            (on_saved)();
            win.close();
        });
    }

    win.set_child(Some(&body));
    win.present();
}
