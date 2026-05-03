use std::cell::RefCell;
use std::rc::Rc;

use chrono::Utc;
use gtk4::gdk::{ContentProvider, DragAction};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, DrawingArea as GtkDrawingArea, DragSource,
    DrawingArea, DropTarget, Entry, EventControllerFocus, EventControllerKey, Expander,
    GestureClick, Label, Orientation, Overlay, Paned, PopoverMenu, ScrolledWindow, Stack, Window,
};
use journal_core::{NotebookId, Page, PageId, Section, SectionId};
use journal_storage::JournalBackend;
use uuid::Uuid;

use crate::dialogs;
use crate::state::{self, SharedState};

pub struct NotebookView {
    pub root: GtkBox,
}

#[derive(Clone)]
struct SidebarCtx {
    parent: ApplicationWindow,
    sections_box: Rc<GtkBox>,
    db: Rc<RefCell<dyn JournalBackend>>,
    state: SharedState,
    notebook_id: NotebookId,
    canvas: DrawingArea,
    /// Planner notebooks own their own page/section structure; we hide the
    /// "+ New Page" / "+ New Section" affordances when this is true.
    is_planner: bool,
}

impl SidebarCtx {
    fn refresh(&self) {
        refresh_sections(
            &self.parent,
            &self.sections_box,
            self.db.clone(),
            self.state.clone(),
            self.notebook_id,
            self.canvas.clone(),
            self.is_planner,
        );
    }
}

