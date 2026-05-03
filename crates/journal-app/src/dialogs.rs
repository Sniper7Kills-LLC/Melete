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
// {Notebook,Section}Store methods reached via dyn JournalBackend.
use uuid::Uuid;

use crate::state::SharedState;

pub(crate) fn persist_notebook_template(t: &journal_core::NotebookTemplate) {
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
    let (section, notebook) = {
        let mut b = s.backend.borrow_mut();
        (b.get_section(section_id).ok(), b.get_notebook(notebook_id).ok())
    };
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

/// Full editor for a NotebookTemplate. When `edit` is `Some`, pre-populates
/// all fields from the existing template and overwrites it on Save (same id).
/// When `edit` is `None`, creates a new template with a fresh UUID.
pub fn prompt_notebook_template_editor(
    parent: &ApplicationWindow,
    state: SharedState,
    edit: Option<NotebookTemplate>,
    on_save: Box<dyn Fn(TemplateId)>,
) {
    use chrono::Weekday;
    use gtk4::{Expander, ScrolledWindow, ToggleButton};
    use journal_core::{DailySlot, NotebookTemplate, SectionTitleFormats};

    let page_templates: Vec<PageTemplate> = {
        let s = state.borrow();
        let reg = s.templates.borrow();
        let mut v: Vec<PageTemplate> = reg.list().iter().map(|t| (*t).clone()).collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        v
    };
    if page_templates.is_empty() {
        tracing::warn!("no page templates available");
        return;
    }

    let is_edit = edit.is_some();
    let title_str = if is_edit { "Edit Notebook Template" } else { "New Notebook Template" };

    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title_str)
        .default_width(560)
        .default_height(680)
        .build();

    let scroll = ScrolledWindow::builder().hexpand(true).vexpand(true).build();
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();
    scroll.set_child(Some(&body));

    body.append(&Label::new(Some("Template name")));
    let name_entry = Entry::builder()
        .placeholder_text("My Planner")
        .text(edit.as_ref().map(|e| e.name.as_str()).unwrap_or(""))
        .build();
    body.append(&name_entry);

    body.append(&Label::new(Some("Description")));
    let desc_entry = Entry::builder()
        .text(edit.as_ref().map(|e| e.description.as_str()).unwrap_or(""))
        .build();
    body.append(&desc_entry);

    body.append(&Label::new(Some("Group by")));
    let grouping_model = StringList::new(&["Month", "Week"]);
    let grouping_sel = match edit.as_ref().map(|e| &e.grouping) {
        Some(PlannerGrouping::Week) => 1,
        _ => 0,
    };
    let grouping_dropdown = DropDown::builder()
        .model(&grouping_model)
        .selected(grouping_sel)
        .build();
    body.append(&grouping_dropdown);

    let vars_hint = "vars: {year} {month} {month_name} {week} {day} {weekday} {date}";

    body.append(&Label::new(Some("Page title format")));
    let title_entry = Entry::builder()
        .text(
            edit.as_ref()
                .map(|e| e.page_title_format.as_str())
                .unwrap_or("{weekday} {month_name} {day}"),
        )
        .build();
    body.append(&title_entry);
    let h = Label::new(Some(vars_hint));
    h.add_css_class("dim-label");
    h.set_halign(gtk4::Align::Start);
    body.append(&h);

    body.append(&Label::new(Some("Year section title")));
    let year_entry = Entry::builder()
        .text(
            edit.as_ref()
                .map(|e| e.section_title_formats.year.as_str())
                .unwrap_or("{year}"),
        )
        .build();
    body.append(&year_entry);

    body.append(&Label::new(Some("Month wrapper title")));
    let month_entry = Entry::builder()
        .text(
            edit.as_ref()
                .map(|e| e.section_title_formats.month.as_str())
                .unwrap_or("{month_name} {year}"),
        )
        .build();
    body.append(&month_entry);

    body.append(&Label::new(Some("Week wrapper title")));
    let week_entry = Entry::builder()
        .text(
            edit.as_ref()
                .map(|e| e.section_title_formats.week.as_str())
                .unwrap_or("Week {week} {year}"),
        )
        .build();
    body.append(&week_entry);

    let pt_strings: Vec<String> = page_templates.iter().map(|t| t.name.clone()).collect();

    // Helper that builds a collapsible list of single-template-picker rows.
    // Returns (expander_root, list_of_dropdowns_vec).
    type TemplateList = Rc<RefCell<Vec<(DropDown, GtkBox)>>>;

    // Builds a collapsible list of page-template-picker rows, pre-populated
    // with the ids in `initial_ids` when editing an existing template.
    let make_template_list = |label: &str, initial_ids: &[TemplateId]| -> (Expander, TemplateList) {
        let list: TemplateList = Rc::new(RefCell::new(Vec::new()));
        let outer = GtkBox::builder().orientation(Orientation::Vertical).spacing(4).build();
        let rows_box = GtkBox::builder().orientation(Orientation::Vertical).spacing(4).build();
        let expander = Expander::builder().label(label).expanded(false).build();

        let add_btn = Button::with_label("+ Add");
        add_btn.set_halign(gtk4::Align::Start);
        outer.append(&rows_box);
        outer.append(&add_btn);

        // Helper closure to create and insert one row (reused for pre-populate).
        let add_row = {
            let pt_strings_c = pt_strings.clone();
            let rows_box_inner = rows_box.clone();
            let list_inner = list.clone();
            Rc::new(move |selected_idx: usize| {
                let refs: Vec<&str> = pt_strings_c.iter().map(|s| s.as_str()).collect();
                let model = StringList::new(&refs);
                let dd = DropDown::builder()
                    .model(&model)
                    .selected(selected_idx as u32)
                    .build();
                let row = GtkBox::builder().orientation(Orientation::Horizontal).spacing(6).build();
                row.append(&dd);
                let del = Button::from_icon_name("edit-delete-symbolic");
                row.append(&del);

                let row_w = row.clone();
                let rows_box_w = rows_box_inner.clone();
                let list_w = list_inner.clone();
                del.connect_clicked(move |_| {
                    rows_box_w.remove(&row_w);
                    list_w.borrow_mut().retain(|(_, r)| !r.eq(&row_w));
                });

                rows_box_inner.append(&row);
                list_inner.borrow_mut().push((dd, row));
            })
        };

        // Pre-populate from existing template data.
        for tid in initial_ids {
            let sel = page_templates.iter().position(|t| t.id == *tid).unwrap_or(0);
            add_row(sel);
        }

        let add_row_for_btn = add_row.clone();
        add_btn.connect_clicked(move |_| add_row_for_btn(0));

        expander.set_child(Some(&outer));
        (expander, list)
    };

    let (year_start_exp, year_start_list) = make_template_list(
        "Year start templates",
        edit.as_ref().map(|e| e.year_start.as_slice()).unwrap_or(&[]),
    );
    let (before_quarter_exp, before_quarter_list) = make_template_list(
        "Before each quarter",
        edit.as_ref().map(|e| e.before_quarter.as_slice()).unwrap_or(&[]),
    );
    let (before_month_exp, before_month_list) = make_template_list(
        "Before each month",
        edit.as_ref().map(|e| e.before_month.as_slice()).unwrap_or(&[]),
    );
    let (before_week_exp, before_week_list) = make_template_list(
        "Before each week",
        edit.as_ref().map(|e| e.before_week.as_slice()).unwrap_or(&[]),
    );
    body.append(&year_start_exp);
    body.append(&before_quarter_exp);
    body.append(&before_month_exp);
    body.append(&before_week_exp);

    body.append(&Label::new(Some("Daily slots")));
    let slots_box = GtkBox::builder().orientation(Orientation::Vertical).spacing(6).build();
    body.append(&slots_box);

    let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let weekday_list = [
        Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu,
        Weekday::Fri, Weekday::Sat, Weekday::Sun,
    ];

    type SlotRowCtl = (Vec<ToggleButton>, DropDown, GtkBox);
    let slots: Rc<RefCell<Vec<SlotRowCtl>>> = Rc::new(RefCell::new(Vec::new()));

    let make_slot = {
        let pt_strings_owned = pt_strings.clone();
        let slots_box = slots_box.clone();
        let slots = slots.clone();
        Rc::new(move |active_days: &[bool], selected_pt: usize| {
            let row = GtkBox::builder().orientation(Orientation::Horizontal).spacing(6).build();
            let mut day_btns = Vec::with_capacity(7);
            for (i, n) in day_names.iter().enumerate() {
                let b = ToggleButton::builder()
                    .label(*n)
                    .active(*active_days.get(i).unwrap_or(&false))
                    .build();
                row.append(&b);
                day_btns.push(b);
            }
            let refs: Vec<&str> = pt_strings_owned.iter().map(|s| s.as_str()).collect();
            let model = StringList::new(&refs);
            let dd = DropDown::builder().model(&model).selected(selected_pt as u32).build();
            row.append(&dd);
            let remove_btn = Button::from_icon_name("edit-delete-symbolic");
            row.append(&remove_btn);

            let row_w = row.clone();
            let slots_box_w = slots_box.clone();
            let slots_w = slots.clone();
            remove_btn.connect_clicked(move |_| {
                slots_box_w.remove(&row_w);
                slots_w.borrow_mut().retain(|(_, _, r)| !r.eq(&row_w));
            });

            slots_box.append(&row);
            slots.borrow_mut().push((day_btns, dd, row));
        })
    };

    // Pre-populate from existing template data, or add a default all-days slot.
    if let Some(ref existing) = edit {
        for slot in &existing.daily_slots {
            let active: Vec<bool> = weekday_list.iter().map(|wd| slot.days.contains(wd)).collect();
            let sel_pt = slot.templates.first()
                .and_then(|tid| page_templates.iter().position(|t| t.id == *tid))
                .unwrap_or(0);
            make_slot(&active, sel_pt);
        }
    } else {
        let all_true = vec![true; 7];
        make_slot(&all_true, 0);
    }

    let add_slot_btn = Button::with_label("+ Add slot");
    body.append(&add_slot_btn);
    {
        let make_slot = make_slot.clone();
        add_slot_btn.connect_clicked(move |_| {
            let none_active = vec![false; 7];
            make_slot(&none_active, 0);
        });
    }

    let btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .margin_top(8)
        .build();
    let cancel = Button::with_label("Cancel");
    let save = Button::with_label("Save");
    btn_row.append(&cancel);
    btn_row.append(&save);
    body.append(&btn_row);

    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    {
        let win = win.clone();
        let state = state.clone();
        let on_save = Rc::new(on_save);
        let page_templates = page_templates.clone();
        let slots = slots.clone();
        // The id to use: keep the existing one when editing, mint a new one otherwise.
        let existing_id: Option<TemplateId> = edit.as_ref().map(|e| e.id);
        save.connect_clicked(move |_| {
            let mut daily_slots: Vec<DailySlot> = Vec::new();
            for (day_btns, dd, _) in slots.borrow().iter() {
                let days: Vec<Weekday> = day_btns
                    .iter()
                    .enumerate()
                    .filter_map(|(i, b)| if b.is_active() { Some(weekday_list[i]) } else { None })
                    .collect();
                if days.is_empty() {
                    continue;
                }
                let idx = dd.selected() as usize;
                if let Some(pt) = page_templates.get(idx) {
                    daily_slots.push(DailySlot { days, templates: vec![pt.id] });
                }
            }
            let collect_ids = |list: &TemplateList| -> Vec<TemplateId> {
                list.borrow().iter().filter_map(|(dd, _)| {
                    page_templates.get(dd.selected() as usize).map(|t| t.id)
                }).collect()
            };
            let id = existing_id.unwrap_or_else(|| TemplateId(Uuid::new_v4()));
            let nt = NotebookTemplate {
                id,
                name: name_entry.text().to_string(),
                description: desc_entry.text().to_string(),
                year_start: collect_ids(&year_start_list),
                before_quarter: collect_ids(&before_quarter_list),
                before_month: collect_ids(&before_month_list),
                before_week: collect_ids(&before_week_list),
                daily_slots,
                grouping: match grouping_dropdown.selected() {
                    1 => PlannerGrouping::Week,
                    _ => PlannerGrouping::Month,
                },
                page_title_format: title_entry.text().to_string(),
                section_title_formats: SectionTitleFormats {
                    year: year_entry.text().to_string(),
                    month: month_entry.text().to_string(),
                    week: week_entry.text().to_string(),
                },
                entry_options: edit.as_ref().map(|e| e.entry_options.clone()).unwrap_or_default(),
            };
            persist_notebook_template(&nt);
            state.borrow().notebook_templates.borrow_mut().insert(nt);
            (on_save)(id);
            win.close();
        });
    }

    win.set_child(Some(&scroll));
    win.present();
}

/// Create a new notebook template. Thin wrapper around `prompt_notebook_template_editor`.
/// Kept for back-compat; the canonical entry point is now the full-screen
/// stack-page editor in `notebook_template_creator`.
#[allow(dead_code)]
pub fn prompt_new_notebook_template(
    parent: &ApplicationWindow,
    state: SharedState,
    on_ok: Box<dyn Fn(TemplateId)>,
) {
    prompt_notebook_template_editor(parent, state, None, on_ok);
}
