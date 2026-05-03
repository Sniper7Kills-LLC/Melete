use std::cell::RefCell;
use std::rc::Rc;

use gtk4::cairo;
use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, DrawingArea, Entry, Label, Orientation,
    Paned, ScrolledWindow, Window,
};
use journal_canvas::{draw_widgets, ViewportTransform};
use journal_core::{
    PageTemplate, Rect, TemplateWidget, WidgetKind, WidgetRect,
    WidgetStyle,
};
use journal_templates::{serialize_template_toml, template_file_from_page_template};
use uuid::Uuid;

use crate::state::SharedState;

#[derive(Debug, Clone, Copy, PartialEq)]
enum PlaceTool {
    None,
    TextBlock,
    Rectangle,
    Ellipse,
    Line,
    GridRegion,
    LinesRegion,
    DotsRegion,
    CalendarMonth,
    Timeline,
    Checklist,
    BigThree,
    PriorityList,
    DailyAppointments,
    WeeklyCompass,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Handle {
    Move,
    ResizeBottomRight,
}

struct CreatorState {
    template: PageTemplate,
    selected_idx: Option<usize>,
    tool: PlaceTool,
    drag_start_canvas: Option<(f64, f64)>,
    drag_active: bool,
    drag_handle: Handle,
    drag_orig_rect: Option<WidgetRect>,
}

impl CreatorState {
    fn new(template: PageTemplate) -> Self {
        Self {
            template,
            selected_idx: None,
            tool: PlaceTool::None,
            drag_start_canvas: None,
            drag_active: false,
            drag_handle: Handle::Move,
            drag_orig_rect: None,
        }
    }
}

pub fn open(
    parent: &ApplicationWindow,
    state: SharedState,
    edit: Option<PageTemplate>,
    on_save: impl Fn() + 'static,
) {
    let template = edit.unwrap_or_else(PageTemplate::default);

    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Template Creator")
        .default_width(1000)
        .default_height(700)
        .build();

    let cs = Rc::new(RefCell::new(CreatorState::new(template)));

    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .build();

    let meta_row = build_meta_row(&cs);
    root.append(&meta_row);

    let palette = build_palette(&cs);
    let canvas_area = build_canvas_area(&cs);
    let (props_scroll, _props_box) = build_props_panel();

    let inner_paned = Paned::new(Orientation::Horizontal);
    inner_paned.set_start_child(Some(&canvas_area));
    inner_paned.set_end_child(Some(&props_scroll));
    inner_paned.set_position(620);

    let paned = Paned::new(Orientation::Horizontal);
    paned.set_start_child(Some(&palette));
    paned.set_end_child(Some(&inner_paned));
    paned.set_position(140);

    root.append(&paned);

    let action_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(gtk4::Align::End)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(8)
        .margin_end(12)
        .build();
    let cancel_btn = Button::with_label("Cancel");
    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    action_row.append(&cancel_btn);
    action_row.append(&save_btn);
    root.append(&action_row);

    win.set_child(Some(&root));

    cancel_btn.connect_clicked({
        let win = win.clone();
        move |_| win.close()
    });

    save_btn.connect_clicked({
        let cs = cs.clone();
        let state = state.clone();
        let win = win.clone();
        move |_| {
            let cs_ref = cs.borrow();
            let t = cs_ref.template.clone();
            drop(cs_ref);
            if let Err(e) = save_template(&t, &state) {
                tracing::error!("save template: {:#}", e);
            } else {
                on_save();
                win.close();
            }
        }
    });

    win.present();
}

fn build_meta_row(cs: &Rc<RefCell<CreatorState>>) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();

    let name_label = Label::new(Some("Name:"));
    row.append(&name_label);
    let name_entry = Entry::builder().placeholder_text("Template name").hexpand(true).build();
    {
        let t = cs.borrow();
        name_entry.set_text(&t.template.name);
    }
    name_entry.connect_changed({
        let cs = cs.clone();
        move |e| {
            cs.borrow_mut().template.name = e.text().to_string();
        }
    });
    row.append(&name_entry);

    let desc_label = Label::new(Some("Description:"));
    row.append(&desc_label);
    let desc_entry = Entry::builder().placeholder_text("Optional description").hexpand(true).build();
    {
        let t = cs.borrow();
        desc_entry.set_text(&t.template.description);
    }
    desc_entry.connect_changed({
        let cs = cs.clone();
        move |e| {
            cs.borrow_mut().template.description = e.text().to_string();
        }
    });
    row.append(&desc_entry);

    row
}