pub fn build_notebook_view(
    parent: &ApplicationWindow,
    state: SharedState,
    notebook_id: NotebookId,
    canvas: DrawingArea,
    canvas_pane: Overlay,
) -> NotebookView {
    let paned = Paned::new(Orientation::Horizontal);
    paned.set_position(280);
    paned.set_hexpand(true);
    paned.set_vexpand(true);

    let sidebar_root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(0)
        .width_request(240)
        .build();

    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let sections_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();
    scroller.set_child(Some(&sections_box));
    sidebar_root.append(&scroller);

    // Planner notebooks own their own structure — pages and sections are
    // generated automatically by date navigation. Free-form section/page
    // creation is hidden so the user can't drift the structure out of sync.
    let is_planner = matches!(
        state.borrow().backend.borrow_mut().get_notebook(notebook_id),
        Ok(nb) if matches!(nb.kind, journal_core::NotebookKind::Planner { .. })
    );

    let new_section_btn = Button::with_label("+ New Section");
    new_section_btn.set_margin_start(8);
    new_section_btn.set_margin_end(8);
    new_section_btn.set_margin_top(4);
    new_section_btn.set_margin_bottom(8);
    new_section_btn.set_visible(!is_planner);
    sidebar_root.append(&new_section_btn);

    paned.set_start_child(Some(&sidebar_root));
    paned.set_end_child(Some(&canvas_pane));
    paned.set_resize_start_child(false);
    paned.set_shrink_start_child(false);

    let db = state.borrow().backend.clone();
    let sections_box_rc = Rc::new(sections_box);

    let ctx = SidebarCtx {
        parent: parent.clone(),
        sections_box: sections_box_rc.clone(),
        db: db.clone(),
        state: state.clone(),
        notebook_id,
        canvas: canvas.clone(),
        is_planner,
    };

    ctx.refresh();

    {
        let ctx = ctx.clone();
        new_section_btn.connect_clicked(move |_| {
            let ctx_inner = ctx.clone();
            dialogs::prompt_new_section(
                &ctx.parent,
                Box::new(move |name| {
                    let position = match ctx_inner.db.borrow_mut().list_sections(ctx_inner.notebook_id) {
                        Ok(v) => v.len() as u32,
                        Err(_) => 0,
                    };
                    let section = Section {
                        id: SectionId(Uuid::new_v4()),
                        notebook_id: ctx_inner.notebook_id,
                        name,
                        position,
                        allowed_templates: None,
                        parent_section_id: None,
                    };
                    if let Err(e) =
                        ctx_inner.db.borrow_mut().insert_section(&section)
                    {
                        tracing::error!("failed to insert section: {}", e);
                        return;
                    }
                    ctx_inner.refresh();
                }),
            );
        });
    }

    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    if let Some(template) = crate::views::planner_nav::resolve_planner_template(&state, notebook_id) {
        let ctx_for_refresh = ctx.clone();
        let on_refresh: Rc<dyn Fn()> = Rc::new(move || ctx_for_refresh.refresh());
        let strip = crate::views::planner_nav::build_nav_strip(
            state.clone(),
            canvas.clone(),
            notebook_id,
            template,
            on_refresh,
        );
        root.append(&strip);
    }
    root.append(&paned);

    // Auto-collapse the sidebar when the window is in portrait orientation
    // (height > width). In portrait, drawing space is at a premium and the
    // sidebar takes a disproportionate slice; switch back to landscape to
    // reveal it again. User override: drag the Paned divider in either
    // direction; the auto-collapse only fires when orientation crosses
    // between portrait and landscape, so a manual position is preserved
    // until the next rotation.
    {
        let paned_for_tick = paned.clone();
        let last_orientation: Rc<std::cell::Cell<Option<bool>>> =
            Rc::new(std::cell::Cell::new(None));
        let last_landscape_position: Rc<std::cell::Cell<i32>> =
            Rc::new(std::cell::Cell::new(280));
        paned.add_tick_callback(move |p, _| {
            let w = p.width();
            let h = p.height();
            if w == 0 || h == 0 {
                return gtk4::glib::ControlFlow::Continue;
            }
            let portrait = h > w;
            let was_portrait = last_orientation.get();
            if was_portrait != Some(portrait) {
                if portrait {
                    let pos = paned_for_tick.position();
                    if pos > 0 {
                        last_landscape_position.set(pos);
                    }
                    paned_for_tick.set_position(0);
                } else {
                    let pos = last_landscape_position.get().max(240);
                    paned_for_tick.set_position(pos);
                }
                last_orientation.set(Some(portrait));
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    NotebookView { root }
}

fn refresh_sections(
    parent: &ApplicationWindow,
    sections_box: &Rc<GtkBox>,
    db: Rc<RefCell<dyn JournalBackend>>,
    state: SharedState,
    notebook_id: NotebookId,
    canvas: DrawingArea,
    is_planner: bool,
) {
    while let Some(child) = sections_box.first_child() {
        sections_box.remove(&child);
    }

    let roots = match db.borrow_mut().list_root_sections(notebook_id) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("failed to list root sections: {}", e);
            return;
        }
    };

    if roots.is_empty() {
        let empty_text = if is_planner {
            "Pages appear here as you navigate to dates above."
        } else {
            "No sections — add one below."
        };
        let empty = Label::new(Some(empty_text));
        empty.add_css_class("dim-label");
        empty.set_halign(gtk4::Align::Start);
        empty.set_wrap(true);
        sections_box.append(&empty);
        return;
    }

    let ctx = SidebarCtx {
        parent: parent.clone(),
        sections_box: sections_box.clone(),
        db: db.clone(),
        state: state.clone(),
        notebook_id,
        canvas: canvas.clone(),
        is_planner,
    };

    for section in roots {
        let row = build_section_row(&ctx, section, 0);
        sections_box.append(&row);
    }
}

fn build_section_row(ctx: &SidebarCtx, section: Section, depth: u32) -> GtkBox {
    let wrapper = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(0)
        .build();
    if depth > 0 {
        wrapper.add_css_class("section-nested");
    }

    let expander = Expander::builder()
        .label(&section.name)
        .expanded(true)
        .build();

    let inner = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .margin_start(if depth == 0 { 8 } else { 12 })
        .build();

    let pages_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .build();
    inner.append(&pages_box);

    let pages = match ctx.db.borrow_mut().list_pages(section.id) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("failed to list pages: {}", e);
            Vec::new()
        }
    };

    for (idx, page) in pages.iter().enumerate() {
        let row = build_page_row(ctx, page, idx as u32);
        pages_box.append(&row);
    }

    // Render child sections as nested expanders inside this section's body.
    let child_sections = match ctx.db.borrow_mut().list_child_sections(section.id) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("failed to list child sections: {}", e);
            Vec::new()
        }
    };
    for child in child_sections {
        let row = build_section_row(ctx, child, depth + 1);
        inner.append(&row);
    }

    let new_page_btn = Button::with_label("+ New Page");
    new_page_btn.set_halign(gtk4::Align::Start);
    new_page_btn.set_visible(!ctx.is_planner);
    inner.append(&new_page_btn);

    let section_id = section.id;
    let notebook_id = ctx.notebook_id;
    {
        let ctx_outer = ctx.clone();
        new_page_btn.connect_clicked(move |_| {
            let ctx_inner = ctx_outer.clone();
            dialogs::prompt_new_page(
                &ctx_outer.parent,
                ctx_outer.state.clone(),
                notebook_id,
                section_id,
                Box::new(move |template_id| {
                    let position = match ctx_inner.db.borrow_mut().list_pages(section_id) {
                        Ok(v) => v.len() as u32,
                        Err(_) => 0,
                    };
                    let now = Utc::now();
                    let page = Page {
                        id: PageId(Uuid::new_v4()),
                        section_id,
                        position,
                        template_id,
                        planner_address: None,
                        created_at: now,
                        modified_at: now,
                        name: String::new(),
                        widget_overrides: Default::default(),
                    };
                    if let Err(e) =
                        ctx_inner.db.borrow_mut().insert_page(&page)
                    {
                        tracing::error!("failed to insert page: {}", e);
                        return;
                    }
                    ctx_inner.refresh();
                    load_page(&ctx_inner.state, &page, &ctx_inner.canvas);
                }),
            );
        });
    }

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    header.add_css_class("section-row");

    // Inline-rename Stack (Label / Entry) for the section name. No popup.
    let name_stack = Stack::new();
    name_stack.set_hexpand(true);
    name_stack.set_valign(gtk4::Align::Center);
    let section_label = Label::builder()
        .label(&section.name)
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    section_label.add_css_class("section-header-label");
    let entry = Entry::builder()
        .text(&section.name)
        .hexpand(true)
        .build();
    entry.add_css_class("inline-rename");
    name_stack.add_named(&section_label, Some("label"));
    name_stack.add_named(&entry, Some("edit"));
    name_stack.set_visible_child_name("label");

    // Section settings gear is irrelevant for planner notebooks — pages are
    // auto-generated by date navigation, sections are auto-created wrappers,
    // and there's no per-section "allowed templates" choice for the user.
    let gear = Button::from_icon_name("emblem-system-symbolic");
    gear.set_tooltip_text(Some("Section settings"));
    gear.add_css_class("flat");
    gear.set_visible(!ctx.is_planner);
    {
        let ctx = ctx.clone();
        let sid = section.id;
        gear.connect_clicked(move |_| {
            let ctx_for_save = ctx.clone();
            crate::settings_dialogs::open_section_settings(
                &ctx.parent,
                ctx.state.clone(),
                sid,
                Box::new(move || ctx_for_save.refresh()),
            );
        });
    }
    header.append(&name_stack);
    header.append(&gear);
    expander.set_label_widget(Some(&header));

    // Double-click section label → enter inline-rename. Single-click is
    // claimed by the Expander itself (toggles expansion).
    {
        let click = GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        let name_stack_dbl = name_stack.clone();
        let entry_dbl = entry.clone();
        let label_dbl = section_label.clone();
        click.connect_pressed(move |g, n, _x, _y| {
            if n == 2 {
                entry_dbl.set_text(&label_dbl.text());
                name_stack_dbl.set_visible_child_name("edit");
                entry_dbl.grab_focus();
                entry_dbl.select_region(0, -1);
                g.set_state(gtk4::EventSequenceState::Claimed);
            }
        });
        section_label.add_controller(click);
    }

    let section_id_local = section.id;
    let commit: Rc<dyn Fn(bool)> = {
        let entry = entry.clone();
        let label = section_label.clone();
        let name_stack = name_stack.clone();
        let ctx = ctx.clone();
        Rc::new(move |save: bool| {
            if name_stack.visible_child_name().as_deref() != Some("edit") {
                return;
            }
            let new_text = entry.text().to_string();
            let trimmed = new_text.trim();
            if save && !trimmed.is_empty() && trimmed != label.text() {
                let current = match ctx.db.borrow_mut().get_section(section_id_local) {
                    Ok(s) => s,
                    Err(e) => { tracing::error!("rename section: {}", e); return; }
                };
                let mut updated = current;
                updated.name = trimmed.to_string();
                if let Err(e) = ctx.db.borrow_mut().update_section(&updated) {
                    tracing::error!("rename section: {}", e);
                } else {
                    label.set_text(trimmed);
                }
            } else {
                entry.set_text(&label.text());
            }
            name_stack.set_visible_child_name("label");
        })
    };
    // Enter on the Entry → commit (see page-row note for why connect_activate).
    {
        let commit = commit.clone();
        entry.connect_activate(move |_| (commit)(true));
    }
    // Esc → cancel.
    {
        let key = EventControllerKey::new();
        key.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let commit = commit.clone();
        key.connect_key_pressed(move |_c, kv, _, _| {
            if kv == gtk4::gdk::Key::Escape {
                (commit)(false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        entry.add_controller(key);
    }
    {
        let focus = EventControllerFocus::new();
        let commit = commit.clone();
        focus.connect_leave(move |_| (commit)(true));
        entry.add_controller(focus);
    }

    expander.set_child(Some(&inner));
    wrapper.append(&expander);

    // Section is draggable from anywhere on its header (no separate handle).
    attach_section_drag_source(&header, section.id);
    attach_section_drop_target(ctx, &wrapper, section.id, section.parent_section_id);

    wrapper
}

fn build_page_row(ctx: &SidebarCtx, page: &Page, list_index: u32) -> GtkBox {
    let label_text = if !page.name.is_empty() {
        page.name.clone()
    } else {
        format!("Page {}", page.position + 1)
    };
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .hexpand(true)
        .spacing(8)
        .build();
    row.add_css_class("page-row");
    row.set_cursor_from_name(Some("pointer"));

    let thumb = build_page_thumbnail(ctx, page);
    thumb.set_margin_top(2);
    thumb.set_margin_bottom(2);
    row.append(&thumb);

    // Inline-rename: a Stack that swaps between a Label (default) and an
    // Entry (double-click activates it). No modal popup.
    let name_stack = Stack::new();
    name_stack.set_hexpand(true);
    name_stack.set_valign(gtk4::Align::Center);
    let label = Label::builder()
        .label(&label_text)
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    let entry = Entry::builder()
        .text(&label_text)
        .hexpand(true)
        .build();
    entry.add_css_class("inline-rename");
    name_stack.add_named(&label, Some("label"));
    name_stack.add_named(&entry, Some("edit"));
    name_stack.set_visible_child_name("label");
    row.append(&name_stack);

    // Single click → load page; double-click → enter inline-rename mode.
    {
        let state = ctx.state.clone();
        let canvas = ctx.canvas.clone();
        let page_for_load = page.clone();
        let name_stack_dbl = name_stack.clone();
        let entry_dbl = entry.clone();
        let label_dbl = label.clone();
        let click = GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        click.connect_released(move |_g, n, _x, _y| {
            if n == 2 {
                entry_dbl.set_text(&label_dbl.text());
                name_stack_dbl.set_visible_child_name("edit");
                entry_dbl.grab_focus();
                entry_dbl.select_region(0, -1);
            } else if n == 1 {
                load_page(&state, &page_for_load, &canvas);
            }
        });
        row.add_controller(click);
    }

    // Commit/cancel inline rename: Enter commits, Esc cancels, focus-leave
    // commits whatever is currently in the entry.
    let commit: Rc<dyn Fn(bool)> = {
        let entry = entry.clone();
        let label = label.clone();
        let name_stack = name_stack.clone();
        let ctx = ctx.clone();
        let page = page.clone();
        Rc::new(move |save: bool| {
            if name_stack.visible_child_name().as_deref() != Some("edit") {
                return;
            }
            let new_text = entry.text().to_string();
            let trimmed = new_text.trim();
            if save && !trimmed.is_empty() && trimmed != label.text() {
                let mut updated = page.clone();
                updated.name = trimmed.to_string();
                updated.modified_at = Utc::now();
                if let Err(e) = ctx.db.borrow_mut().update_page(&updated) {
                    tracing::error!("rename page: {}", e);
                } else {
                    label.set_text(trimmed);
                }
            } else {
                entry.set_text(&label.text());
            }
            name_stack.set_visible_child_name("label");
        })
    };

    // Enter on the Entry → commit. Use the canonical `activate` signal
    // because GtkEntry consumes Return at the widget level before bubbling
    // it to an EventControllerKey, so connect_key_pressed never sees it.
    {
        let commit = commit.clone();
        entry.connect_activate(move |_| (commit)(true));
    }
    // Esc → cancel. Use Capture phase so the Entry's own handler doesn't
    // swallow it first.
    {
        let key = EventControllerKey::new();
        key.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let commit = commit.clone();
        key.connect_key_pressed(move |_c, kv, _, _| {
            if kv == gtk4::gdk::Key::Escape {
                (commit)(false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        entry.add_controller(key);
    }
    {
        let focus = EventControllerFocus::new();
        let commit = commit.clone();
        focus.connect_leave(move |_| (commit)(true));
        entry.add_controller(focus);
    }

    // Drag the entire row — no separate handle.
    attach_page_drag_source(&row, page.id, page.section_id);
    attach_page_drop_target(ctx, &row, page.id, page.section_id, list_index);

    // Right-click context menu: Duplicate / Delete (not on planner notebooks).
    if !ctx.is_planner {
        attach_page_context_menu(ctx, &row, page);
    }

    // Highlight the currently-loaded page so users see context at a glance.
    {
        let state = ctx.state.clone();
        let row_for_tick = row.clone();
        let page_id = page.id;
        row.add_tick_callback(move |_, _| {
            let active = state.borrow().current_page_id == Some(page_id);
            if active {
                row_for_tick.add_css_class("current");
            } else {
                row_for_tick.remove_css_class("current");
            }
            glib::ControlFlow::Continue
        });
    }

    row
}

fn attach_page_context_menu(ctx: &SidebarCtx, row: &GtkBox, page: &Page) {
    // Build the GMenu model.
    let menu = gio::Menu::new();
    menu.append(Some("Duplicate page"), Some("page-ctx.duplicate"));
    menu.append(Some("Delete page"), Some("page-ctx.delete"));

    let popover = PopoverMenu::from_model(Some(&menu));
    popover.set_parent(row);
    popover.set_has_arrow(false);

    // Wire up SimpleAction group on the row.
    let action_group = gio::SimpleActionGroup::new();

    // --- Duplicate action ---
    {
        let ctx = ctx.clone();
        let page = page.clone();
        let canvas = ctx.canvas.clone();
        let dup_action = gio::SimpleAction::new("duplicate", None);
        dup_action.connect_activate(move |_, _| {
            duplicate_page(&ctx, &page, &canvas);
        });
        action_group.add_action(&dup_action);
    }

    // --- Delete action ---
    {
        let ctx = ctx.clone();
        let page = page.clone();
        let parent_win = ctx.parent.clone();
        let del_action = gio::SimpleAction::new("delete", None);
        del_action.connect_activate(move |_, _| {
            let win = Window::builder()
                .transient_for(&parent_win)
                .modal(true)
                .title("Delete page?")
                .default_width(320)
                .build();
            let body = GtkBox::builder()
                .orientation(Orientation::Vertical)
                .spacing(12)
                .margin_top(16)
                .margin_bottom(16)
                .margin_start(16)
                .margin_end(16)
                .build();
            let msg = Label::new(Some(
                "This will permanently delete the page and all its strokes.",
            ));
            msg.set_wrap(true);
            body.append(&msg);
            let btn_row = GtkBox::builder()
                .orientation(Orientation::Horizontal)
                .spacing(8)
                .halign(gtk4::Align::End)
                .build();
            let cancel_btn = Button::with_label("Cancel");
            let delete_btn = Button::with_label("Delete");
            delete_btn.add_css_class("destructive-action");
            btn_row.append(&cancel_btn);
            btn_row.append(&delete_btn);
            body.append(&btn_row);
            win.set_child(Some(&body));
            {
                let win = win.clone();
                cancel_btn.connect_clicked(move |_| win.close());
            }
            {
                let win = win.clone();
                let ctx = ctx.clone();
                let page = page.clone();
                delete_btn.connect_clicked(move |_| {
                    if let Err(e) = ctx.db.borrow_mut().delete_page(page.id) {
                        tracing::error!("delete page {:?}: {}", page.id, e);
                    } else {
                        ctx.refresh();
                    }
                    win.close();
                });
            }
            win.present();
        });
        action_group.add_action(&del_action);
    }

    row.insert_action_group("page-ctx", Some(&action_group));

    // Secondary-button click → position the popover under the cursor and pop it.
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
    row.add_controller(right_click);
}

fn duplicate_page(ctx: &SidebarCtx, page: &Page, canvas: &DrawingArea) {
    let new_name = if page.name.is_empty() {
        String::new()
    } else {
        format!("{} (copy)", page.name)
    };
    let now = Utc::now();
    let new_id = PageId(Uuid::new_v4());
    let new_page = Page {
        id: new_id,
        section_id: page.section_id,
        position: page.position + 1,
        template_id: page.template_id,
        planner_address: None,
        created_at: now,
        modified_at: now,
        name: new_name,
        widget_overrides: page.widget_overrides.clone(),
    };

    {
        let mut db = ctx.db.borrow_mut();
        if let Err(e) = db.insert_page(&new_page) {
            tracing::error!("duplicate page insert: {}", e);
            return;
        }
        let strokes = match db.list_strokes_for_page(page.id) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("duplicate: list strokes: {}", e);
                vec![]
            }
        };
        for mut s in strokes {
            s.id = Uuid::new_v4();
            if let Err(e) = db.insert_stroke(&s, new_id) {
                tracing::error!("duplicate stroke insert: {}", e);
            }
        }
        if let Err(e) = db.reorder_page(new_id, page.position + 1) {
            tracing::warn!("duplicate reorder: {}", e);
        }
    }

    ctx.refresh();
    load_page(&ctx.state, &new_page, canvas);
}

fn build_page_thumbnail(ctx: &SidebarCtx, page: &Page) -> GtkDrawingArea {
    let thumb_area = GtkDrawingArea::builder()
        .width_request(crate::thumbnail::THUMB_W)
        .height_request(crate::thumbnail::THUMB_H)
        .build();

    let state = ctx.state.clone();
    let page_id = page.id;
    let template_id = page.template_id;

    {
        let state = state.clone();
        thumb_area.set_draw_func(move |_area, ctx_cairo, _w, _h| {
            let dark_mode = state.borrow().dark_mode;
            let template = template_id.and_then(|tid| {
                state.borrow().templates.borrow().get(tid).cloned()
            });

            if let Some(surface) = crate::thumbnail::get_or_generate_thumbnail(
                &state,
                page_id,
                template.as_ref(),
                dark_mode,
            ) {
                let mut s = state.borrow_mut();
                s.thumbnail_cache.insert(page_id, surface);
            }

            let has_surface = state.borrow().thumbnail_cache.contains_key(&page_id);
            if has_surface {
                let s = state.borrow();
                if let Some(surface) = s.thumbnail_cache.get(&page_id) {
                    ctx_cairo.save().ok();
                    ctx_cairo.set_source_surface(surface, 0.0, 0.0).ok();
                    ctx_cairo.paint().ok();
                    ctx_cairo.restore().ok();

                    ctx_cairo.set_source_rgba(0.5, 0.5, 0.5, 0.4);
                    ctx_cairo.set_line_width(0.5);
                    ctx_cairo.rectangle(0.0, 0.0, crate::thumbnail::THUMB_W as f64, crate::thumbnail::THUMB_H as f64);
                    let _ = ctx_cairo.stroke();
                }
            } else {
                ctx_cairo.set_source_rgba(0.85, 0.85, 0.9, 1.0);
                ctx_cairo.rectangle(0.0, 0.0, 40.0, 52.0);
                let _ = ctx_cairo.fill();
                ctx_cairo.set_source_rgba(0.5, 0.5, 0.5, 0.4);
                ctx_cairo.set_line_width(0.5);
                ctx_cairo.rectangle(0.0, 0.0, 40.0, 52.0);
                let _ = ctx_cairo.stroke();
            }
        });
    }

    thumb_area
}

const PAGE_DRAG_PREFIX: &str = "page:";
const SECTION_DRAG_PREFIX: &str = "section:";

fn attach_page_drag_source(handle: &GtkBox, page_id: PageId, section_id: SectionId) {
    let source = DragSource::new();
    source.set_actions(DragAction::MOVE);
    source.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let payload = format!("{}{}:{}", PAGE_DRAG_PREFIX, section_id.0, page_id.0);
    source.connect_prepare(move |_, _, _| {
        let value = payload.clone().to_value();
        Some(ContentProvider::for_value(&value))
    });
    handle.add_controller(source);
}

fn attach_page_drop_target(
    ctx: &SidebarCtx,
    row: &GtkBox,
    target_page_id: PageId,
    target_section_id: SectionId,
    target_index: u32,
) {
    let target = DropTarget::new(glib::types::Type::STRING, DragAction::MOVE);
    {
        let row_enter = row.clone();
        target.connect_enter(move |_, _, _| {
            row_enter.add_css_class("drag-target");
            DragAction::MOVE
        });
    }
    {
        let row_leave = row.clone();
        target.connect_leave(move |_| {
            row_leave.remove_css_class("drag-target");
        });
    }
    let ctx = ctx.clone();
    let row_drop = row.clone();
    target.connect_drop(move |_, value, _x, _y| {
        row_drop.remove_css_class("drag-target");
        let payload = match value.get::<String>() {
            Ok(s) => s,
            Err(_) => return false,
        };
        let Some(rest) = payload.strip_prefix(PAGE_DRAG_PREFIX) else {
            return false;
        };
        let Some((src_section_str, src_page_str)) = rest.split_once(':') else {
            return false;
        };
        let Ok(src_section_uuid) = Uuid::parse_str(src_section_str) else {
            return false;
        };
        let Ok(src_page_uuid) = Uuid::parse_str(src_page_str) else {
            return false;
        };
        let src_section_id = SectionId(src_section_uuid);
        let src_id = PageId(src_page_uuid);
        if src_id == target_page_id {
            return false;
        }
        let result = if src_section_id == target_section_id {
            ctx.db.borrow_mut().reorder_page(src_id, target_index)
        } else {
            ctx.db.borrow_mut().move_page(src_id, target_section_id, target_index)
        };
        if let Err(e) = result {
            tracing::error!("failed to move/reorder page: {}", e);
            return false;
        }
        ctx.refresh();
        true
    });
    row.add_controller(target);
}

fn attach_section_drag_source(handle: &GtkBox, section_id: SectionId) {
    let source = DragSource::new();
    source.set_actions(DragAction::MOVE);
    source.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let payload = format!("{}{}", SECTION_DRAG_PREFIX, section_id.0);
    source.connect_prepare(move |_, _, _| {
        let value = payload.clone().to_value();
        Some(ContentProvider::for_value(&value))
    });
    handle.add_controller(source);
}

fn attach_section_drop_target(
    ctx: &SidebarCtx,
    wrapper: &GtkBox,
    target_section_id: SectionId,
    target_parent_id: Option<SectionId>,
) {
    let target = DropTarget::new(glib::types::Type::STRING, DragAction::MOVE);
    {
        let wrapper_enter = wrapper.clone();
        target.connect_enter(move |_, _, _| {
            wrapper_enter.add_css_class("drag-target");
            DragAction::MOVE
        });
    }
    {
        let wrapper_leave = wrapper.clone();
        target.connect_leave(move |_| {
            wrapper_leave.remove_css_class("drag-target");
        });
    }
    let ctx = ctx.clone();
    let wrapper_drop = wrapper.clone();
    target.connect_drop(move |_, value, _x, _y| {
        wrapper_drop.remove_css_class("drag-target");
        let payload = match value.get::<String>() {
            Ok(s) => s,
            Err(_) => return false,
        };
        if let Some(rest) = payload.strip_prefix(PAGE_DRAG_PREFIX) {
            let Some((src_section_str, src_page_str)) = rest.split_once(':') else {
                return false;
            };
            let Ok(src_section_uuid) = Uuid::parse_str(src_section_str) else {
                return false;
            };
            let Ok(src_page_uuid) = Uuid::parse_str(src_page_str) else {
                return false;
            };
            let src_section_id = SectionId(src_section_uuid);
            let src_id = PageId(src_page_uuid);
            if src_section_id == target_section_id {
                return false;
            }
            let n = ctx.db.borrow_mut()
                .list_pages(target_section_id)
                .map(|v| v.len() as u32)
                .unwrap_or(0);
            if let Err(e) = ctx.db.borrow_mut().move_page(src_id, target_section_id, n) {
                tracing::error!("failed to move page across sections: {}", e);
                return false;
            }
            ctx.refresh();
            return true;
        }
        let Some(uuid_str) = payload.strip_prefix(SECTION_DRAG_PREFIX) else {
            return false;
        };
        let Ok(src_uuid) = Uuid::parse_str(uuid_str) else {
            return false;
        };
        let src_id = SectionId(src_uuid);
        if src_id == target_section_id {
            return false;
        }
        // Cross-parent moves are out of scope; reject them with a log so the
        // drop appears as a no-op at the UI level.
        let src_parent = match ctx.db.borrow_mut().get_section(src_id) {
            Ok(s) => s.parent_section_id,
            Err(e) => {
                tracing::error!("failed to load drag source section: {}", e);
                return false;
            }
        };
        if src_parent != target_parent_id {
            tracing::info!(
                "rejected cross-parent section move src={:?} src_parent={:?} target_parent={:?}",
                src_id,
                src_parent,
                target_parent_id
            );
            return false;
        }
        // Find the target's position within its sibling group.
        let new_pos = match ctx.db.borrow_mut().get_section(target_section_id) {
            Ok(s) => s.position,
            Err(e) => {
                tracing::error!("failed to read target section position: {}", e);
                return false;
            }
        };
        if let Err(e) = ctx.db.borrow_mut().reorder_section(src_id, new_pos) {
            tracing::error!("failed to reorder section: {}", e);
            return false;
        }
        ctx.refresh();
        true
    });
    wrapper.add_controller(target);
}

fn load_page(state: &SharedState, page: &Page, canvas: &DrawingArea) {
    let template = match page.template_id {
        Some(tid) => state.borrow().templates.borrow().get(tid).cloned(),
        None => None,
    };
    state::set_current_template(state, template);
    state::set_current_page(state, page.id);

    // If this is a planner Day page and the planner nav strip is installed,
    // sync the strip's "current date" so prev/next walk from this page.
    if let Some(journal_core::CalendarPageAddress::Day { date, .. }) = page.planner_address {
        let sync = state.borrow().planner_nav_sync_date.clone();
        if let Some(sync) = sync {
            (sync)(date);
        }
    }

    canvas.queue_draw();
}
