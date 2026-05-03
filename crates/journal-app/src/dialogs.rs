use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use chrono::NaiveDate;
use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, Calendar, DropDown, Entry, Label, Orientation,
    StringList, Window,
};
use journal_core::{
    NotebookId, NotebookTemplate, PageTemplate, PlannerGrouping, SectionId, TemplateId,
};
use journal_storage::{notebook_store, section_store};
use uuid::Uuid;

use crate::state::SharedState;

fn persist_notebook_template(t: &journal_core::NotebookTemplate) {
    let dir = match dirs::data_dir() {
        Some(d) => d.join("journal").join("notebook_templates"),
        None => return,
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("create notebook_templates dir failed: {}", e);
        return;
    }
    let path = dir.join(format!("{}.toml", t.id.0));
    let text = match toml::to_string(t) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("serialize notebook template failed: {}", e);
            return;
        }
    };
    if let Err(e) = std::fs::write(&path, text) {
        tracing::warn!("write notebook template failed: {}", e);
    }
}

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

/// Prompt for new page template choice. The dropdown shows "Blank (no template)"
/// at the top, followed by the templates allowed for `section_id`.
///
/// Filtering rule:
/// - If the section has `allowed_templates = Some(list)`, that list is used.
/// - Else if the notebook has a non-empty `assigned_templates`, use that.
/// - Else, show all registered templates.
pub fn prompt_new_page(
    parent: &ApplicationWindow,
    state: SharedState,
    notebook_id: NotebookId,
    section_id: SectionId,
    on_ok: Box<dyn Fn(Option<TemplateId>)>,
) {
    let templates = available_templates_for_section(&state, notebook_id, section_id);

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

    let names: Vec<String> = std::iter::once("Blank (no template)".to_string())
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

fn available_templates_for_section(
    state: &SharedState,
    notebook_id: NotebookId,
    section_id: SectionId,
) -> Vec<PageTemplate> {
    let s = state.borrow();
    let conn = s.db.borrow();
    let section = section_store::get_section(conn.conn(), section_id).ok();
    let notebook = notebook_store::get_notebook(conn.conn(), notebook_id).ok();
    drop(conn);
    let reg = s.templates.borrow();
    let all: Vec<PageTemplate> = {
        let mut v: Vec<PageTemplate> = reg.list().iter().map(|t| (*t).clone()).collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        v
    };

    let allow_filter: Option<HashSet<TemplateId>> = match section.and_then(|s| s.allowed_templates)
    {
        Some(list) => Some(list.into_iter().collect()),
        None => match notebook.map(|n| n.assigned_templates) {
            Some(v) if !v.is_empty() => Some(v.into_iter().collect()),
            _ => None,
        },
    };

    match allow_filter {
        Some(set) => all.into_iter().filter(|t| set.contains(&t.id)).collect(),
        None => all,
    }
}

/// The user's choices when creating a planner notebook. The `template_id`
/// returned here is a per-notebook clone that the caller has already inserted
/// into the registry — it has the chosen `grouping` and `page_title_format`
/// applied. This avoids mutating the built-in template entries.
pub struct PlannerChoice {
    pub name: String,
    pub template_id: TemplateId,
    pub creation_date: NaiveDate,
}

pub fn prompt_new_planner(
    parent: &ApplicationWindow,
    state: SharedState,
    on_ok: Box<dyn Fn(PlannerChoice)>,
) {
    let templates: Vec<NotebookTemplate> = {
        let s = state.borrow();
        let reg = s.notebook_templates.borrow();
        let mut v: Vec<NotebookTemplate> = reg.list().iter().map(|t| (*t).clone()).collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        v
    };
    if templates.is_empty() {
        tracing::warn!("no notebook templates registered; cannot create planner");
        return;
    }

    let win = modal(parent, "New Planner");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    body.append(&Label::new(Some("Planner name")));
    let name_entry = Entry::builder().placeholder_text("My Planner").build();
    body.append(&name_entry);

    body.append(&Label::new(Some("Notebook template")));
    let template_names: Vec<&str> = templates.iter().map(|t| t.name.as_str()).collect();
    let model = StringList::new(&template_names);
    let template_dropdown = DropDown::builder().model(&model).selected(0).build();
    body.append(&template_dropdown);

    body.append(&Label::new(Some("Group by")));
    let grouping_model = StringList::new(&["Month", "Week"]);
    let grouping_dropdown = DropDown::builder().model(&grouping_model).selected(0).build();
    body.append(&grouping_dropdown);

    body.append(&Label::new(Some("Page title format")));
    let title_entry = Entry::builder().text(&templates[0].page_title_format).build();
    body.append(&title_entry);
    {
        let templates = templates.clone();
        let title_entry = title_entry.clone();
        template_dropdown.connect_selected_notify(move |dd| {
            if let Some(t) = templates.get(dd.selected() as usize) {
                title_entry.set_text(&t.page_title_format);
                // Reflect template's default grouping in the dropdown too.
            }
        });
    }

    body.append(&Label::new(Some("Creation date")));
    let calendar = Calendar::new();
    body.append(&calendar);

    let templates_rc = Rc::new(templates);
    let on_ok = Rc::new(on_ok);
    let row = build_button_row(&win, {
        let templates = templates_rc.clone();
        let state = state.clone();
        let name_entry = name_entry.clone();
        let template_dropdown = template_dropdown.clone();
        let grouping_dropdown = grouping_dropdown.clone();
        let title_entry = title_entry.clone();
        let calendar = calendar.clone();
        let on_ok = on_ok.clone();
        move || {
            let name = name_entry.text().to_string();
            if name.trim().is_empty() {
                return;
            }
            let idx = template_dropdown.selected() as usize;
            let Some(base) = templates.get(idx) else {
                return;
            };
            let grouping = match grouping_dropdown.selected() {
                1 => PlannerGrouping::Week,
                _ => PlannerGrouping::Month,
            };
            let page_title_format = {
                let t = title_entry.text().to_string();
                if t.trim().is_empty() {
                    base.page_title_format.clone()
                } else {
                    t
                }
            };

            // Calendar yields gtk's GDateTime; pull components and build a NaiveDate.
            let dt = calendar.date();
            let creation_date = NaiveDate::from_ymd_opt(
                dt.year(),
                dt.month() as u32,
                dt.day_of_month() as u32,
            )
            .unwrap_or_else(|| chrono::Utc::now().date_naive());

            // Clone the chosen NotebookTemplate, override grouping + title
            // format, then insert under a fresh UUID so the registry retains
            // the unmodified built-in.
            let mut clone = base.clone();
            clone.id = TemplateId(Uuid::new_v4());
            clone.grouping = grouping;
            clone.page_title_format = page_title_format;
            let new_id = clone.id;
            persist_notebook_template(&clone);
            state.borrow().notebook_templates.borrow_mut().insert(clone);

            (on_ok)(PlannerChoice {
                name,
                template_id: new_id,
                creation_date,
            });
        }
    });
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}

/// Stub editor for a custom NotebookTemplate. Currently asks for a name +
/// grouping + page title format, clones the first built-in template under a
/// new id, and saves it to the registry. Full slot editing is out of scope
/// for this PR.
#[allow(dead_code)]
pub fn prompt_new_notebook_template(
    parent: &ApplicationWindow,
    state: SharedState,
    on_ok: Box<dyn Fn(TemplateId)>,
) {
    let base = match state.borrow().notebook_templates.borrow().list().first() {
        Some(t) => (*t).clone(),
        None => {
            tracing::warn!("no notebook templates to clone");
            return;
        }
    };

    let win = modal(parent, "New Notebook Template");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    body.append(&Label::new(Some("Template name")));
    let name_entry = Entry::builder().text(&base.name).build();
    body.append(&name_entry);

    body.append(&Label::new(Some("Group by")));
    let grouping_model = StringList::new(&["Month", "Week"]);
    let grouping_dropdown = DropDown::builder().model(&grouping_model).selected(0).build();
    body.append(&grouping_dropdown);

    body.append(&Label::new(Some("Page title format")));
    let title_entry = Entry::builder().text(&base.page_title_format).build();
    body.append(&title_entry);

    let on_ok = Rc::new(on_ok);
    let row = build_button_row(&win, {
        let state = state.clone();
        let name_entry = name_entry.clone();
        let grouping_dropdown = grouping_dropdown.clone();
        let title_entry = title_entry.clone();
        let on_ok = on_ok.clone();
        let base = base.clone();
        move || {
            let mut clone = base.clone();
            clone.id = TemplateId(Uuid::new_v4());
            clone.name = name_entry.text().to_string();
            clone.grouping = match grouping_dropdown.selected() {
                1 => PlannerGrouping::Week,
                _ => PlannerGrouping::Month,
            };
            let pt = title_entry.text().to_string();
            if !pt.trim().is_empty() {
                clone.page_title_format = pt;
            }
            let id = clone.id;
            state.borrow().notebook_templates.borrow_mut().insert(clone);
            (on_ok)(id);
        }
    });
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}
