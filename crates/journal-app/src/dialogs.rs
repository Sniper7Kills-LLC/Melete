use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, DropDown, Entry, Label, Orientation, StringList,
    Window,
};
use journal_core::{PageTemplate, TemplateId};

fn modal(parent: &ApplicationWindow, title: &str) -> Window {
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(360)
        .build();
    win
}

fn build_button_row<F: Fn() + 'static>(win: &Window, on_ok: F) -> GtkBox {
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
    let ok = Button::with_label("OK");
    ok.add_css_class("suggested-action");
    {
        let win = win.clone();
        ok.connect_clicked(move |_| {
            on_ok();
            win.close();
        });
    }
    row.append(&cancel);
    row.append(&ok);
    row
}

pub fn prompt_new_notebook(parent: &ApplicationWindow, on_ok: Box<dyn Fn(String)>) {
    let win = modal(parent, "New Notebook");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    body.append(&Label::new(Some("Notebook name")));
    let entry = Entry::builder().placeholder_text("My Notebook").build();
    body.append(&entry);

    let entry_for_ok = entry.clone();
    let row = build_button_row(&win, move || {
        let name = entry_for_ok.text().to_string();
        if !name.trim().is_empty() {
            on_ok(name);
        }
    });
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}

pub fn prompt_new_section(parent: &ApplicationWindow, on_ok: Box<dyn Fn(String)>) {
    let win = modal(parent, "New Section");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    body.append(&Label::new(Some("Section name")));
    let entry = Entry::builder().placeholder_text("Section").build();
    body.append(&entry);

    let entry_for_ok = entry.clone();
    let row = build_button_row(&win, move || {
        let name = entry_for_ok.text().to_string();
        if !name.trim().is_empty() {
            on_ok(name);
        }
    });
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}

pub fn prompt_rename(
    parent: &ApplicationWindow,
    title: &str,
    current_name: &str,
    on_ok: Box<dyn Fn(String)>,
) {
    let win = modal(parent, title);
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    body.append(&Label::new(Some("Name")));
    let entry = Entry::builder().text(current_name).build();
    body.append(&entry);

    let entry_for_ok = entry.clone();
    let row = build_button_row(&win, move || {
        let name = entry_for_ok.text().to_string();
        on_ok(name);
    });
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}

/// Prompt for new page name + template choice.
/// templates[0] is the "Blank" choice (None template id). Remaining are concrete templates.
pub fn prompt_new_page(
    parent: &ApplicationWindow,
    templates: Vec<PageTemplate>,
    on_ok: Box<dyn Fn(Option<TemplateId>)>,
) {
    let win = modal(parent, "New Page");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    body.append(&Label::new(Some("Template")));

    let names: Vec<String> = std::iter::once("Blank".to_string())
        .chain(templates.iter().map(|t| t.name.clone()))
        .collect();
    let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let model = StringList::new(&name_refs);
    let dropdown = DropDown::builder().model(&model).selected(0).build();
    body.append(&dropdown);

    let templates_rc: Rc<RefCell<Vec<PageTemplate>>> = Rc::new(RefCell::new(templates));
    let dropdown_for_ok = dropdown.clone();
    let templates_for_ok = templates_rc.clone();
    let row = build_button_row(&win, move || {
        let idx = dropdown_for_ok.selected() as usize;
        let template_id = if idx == 0 {
            None
        } else {
            templates_for_ok.borrow().get(idx - 1).map(|t| t.id)
        };
        on_ok(template_id);
    });
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}