fn build_palette(cs: &Rc<RefCell<CreatorState>>) -> ScrolledWindow {
    let scroller = ScrolledWindow::builder()
        .width_request(140)
        .vexpand(true)
        .build();

    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let label = Label::builder().label("Widgets").halign(gtk4::Align::Start).build();
    label.add_css_class("title-4");
    vbox.append(&label);

    let tools: &[(&str, PlaceTool)] = &[
        ("Text", PlaceTool::TextBlock),
        ("Rectangle", PlaceTool::Rectangle),
        ("Ellipse", PlaceTool::Ellipse),
        ("Line", PlaceTool::Line),
        ("Grid", PlaceTool::GridRegion),
        ("Lines", PlaceTool::LinesRegion),
        ("Dots", PlaceTool::DotsRegion),
        ("Calendar", PlaceTool::CalendarMonth),
        ("Timeline", PlaceTool::Timeline),
        ("Checklist", PlaceTool::Checklist),
        ("Big Three", PlaceTool::BigThree),
        ("Priority List", PlaceTool::PriorityList),
        ("Day Schedule", PlaceTool::DailyAppointments),
        ("Weekly Compass", PlaceTool::WeeklyCompass),
    ];

    for (label_text, tool) in tools {
        let btn = Button::with_label(label_text);
        let cs2 = cs.clone();
        let t = *tool;
        btn.connect_clicked(move |_| {
            cs2.borrow_mut().tool = t;
            cs2.borrow_mut().selected_idx = None;
        });
        vbox.append(&btn);
    }

    let desel_btn = Button::with_label("Select/Move");
    desel_btn.connect_clicked({
        let cs = cs.clone();
        move |_| {
            cs.borrow_mut().tool = PlaceTool::None;
        }
    });
    vbox.prepend(&desel_btn);

    let del_btn = Button::with_label("Delete widget");
    del_btn.add_css_class("destructive-action");
    del_btn.connect_clicked({
        let cs = cs.clone();
        move |_| {
            let mut s = cs.borrow_mut();
            if let Some(idx) = s.selected_idx {
                if idx < s.template.widgets.len() {
                    s.template.widgets.remove(idx);
                }
                s.selected_idx = None;
            }
        }
    });
    vbox.append(&del_btn);

    scroller.set_child(Some(&vbox));
    scroller
}

