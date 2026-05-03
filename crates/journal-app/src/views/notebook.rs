use std::cell::RefCell;
use std::rc::Rc;

use chrono::Utc;
use gtk4::gdk::{ContentProvider, DragAction, Rectangle};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, DrawingArea as GtkDrawingArea, DragSource,
    DrawingArea, DropTarget, Expander, GestureClick, GestureLongPress, Image, Label, Orientation,
    Overlay, Paned, PopoverMenu, ScrolledWindow,
};
use journal_core::{NotebookId, Page, PageId, Section, SectionId};
use journal_storage::{page_store, section_store, Db};
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
    db: Rc<RefCell<Db>>,
    state: SharedState,
    notebook_id: NotebookId,
    canvas: DrawingArea,
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

    let new_section_btn = Button::with_label("+ New Section");
    new_section_btn.set_margin_start(8);
    new_section_btn.set_margin_end(8);
    new_section_btn.set_margin_top(4);
    new_section_btn.set_margin_bottom(8);
    sidebar_root.append(&new_section_btn);

    paned.set_start_child(Some(&sidebar_root));
    paned.set_end_child(Some(&canvas_pane));
    paned.set_resize_start_child(false);
    paned.set_shrink_start_child(false);

    let db = state.borrow().db.clone();
    let sections_box_rc = Rc::new(sections_box);

    let ctx = SidebarCtx {
        parent: parent.clone(),
        sections_box: sections_box_rc.clone(),
        db: db.clone(),
        state: state.clone(),
        notebook_id,
        canvas: canvas.clone(),
    };

    ctx.refresh();

    {
        let ctx = ctx.clone();
        new_section_btn.connect_clicked(move |_| {
            let ctx_inner = ctx.clone();
            dialogs::prompt_new_section(
                &ctx.parent,
                Box::new(move |name| {
                    let position = match section_store::list_sections(
                        ctx_inner.db.borrow().conn(),
                        ctx_inner.notebook_id,
                    ) {
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
                        section_store::insert_section(ctx_inner.db.borrow().conn(), &section)
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

    NotebookView { root }
}

fn refresh_sections(
    parent: &ApplicationWindow,
    sections_box: &Rc<GtkBox>,
    db: Rc<RefCell<Db>>,
    state: SharedState,
    notebook_id: NotebookId,
    canvas: DrawingArea,
) {
    while let Some(child) = sections_box.first_child() {
        sections_box.remove(&child);
    }

    let roots = match section_store::list_root_sections(db.borrow().conn(), notebook_id) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("failed to list root sections: {}", e);
            return;
        }
    };

    if roots.is_empty() {
        let empty = Label::new(Some("No sections — add one below."));
        empty.add_css_class("dim-label");
        empty.set_halign(gtk4::Align::Start);
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

    let pages = match page_store::list_pages(ctx.db.borrow().conn(), section.id) {
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
    let child_sections = match section_store::list_child_sections(ctx.db.borrow().conn(), section.id) {
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
                    let position = match page_store::list_pages(
                        ctx_inner.db.borrow().conn(),
                        section_id,
                    ) {
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
                    };
                    if let Err(e) =
                        page_store::insert_page(ctx_inner.db.borrow().conn(), &page)
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
        .spacing(4)
        .hexpand(true)
        .build();
    let section_handle = drag_handle_box();
    let section_label = Label::builder()
        .label(&section.name)
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    section_label.add_css_class("section-header-label");
    let gear = Button::from_icon_name("emblem-system-symbolic");
    gear.set_tooltip_text(Some("Section settings"));
    gear.add_css_class("flat");
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
    header.append(&section_handle);
    header.append(&section_label);
    header.append(&gear);
    expander.set_label_widget(Some(&header));

    expander.set_child(Some(&inner));
    wrapper.append(&expander);

    attach_section_context_menu(ctx, &section_label, &section);
    attach_section_drag_source(&section_handle, section.id);
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
        .spacing(4)
        .build();
    row.add_css_class("page-row");

    let handle = drag_handle_box();
    row.append(&handle);

    let thumb = build_page_thumbnail(ctx, page);
    row.append(&thumb);

    let label = Label::builder()
        .label(&label_text)
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .margin_top(4)
        .margin_bottom(4)
        .margin_end(8)
        .build();
    row.append(&label);

    {
        let state = ctx.state.clone();
        let canvas = ctx.canvas.clone();
        let page = page.clone();
        let click = GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        click.connect_released(move |_g, n, _x, _y| {
            if n == 1 {
                load_page(&state, &page, &canvas);
            }
        });
        row.add_controller(click);
    }

    attach_page_context_menu(ctx, &label, page);
    attach_page_drag_source(&handle, page.id, page.section_id);
    attach_page_drop_target(ctx, &row, page.id, page.section_id, list_index);

    row
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

fn drag_handle_box() -> GtkBox {
    // Touch-friendly hit area (≥44px tall, 36px wide) so a finger or stylus
    // can grab it reliably; the visible icon stays compact via centring.
    let wrap = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .width_request(36)
        .height_request(44)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .build();
    wrap.add_css_class("drag-handle");
    let img = Image::from_icon_name("list-drag-handle-symbolic");
    img.set_icon_size(gtk4::IconSize::Normal);
    img.set_halign(gtk4::Align::Center);
    img.set_valign(gtk4::Align::Center);
    img.add_css_class("dim-label");
    wrap.append(&img);
    wrap.set_tooltip_text(Some("Drag to reorder"));
    wrap.set_cursor_from_name(Some("grab"));
    wrap
}

fn make_rename_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Rename"), Some("row.rename"));
    menu
}

fn show_popover_at<W: IsA<gtk4::Widget>>(parent: &W, x: f64, y: f64) {
    let popover = PopoverMenu::from_model(Some(&make_rename_menu()));
    popover.set_parent(parent);
    popover.set_has_arrow(false);
    let rect = Rectangle::new(x as i32, y as i32, 1, 1);
    popover.set_pointing_to(Some(&rect));
    popover.connect_closed(|p| p.unparent());
    popover.popup();
}

fn attach_page_context_menu(ctx: &SidebarCtx, row: &Label, page: &Page) {
    let ctx_outer = ctx.clone();
    let page_outer = page.clone();
    let open_rename = move || {
        let ctx_inner = ctx_outer.clone();
        let page_inner = page_outer.clone();
        dialogs::prompt_rename(
            &ctx_outer.parent,
            "Rename Page",
            &page_outer.name,
            Box::new(move |new_name| {
                let mut updated = page_inner.clone();
                updated.name = new_name;
                updated.modified_at = Utc::now();
                if let Err(e) = page_store::update_page(ctx_inner.db.borrow().conn(), &updated)
                {
                    tracing::error!("failed to rename page: {}", e);
                    return;
                }
                ctx_inner.refresh();
            }),
        );
    };
    let open_rename = Rc::new(open_rename);

    let long_press = GestureLongPress::new();
    long_press.set_touch_only(false);
    long_press.set_delay_factor(0.6);
    {
        let open = open_rename.clone();
        long_press.connect_pressed(move |_g, _x, _y| (open)());
    }
    row.add_controller(long_press);

    let dbl = GestureClick::new();
    dbl.set_button(gtk4::gdk::BUTTON_PRIMARY);
    {
        let open = open_rename.clone();
        dbl.connect_pressed(move |_g, n, _x, _y| {
            if n == 2 {
                (open)();
            }
        });
    }
    row.add_controller(dbl);
}

fn attach_section_context_menu(ctx: &SidebarCtx, wrapper: &Label, section: &Section) {
    let action_group = gio::SimpleActionGroup::new();
    let rename_action = gio::SimpleAction::new("rename", None);
    {
        let ctx = ctx.clone();
        let section = section.clone();
        rename_action.connect_activate(move |_, _| {
            let ctx_inner = ctx.clone();
            let section_inner = section.clone();
            dialogs::prompt_rename(
                &ctx.parent,
                "Rename Section",
                &section.name,
                Box::new(move |new_name| {
                    if new_name.trim().is_empty() {
                        return;
                    }
                    let mut updated = section_inner.clone();
                    updated.name = new_name;
                    if let Err(e) =
                        section_store::update_section(ctx_inner.db.borrow().conn(), &updated)
                    {
                        tracing::error!("failed to rename section: {}", e);
                        return;
                    }
                    ctx_inner.refresh();
                }),
            );
        });
    }
    action_group.add_action(&rename_action);
    wrapper.insert_action_group("row", Some(&action_group));

    let click = GestureClick::new();
    click.set_button(gtk4::gdk::BUTTON_SECONDARY);
    click.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let wrapper = wrapper.clone();
        click.connect_pressed(move |_g, _n, x, y| {
            show_popover_at(&wrapper, x, y);
        });
    }
    wrapper.add_controller(click);

    let long_press = GestureLongPress::new();
    long_press.set_touch_only(false);
    long_press.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let wrapper = wrapper.clone();
        long_press.connect_pressed(move |_g, x, y| {
            show_popover_at(&wrapper, x, y);
        });
    }
    wrapper.add_controller(long_press);
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
        let mut db = ctx.db.borrow_mut();
        let result = if src_section_id == target_section_id {
            page_store::reorder_page(db.conn_mut(), src_id, target_index)
        } else {
            page_store::move_page(db.conn_mut(), src_id, target_section_id, target_index)
        };
        if let Err(e) = result {
            tracing::error!("failed to move/reorder page: {}", e);
            return false;
        }
        drop(db);
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
            let mut db = ctx.db.borrow_mut();
            let n = page_store::list_pages(db.conn(), target_section_id)
                .map(|v| v.len() as u32)
                .unwrap_or(0);
            if let Err(e) = page_store::move_page(db.conn_mut(), src_id, target_section_id, n) {
                tracing::error!("failed to move page across sections: {}", e);
                return false;
            }
            drop(db);
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
        let src_parent = match section_store::get_section(ctx.db.borrow().conn(), src_id) {
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
        let new_pos = match section_store::get_section(ctx.db.borrow().conn(), target_section_id) {
            Ok(s) => s.position,
            Err(e) => {
                tracing::error!("failed to read target section position: {}", e);
                return false;
            }
        };
        let mut db = ctx.db.borrow_mut();
        if let Err(e) = section_store::reorder_section(db.conn_mut(), src_id, new_pos) {
            tracing::error!("failed to reorder section: {}", e);
            return false;
        }
        drop(db);
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
    canvas.queue_draw();
}
