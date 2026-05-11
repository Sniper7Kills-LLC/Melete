use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box as GtkBox, Button, FlowBox, GestureClick, Image, Label,
    Orientation, PopoverMenu, ScrolledWindow, SelectionMode, Separator, Window,
};
use melete_core::{Notebook, NotebookId, NotebookKind, PageTemplate};

#[derive(Debug, Clone, Copy)]
pub enum NotebookKindChoice {
    Standard,
    Planner,
}

/// Small modal that asks the user whether their new notebook should be a
/// free-form notebook or a calendar-based planner. Replaces the old pair of
/// header buttons ("New notebook" / "New planner") with a single CTA + this
/// chooser.
fn prompt_notebook_kind(
    parent: &ApplicationWindow,
    on_pick: Box<dyn Fn(NotebookKindChoice) + 'static>,
) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("New notebook")
        .default_width(420)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();

    let prompt = Label::builder()
        .label("What kind of notebook?")
        .halign(Align::Start)
        .build();
    prompt.add_css_class("title-3");
    body.append(&prompt);

    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(12)
        .build();

    let make_card = |icon: &str, title: &str, subtitle: &str| -> Button {
        let btn = Button::new();
        btn.add_css_class("notebook-card");
        btn.add_css_class("flat");
        btn.set_hexpand(true);
        let v = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .build();
        let icon_w = Image::from_icon_name(icon);
        icon_w.set_pixel_size(40);
        icon_w.set_halign(Align::Start);
        v.append(&icon_w);
        let t = Label::builder().label(title).halign(Align::Start).build();
        t.add_css_class("card-title");
        v.append(&t);
        let s = Label::builder()
            .label(subtitle)
            .halign(Align::Start)
            .wrap(true)
            .build();
        s.add_css_class("card-subtitle");
        v.append(&s);
        btn.set_child(Some(&v));
        btn
    };

    let standard_card = make_card(
        "x-office-address-book-symbolic",
        "Notebook",
        "Free-form sections and pages — like a paper notebook.",
    );
    let planner_card = make_card(
        "x-office-calendar-symbolic",
        "Planner",
        "Calendar-based pages auto-generated from a notebook template.",
    );
    row.append(&standard_card);
    row.append(&planner_card);
    body.append(&row);

    win.set_child(Some(&body));

    let on_pick = Rc::new(on_pick);
    {
        let on_pick = on_pick.clone();
        let win = win.clone();
        standard_card.connect_clicked(move |_| {
            win.close();
            (on_pick)(NotebookKindChoice::Standard);
        });
    }
    {
        let on_pick = on_pick.clone();
        let win = win.clone();
        planner_card.connect_clicked(move |_| {
            win.close();
            (on_pick)(NotebookKindChoice::Planner);
        });
    }

    win.present();
}
use melete_storage::NotebookBackend;
use uuid::Uuid;

use crate::dialogs;
use crate::settings_dialogs;
use crate::state::SharedState;
use crate::template_manager;