fn build_canvas_area(cs: &Rc<RefCell<CreatorState>>) -> DrawingArea {
    let area = DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();

    area.set_draw_func({
        let cs = cs.clone();
        move |_area, ctx, w, h| {
            draw_creator_canvas(ctx, w as f64, h as f64, &cs.borrow());
        }
    });

    let drag = gtk4::GestureDrag::new();
    drag.connect_drag_begin({
        let cs = cs.clone();
        let area = area.clone();
        move |gesture, x, y| {
            let size = get_area_size(&area);
            let canvas_pt = screen_to_template(x, y, size, &cs.borrow().template);
            let mut s = cs.borrow_mut();
            if s.tool != PlaceTool::None {
                s.drag_start_canvas = Some(canvas_pt);
                s.drag_active = false;
            } else {
                let hit = hit_test(&s.template.widgets, canvas_pt);
                if let Some(idx) = hit {
                    let handle = resize_handle_hit(&s.template.widgets[idx].rect, canvas_pt);
                    s.selected_idx = Some(idx);
                    s.drag_start_canvas = Some(canvas_pt);
                    s.drag_handle = handle;
                    s.drag_orig_rect = Some(s.template.widgets[idx].rect.clone());
                    s.drag_active = true;
                } else {
                    s.selected_idx = None;
                    s.drag_start_canvas = None;
                }
                gesture.set_state(gtk4::EventSequenceState::Claimed);
            }
        }
    });

    drag.connect_drag_update({
        let cs = cs.clone();
        let area = area.clone();
        move |_, dx, dy| {
            let size = get_area_size(&area);
            let scale = template_scale(size, &cs.borrow().template);
            let (dcx, dcy) = (dx / scale, dy / scale);
            let mut s = cs.borrow_mut();
            if s.tool != PlaceTool::None {
                s.drag_active = true;
            } else if s.drag_active {
                if let (Some(orig), Some(idx)) = (s.drag_orig_rect.clone(), s.selected_idx) {
                    if idx < s.template.widgets.len() {
                        match s.drag_handle {
                            Handle::Move => {
                                s.template.widgets[idx].rect.x = orig.x + dcx;
                                s.template.widgets[idx].rect.y = orig.y + dcy;
                            }
                            Handle::ResizeBottomRight => {
                                let new_w = (orig.width + dcx).max(2.0);
                                let new_h = (orig.height + dcy).max(2.0);
                                s.template.widgets[idx].rect.width = new_w;
                                s.template.widgets[idx].rect.height = new_h;
                            }
                        }
                    }
                }
            }
            drop(s);
            area.queue_draw();
        }
    });

    drag.connect_drag_end({
        let cs = cs.clone();
        let area = area.clone();
        move |_, dx, dy| {
            let size = get_area_size(&area);
            let canvas_start = {
                let s = cs.borrow();
                s.drag_start_canvas
            };
            let Some(start) = canvas_start else { return };
            let scale = template_scale(size, &cs.borrow().template);
            let end = (start.0 + dx / scale, start.1 + dy / scale);

            let mut s = cs.borrow_mut();
            if s.tool != PlaceTool::None && s.drag_active {
                let rx = start.0.min(end.0);
                let ry = start.1.min(end.1);
                let rw = (end.0 - start.0).abs().max(2.0);
                let rh = (end.1 - start.1).abs().max(2.0);
                let kind = default_kind_for(s.tool);
                let widget = TemplateWidget {
                    id: Uuid::new_v4(),
                    kind,
                    rect: WidgetRect { x: rx, y: ry, width: rw, height: rh },
                    style: WidgetStyle::default(),
                };
                s.template.widgets.push(widget);
                s.selected_idx = Some(s.template.widgets.len() - 1);
                s.tool = PlaceTool::None;
            }
            s.drag_start_canvas = None;
            s.drag_active = false;
            s.drag_orig_rect = None;
            drop(s);
            area.queue_draw();
        }
    });

    area.add_controller(drag);

    let key = gtk4::EventControllerKey::new();
    key.connect_key_pressed({
        let cs = cs.clone();
        let area = area.clone();
        move |_, key, _, _| {
            if key == gtk4::gdk::Key::Delete || key == gtk4::gdk::Key::BackSpace {
                let mut s = cs.borrow_mut();
                if let Some(idx) = s.selected_idx {
                    if idx < s.template.widgets.len() {
                        s.template.widgets.remove(idx);
                    }
                    s.selected_idx = None;
                    drop(s);
                    area.queue_draw();
                }
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        }
    });
    area.set_focusable(true);
    area.add_controller(key);

    area
}

fn build_props_panel() -> (ScrolledWindow, GtkBox) {
    let scroller = ScrolledWindow::builder()
        .width_request(200)
        .vexpand(true)
        .build();
    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_start(8)
        .margin_end(8)
        .build();
    let label = Label::builder().label("Properties").halign(gtk4::Align::Start).build();
    label.add_css_class("title-4");
    vbox.append(&label);
    scroller.set_child(Some(&vbox));
    (scroller, vbox)
}

fn get_area_size(area: &DrawingArea) -> (f64, f64) {
    (area.width() as f64, area.height() as f64)
}

fn template_scale(screen_size: (f64, f64), template: &PageTemplate) -> f64 {
    let margin = 0.9;
    let (sw, sh) = screen_size;
    let (tw, th) = template.size_mm;
    if sw <= 0.0 || sh <= 0.0 || tw <= 0.0 || th <= 0.0 {
        return 1.0;
    }
    (sw / tw).min(sh / th) * margin
}

fn template_origin(screen_size: (f64, f64), template: &PageTemplate) -> (f64, f64) {
    let scale = template_scale(screen_size, template);
    let (sw, sh) = screen_size;
    let (tw, th) = template.size_mm;
    let ox = (sw - tw * scale) * 0.5;
    let oy = (sh - th * scale) * 0.5;
    (ox, oy)
}

fn screen_to_template(sx: f64, sy: f64, size: (f64, f64), template: &PageTemplate) -> (f64, f64) {
    let scale = template_scale(size, template);
    let (ox, oy) = template_origin(size, template);
    ((sx - ox) / scale, (sy - oy) / scale)
}

fn hit_test(widgets: &[TemplateWidget], pt: (f64, f64)) -> Option<usize> {
    for (i, w) in widgets.iter().enumerate().rev() {
        let r = &w.rect;
        if pt.0 >= r.x && pt.0 <= r.x + r.width && pt.1 >= r.y && pt.1 <= r.y + r.height {
            return Some(i);
        }
    }
    None
}

fn resize_handle_hit(r: &WidgetRect, pt: (f64, f64)) -> Handle {
    let margin = 8.0;
    let bx = r.x + r.width;
    let by = r.y + r.height;
    if (pt.0 - bx).abs() < margin && (pt.1 - by).abs() < margin {
        Handle::ResizeBottomRight
    } else {
        Handle::Move
    }
}

fn default_kind_for(tool: PlaceTool) -> WidgetKind {
    match tool {
        PlaceTool::TextBlock => WidgetKind::TextBlock { text: "Text".into(), font_size_mm: 5.0 },
        PlaceTool::Rectangle => WidgetKind::Rectangle,
        PlaceTool::Ellipse => WidgetKind::Ellipse,
        PlaceTool::Line => WidgetKind::Line { thickness_mm: 0.5 },
        PlaceTool::GridRegion => WidgetKind::GridRegion { spacing_mm: 5.0 },
        PlaceTool::LinesRegion => WidgetKind::LinesRegion { spacing_mm: 7.0 },
        PlaceTool::DotsRegion => WidgetKind::DotsRegion { spacing_mm: 5.0 },
        PlaceTool::CalendarMonth => WidgetKind::CalendarMonth,
        PlaceTool::Timeline => WidgetKind::Timeline { start_hour: 8, end_hour: 20, slot_minutes: 30 },
        PlaceTool::Checklist => WidgetKind::Checklist { items: vec!["Item 1".into(), "Item 2".into(), "Item 3".into()] },
        PlaceTool::BigThree => WidgetKind::BigThree,
        PlaceTool::PriorityList => WidgetKind::PriorityList { count: 12 },
        PlaceTool::DailyAppointments => WidgetKind::DailyAppointments { start_hour: 7, end_hour: 19 },
        PlaceTool::WeeklyCompass => WidgetKind::WeeklyCompass,
        PlaceTool::None => WidgetKind::Rectangle,
    }
}

fn draw_creator_canvas(ctx: &cairo::Context, w: f64, h: f64, cs: &CreatorState) {
    ctx.set_source_rgb(0.85, 0.85, 0.88);
    let _ = ctx.paint();

    if w <= 0.0 || h <= 0.0 {
        return;
    }

    let template = &cs.template;
    let scale = template_scale((w, h), template);
    let (ox, oy) = template_origin((w, h), template);
    let (tw, th) = template.size_mm;

    ctx.save().ok();
    ctx.translate(ox, oy);
    ctx.scale(scale, scale);

    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.rectangle(0.0, 0.0, tw, th);
    let _ = ctx.fill();

    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.15);
    ctx.set_line_width(0.5 / scale);
    ctx.rectangle(0.0, 0.0, tw, th);
    let _ = ctx.stroke();

    let page_rect = Rect { x: 0.0, y: 0.0, width: tw, height: th };

    let viewport = journal_core::Viewport {
        center: journal_core::Point { x: tw * 0.5, y: th * 0.5 },
        zoom: scale,
        rotation: 0.0,
    };
    let transform = ViewportTransform::new(viewport, tw * scale, th * scale);

    if !template.widgets.is_empty() {
        draw_widgets(ctx, &transform, &template.widgets, page_rect);
    }

    if let Some(idx) = cs.selected_idx {
        if let Some(w_ref) = template.widgets.get(idx) {
            draw_selection_overlay(ctx, &w_ref.rect, scale);
        }
    }

    ctx.restore().ok();
}

