//! Template Preview window — renders a PageTemplate through the
//! production Vello pipeline with widget data populated, so the user
//! can see how fetch-backed widgets (Quote, Weather, BibleVerse, …)
//! actually look on a real page without leaving the editor.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use chrono::Local;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box as GtkBox, Button, DrawingArea, Label, Orientation, Window};
use melete_canvas::vello_renderer::{OverlayState, ToolStyleParams, VelloRenderer};
use melete_canvas::ViewportTransform;
use melete_core::{PageTemplate, Point, Rect, Viewport, WidgetData, WidgetPayload};
use melete_widgets::{WidgetRenderContext, WidgetRenderer};
use uuid::Uuid;

use crate::fetcher;

pub fn open_preview(parent: &ApplicationWindow, template: PageTemplate) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(format!(
            "Preview — {}",
            if template.name.is_empty() {
                "untitled template"
            } else {
                template.name.as_str()
            }
        ))
        .default_width(720)
        .default_height(900)
        .build();

    let outer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(0)
        .build();

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();
    let date = Local::now().date_naive();
    let date_lbl = Label::builder()
        .label(format!("As of {}", date.format("%a, %b %-d, %Y")))
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    date_lbl.add_css_class("dim-label");
    let status_lbl = Label::builder()
        .label("Loading widget data…")
        .halign(gtk4::Align::End)
        .build();
    status_lbl.add_css_class("dim-label");
    let close_btn = Button::with_label("Close");
    {
        let win = win.clone();
        close_btn.connect_clicked(move |_| win.close());
    }
    header.append(&date_lbl);
    header.append(&status_lbl);
    header.append(&close_btn);
    outer.append(&header);

    let area = DrawingArea::builder().hexpand(true).vexpand(true).build();
    outer.append(&area);

    let widget_data: Rc<RefCell<HashMap<Uuid, WidgetData>>> = Rc::new(RefCell::new(HashMap::new()));
    let renderer: Rc<RefCell<Option<VelloRenderer>>> = Rc::new(RefCell::new(None));
    let widgets_renderer: Rc<RefCell<WidgetRenderer>> =
        Rc::new(RefCell::new(WidgetRenderer::new()));

    // Spawn a worker thread that fetches every fetch-backed widget once
    // and sends results back over an mpsc channel. Non-fetch widgets
    // (TextBlock, Calendar, Rectangle, etc.) skip the channel entirely
    // — the renderer already shows them populated from page geometry.
    let (tx, rx) = mpsc::channel::<(Uuid, WidgetData)>();
    let to_fetch_count = template
        .widgets
        .iter()
        .filter(|w| fetcher::freshness_for(&w.kind).is_some())
        .count();

    {
        let template_for_worker = template.clone();
        std::thread::spawn(move || {
            for w in &template_for_worker.widgets {
                if fetcher::freshness_for(&w.kind).is_none() {
                    continue;
                }
                let payload = match fetcher::fetch_widget_blocking(w, Some(date)) {
                    Ok(p) => p,
                    Err(e) => WidgetPayload::Error { message: e },
                };
                let data = WidgetData {
                    payload,
                    fetched_at: chrono::Utc::now(),
                    frozen: false,
                };
                if tx.send((w.id, data)).is_err() {
                    break;
                }
            }
        });
    }

    // Drain the channel onto the main thread. Any new payload triggers a
    // redraw so the user sees results stream in.
    {
        let widget_data = widget_data.clone();
        let area_inner = area.clone();
        let status_lbl = status_lbl.clone();
        let received = Rc::new(RefCell::new(0usize));
        glib::timeout_add_local(Duration::from_millis(150), move || {
            let mut updated = false;
            loop {
                match rx.try_recv() {
                    Ok((id, data)) => {
                        widget_data.borrow_mut().insert(id, data);
                        *received.borrow_mut() += 1;
                        updated = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            }
            let n = *received.borrow();
            if to_fetch_count == 0 {
                status_lbl.set_text("No fetch widgets in this template.");
            } else if n >= to_fetch_count {
                status_lbl.set_text(&format!("Loaded {} / {} widgets.", n, to_fetch_count));
            } else {
                status_lbl.set_text(&format!("Loading… {} / {} widgets", n, to_fetch_count));
            }
            if updated {
                area_inner.queue_draw();
            }
            // Stop polling once all fetches are in.
            if n >= to_fetch_count {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    {
        let template_for_draw = template.clone();
        let renderer = renderer.clone();
        let widgets_renderer = widgets_renderer.clone();
        let widget_data = widget_data.clone();
        area.set_draw_func(move |_, ctx, w, h| {
            if w <= 0 || h <= 0 {
                return;
            }
            let mut renderer_slot = renderer.borrow_mut();
            if renderer_slot.is_none() {
                match VelloRenderer::new() {
                    Ok(r) => *renderer_slot = Some(r),
                    Err(e) => {
                        tracing::warn!("preview vello init: {e:?}");
                        draw_init_failure(ctx, w, h);
                        return;
                    }
                }
            }
            let r = renderer_slot.as_mut().unwrap();

            let (tw_mm, th_mm) = template_for_draw.size_mm;
            let zoom = ((w as f64 / tw_mm).min(h as f64 / th_mm)) * 0.95;
            let viewport = Viewport {
                center: Point {
                    x: tw_mm * 0.5,
                    y: th_mm * 0.5,
                },
                zoom,
                rotation: 0.0,
            };
            let transform = ViewportTransform::new(viewport, w as f64, h as f64);
            let page_rect = Rect {
                x: 0.0,
                y: 0.0,
                width: tw_mm,
                height: th_mm,
            };
            let bg = melete_templates::page_template_to_background_config(&template_for_draw);
            let widgets = template_for_draw.widgets.clone();
            let widget_ctx = WidgetRenderContext {
                date: Some(date),
                overrides: Default::default(),
                widget_data: widget_data.borrow().clone(),
                dark_mode: crate::is_dark_mode(),
            };
            let overlays = OverlayState {
                dark_mode: crate::is_dark_mode(),
                ..Default::default()
            };

            let widgets_renderer_for_closure = widgets_renderer.clone();
            // Template-editor preview has no backend handle yet, so it
            // can't resolve `asset:<name>` URIs to bytes. Image / PDF
            // backgrounds therefore render as blank in the preview.
            // Wiring an in-memory pending-asset resolver is a follow-up.
            let resolver = crate::asset_resolver::null_resolver();
            let bytes = match r.render_rgba(
                &transform,
                &bg,
                page_rect,
                &[],
                &HashSet::new(),
                &overlays,
                &ToolStyleParams::default(),
                &resolver,
                w as u32,
                h as u32,
                |scene, world_to_screen, pr| {
                    widgets_renderer_for_closure.borrow_mut().draw_widgets(
                        scene,
                        world_to_screen,
                        &widgets,
                        pr,
                        &widget_ctx,
                    );
                },
            ) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("preview render: {e:?}");
                    return;
                }
            };
            blit_rgba_to_cairo(ctx, &bytes, w as u32, h as u32);
        });
    }

    win.set_child(Some(&outer));
    win.present();
}

fn blit_rgba_to_cairo(ctx: &gtk4::cairo::Context, rgba: &[u8], w: u32, h: u32) {
    use gtk4::cairo;
    let mut bgra = vec![0u8; rgba.len()];
    for px in 0..(rgba.len() / 4) {
        let i = px * 4;
        bgra[i] = rgba[i + 2];
        bgra[i + 1] = rgba[i + 1];
        bgra[i + 2] = rgba[i];
        bgra[i + 3] = rgba[i + 3];
    }
    let stride = cairo::Format::ARgb32
        .stride_for_width(w)
        .unwrap_or((w * 4) as i32);
    let surface = match cairo::ImageSurface::create_for_data(
        bgra,
        cairo::Format::ARgb32,
        w as i32,
        h as i32,
        stride,
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("preview surface: {e}");
            return;
        }
    };
    let _ = ctx.set_source_surface(&surface, 0.0, 0.0);
    let _ = ctx.paint();
}

fn draw_init_failure(ctx: &gtk4::cairo::Context, _w: i32, h: i32) {
    ctx.set_source_rgb(0.6, 0.3, 0.3);
    ctx.move_to(12.0, h as f64 * 0.55);
    let _ = ctx.show_text("(GPU preview unavailable)");
}