/// Build the home screen widget. `on_open` is called when a notebook is selected.
/// Returns the root widget — caller is responsible for placing it in the window.
pub fn build_home(
    parent: &ApplicationWindow,
    state: SharedState,
    db: Rc<RefCell<dyn NotebookBackend>>,
    on_open: Rc<dyn Fn(NotebookId)>,
    on_open_template_editor: Rc<dyn Fn(Option<PageTemplate>)>,
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

    let templates_btn = Button::with_label("Templates");
    templates_btn.set_tooltip_text(Some("Manage page and notebook templates"));
    header.append(&templates_btn);

    // Single "New notebook" button — picks Standard or Planner via a small
    // chooser dialog. Replaces the old separate "New planner" button.
    let new_btn = Button::with_label("New notebook");
    new_btn.add_css_class("suggested-action");
    header.append(&new_btn);
    root.append(&header);

    {
        let parent = parent.clone();
        let state = state.clone();
        let opener = on_open_template_editor.clone();
        templates_btn.connect_clicked(move |_| {
            template_manager::open(&parent, state.clone(), opener.clone());
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
    let card_ctx = CardCtx {
        parent: parent.clone(),
        state: state.clone(),
        db: db.clone(),
        list_box: list_box_rc.clone(),
        on_open: on_open.clone(),
    };
    refresh_list(&list_box_rc, &card_ctx);

    {
        let parent = parent.clone();
        let card_ctx = card_ctx.clone();
        new_btn.connect_clicked(move |_| {
            let parent_inner = parent.clone();
            let card_ctx_inner = card_ctx.clone();

            // Step 1: ask the user which kind of notebook they want.
            prompt_notebook_kind(
                &parent,
                Box::new(move |kind| {
                    let parent2 = parent_inner.clone();
                    let ctx2 = card_ctx_inner.clone();
                    match kind {
                        NotebookKindChoice::Standard => {
                            let ctx2 = ctx2.clone();
                            dialogs::prompt_new_notebook(
                                &parent2,
                                Box::new(move |name| {
                                    let nb = Notebook {
                                        id: NotebookId(Uuid::new_v4()),
                                        name,
                                        kind: NotebookKind::Standard,
                                        assigned_templates: Vec::new(),
                                    };
                                    if let Err(e) = ctx2.db.borrow_mut().insert_notebook(&nb) {
                                        tracing::error!("failed to insert notebook: {}", e);
                                        return;
                                    }
                                    refresh_list(&ctx2.list_box, &ctx2);
                                }),
                            );
                        }
                        NotebookKindChoice::Planner => {
                            let ctx2 = ctx2.clone();
                            dialogs::prompt_new_planner(
                                &parent2,
                                ctx2.state.clone(),
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
                                    if let Err(e) = ctx2.db.borrow_mut().insert_notebook(&nb) {
                                        tracing::error!("failed to insert planner: {}", e);
                                        return;
                                    }
                                    refresh_list(&ctx2.list_box, &ctx2);
                                }),
                            );
                        }
                    }
                }),
            );
        });
    }

    root
}

#[derive(Clone)]
struct CardCtx {
    parent: ApplicationWindow,
    state: SharedState,
    db: Rc<RefCell<dyn NotebookBackend>>,
    list_box: Rc<GtkBox>,
    on_open: Rc<dyn Fn(NotebookId)>,
}

fn notebook_card(nb: &Notebook, ctx: &CardCtx) -> Button {
    // Outer button so the entire card is a single tap target.
    let btn = Button::new();
    btn.add_css_class("notebook-card");
    btn.add_css_class("flat");

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .halign(Align::Fill)
        .valign(Align::Start)
        .build();

    let (kind_text, icon_name) = match &nb.kind {
        NotebookKind::Standard => ("Notebook", "x-office-address-book-symbolic"),
        NotebookKind::Planner { .. } => ("Planner", "x-office-calendar-symbolic"),
    };

    let header_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(Align::Fill)
        .build();
    let icon = Image::from_icon_name(icon_name);
    icon.set_icon_size(gtk4::IconSize::Large);
    icon.add_css_class("dim-label");
    header_row.append(&icon);
    let kind_lbl = Label::builder()
        .label(kind_text)
        .halign(Align::Start)
        .hexpand(true)
        .build();
    kind_lbl.add_css_class("card-kind");
    header_row.append(&kind_lbl);
    body.append(&header_row);

    let title = Label::builder()
        .label(&nb.name)
        .halign(Align::Start)
        .wrap(true)
        .build();
    title.add_css_class("card-title");
    body.append(&title);

    let subtitle_text = match &nb.kind {
        NotebookKind::Planner { creation_date, .. } => format!("Created {}", creation_date),
        NotebookKind::Standard => "Standard notebook".to_string(),
    };
    let subtitle = Label::builder()
        .label(&subtitle_text)
        .halign(Align::Start)
        .build();
    subtitle.add_css_class("card-subtitle");
    body.append(&subtitle);

    btn.set_child(Some(&body));
    let id = nb.id;
    {
        let on_open = ctx.on_open.clone();
        btn.connect_clicked(move |_| on_open(id));
    }
    attach_card_context_menu(&btn, nb, ctx);
    btn
}

fn attach_card_context_menu(btn: &Button, nb: &Notebook, ctx: &CardCtx) {
    let menu = gio::Menu::new();
    menu.append(Some("Manage templates…"), Some("nb-ctx.templates"));
    menu.append(Some("Delete notebook…"), Some("nb-ctx.delete"));

    let popover = PopoverMenu::from_model(Some(&menu));
    popover.set_parent(btn);
    popover.set_has_arrow(false);

    let action_group = gio::SimpleActionGroup::new();
    let id = nb.id;
    let nb_name = nb.name.clone();

    {
        let ctx = ctx.clone();
        let act = gio::SimpleAction::new("templates", None);
        act.connect_activate(move |_, _| {
            let ctx_for_save = ctx.clone();
            settings_dialogs::open_notebook_settings(
                &ctx.parent,
                ctx.state.clone(),
                id,
                Box::new(move || {
                    refresh_list(
                        &ctx_for_save.list_box,
                        &ctx_for_save,
                    );
                }),
            );
        });
        action_group.add_action(&act);
    }

    {
        let ctx = ctx.clone();
        let nb_name = nb_name.clone();
        let act = gio::SimpleAction::new("delete", None);
        act.connect_activate(move |_, _| {
            confirm_delete_notebook(&ctx, id, &nb_name);
        });
        action_group.add_action(&act);
    }

    btn.insert_action_group("nb-ctx", Some(&action_group));

    let right_click = GestureClick::new();
    right_click.set_button(gtk4::gdk::BUTTON_SECONDARY);
    right_click.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let popover = popover.clone();
        right_click.connect_pressed(move |g, _n, x, y| {
            let rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
            popover.set_pointing_to(Some(&rect));
            popover.popup();
            g.set_state(gtk4::EventSequenceState::Claimed);
        });
    }
    btn.add_controller(right_click);
}