fn draw_selection_overlay(ctx: &cairo::Context, r: &WidgetRect, scale: f64) {
    let lw = 1.5 / scale;
    ctx.set_line_width(lw);
    ctx.set_source_rgba(0.2, 0.5, 1.0, 0.8);
    ctx.rectangle(r.x, r.y, r.width, r.height);
    let _ = ctx.stroke();

    let handle_sz = 6.0 / scale;
    let hx = r.x + r.width - handle_sz * 0.5;
    let hy = r.y + r.height - handle_sz * 0.5;
    ctx.set_source_rgba(0.2, 0.5, 1.0, 1.0);
    ctx.rectangle(hx, hy, handle_sz, handle_sz);
    let _ = ctx.fill();
}

fn templates_dir() -> Option<std::path::PathBuf> {
    let base = dirs::data_dir().or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))?;
    Some(base.join("journal").join("templates"))
}

fn save_template(template: &PageTemplate, state: &SharedState) -> anyhow::Result<()> {
    let tdir = templates_dir().ok_or_else(|| anyhow::anyhow!("could not resolve data dir"))?;
    std::fs::create_dir_all(&tdir)?;
    let toml_path = tdir.join(format!("{}.toml", template.id.0));
    let file = template_file_from_page_template(template);
    let toml_text = serialize_template_toml(&file)
        .map_err(|e| anyhow::anyhow!("serialize: {}", e))?;
    std::fs::write(&toml_path, toml_text)?;
    let s = state.borrow();
    s.templates.borrow_mut().insert(template.clone());
    Ok(())
}