fn confirm_delete_notebook(ctx: &CardCtx, id: NotebookId, nb_name: &str) {
    let win = Window::builder()
        .transient_for(&ctx.parent)
        .modal(true)
        .title("Delete notebook?")
        .default_width(360)
        .build();
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();
    let msg = Label::new(Some(&format!(
        "Permanently delete \u{201c}{}\u{201d}? All sections, pages, and strokes inside this notebook will be removed.",
        nb_name
    )));
    msg.set_wrap(true);
    msg.set_halign(Align::Start);
    body.append(&msg);
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(Align::End)
        .build();
    let cancel = Button::with_label("Cancel");
    let delete = Button::with_label("Delete");
    delete.add_css_class("destructive-action");
    row.append(&cancel);
    row.append(&delete);
    body.append(&row);
    win.set_child(Some(&body));

    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    {
        let win = win.clone();
        let ctx = ctx.clone();
        delete.connect_clicked(move |_| {
            // Scope the borrow_mut so the RefMut is dropped before
            // refresh_list runs (refresh_list re-borrows the backend).
            let result = ctx.db.borrow_mut().delete_notebook(id);
            match result {
                Err(e) => tracing::error!("failed to delete notebook {:?}: {}", id, e),
                Ok(_) => {
                    let mut cfg = crate::config::load();
                    cfg.recent_notebook_ids.retain(|u| *u != id.0);
                    if let Err(e) = crate::config::save(&cfg) {
                        tracing::warn!("failed to prune recent notebooks: {}", e);
                    }
                    refresh_list(&ctx.list_box, &ctx);
                }
            }
            win.close();
        });
    }
    win.present();
}

fn build_card_grid(notebooks: &[&Notebook], ctx: &CardCtx) -> FlowBox {
    let flow = FlowBox::builder()
        .max_children_per_line(8)
        .min_children_per_line(1)
        .selection_mode(SelectionMode::None)
        .row_spacing(12)
        .column_spacing(12)
        .homogeneous(true)
        .activate_on_single_click(false)
        .build();
    for nb in notebooks {
        flow.append(&notebook_card(nb, ctx));
    }
    flow
}

fn build_empty_state() -> GtkBox {
    let v = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .valign(Align::Center)
        .vexpand(true)
        .build();
    v.add_css_class("empty-state");
    let icon = Image::from_icon_name("x-office-address-book-symbolic");
    icon.add_css_class("empty-state-icon");
    icon.set_pixel_size(96);
    v.append(&icon);
    let title = Label::new(Some("Start your first notebook"));
    title.add_css_class("empty-state-title");
    v.append(&title);
    let subtitle = Label::new(Some(
        "Use the buttons above to create a planner or a free-form notebook.",
    ));
    subtitle.add_css_class("empty-state-subtitle");
    subtitle.set_wrap(true);
    subtitle.set_justify(gtk4::Justification::Center);
    v.append(&subtitle);
    v
}

fn refresh_list(list_box: &Rc<GtkBox>, ctx: &CardCtx) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let notebooks = match ctx.db.borrow_mut().list_notebooks() {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("failed to list notebooks: {}", e);
            return;
        }
    };

    if notebooks.is_empty() {
        list_box.append(&build_empty_state());
        return;
    }

    let recent_ids = crate::config::load().recent_notebook_ids;
    if !recent_ids.is_empty() {
        let recent_notebooks: Vec<&Notebook> = recent_ids
            .iter()
            .filter_map(|uid| notebooks.iter().find(|nb| nb.id.0 == *uid))
            .collect();

        if !recent_notebooks.is_empty() {
            let recent_label = Label::builder()
                .label("Recent")
                .halign(Align::Start)
                .build();
            recent_label.add_css_class("heading");
            list_box.append(&recent_label);

            list_box.append(&build_card_grid(&recent_notebooks, ctx));

            let sep = Separator::new(Orientation::Horizontal);
            sep.set_margin_top(16);
            sep.set_margin_bottom(8);
            list_box.append(&sep);

            let all_label = Label::builder()
                .label("All Notebooks")
                .halign(Align::Start)
                .build();
            all_label.add_css_class("heading");
            list_box.append(&all_label);
        }
    }

    let all_refs: Vec<&Notebook> = notebooks.iter().collect();
    list_box.append(&build_card_grid(&all_refs, ctx));
}
