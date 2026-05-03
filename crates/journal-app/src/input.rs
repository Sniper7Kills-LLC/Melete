use std::cell::Cell;
use std::rc::Rc;

use chrono::Utc;
use gtk4::gdk::ModifierType;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    DrawingArea, EventControllerScroll, EventControllerScrollFlags, GestureDrag, GestureStylus,
    GestureZoom,
};
use journal_core::{Rect, Stroke, StrokePoint};
use journal_storage::stroke_store;
use uuid::Uuid;

use crate::state::SharedState;

fn now_ms() -> u64 {
    Utc::now().timestamp_millis().max(0) as u64
}

fn extend_bbox(bbox: &mut Rect, p: &StrokePoint) {
    let x0 = bbox.x.min(p.x);
    let y0 = bbox.y.min(p.y);
    let x1 = (bbox.x + bbox.width).max(p.x);
    let y1 = (bbox.y + bbox.height).max(p.y);
    bbox.x = x0;
    bbox.y = y0;
    bbox.width = x1 - x0;
    bbox.height = y1 - y0;
}

fn pad_bbox(bbox: &mut Rect, padding: f64) {
    bbox.x -= padding;
    bbox.y -= padding;
    bbox.width += padding * 2.0;
    bbox.height += padding * 2.0;
}

pub fn attach_stylus(area: &DrawingArea, state: SharedState) {
    let gesture = GestureStylus::new();
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);

    {
        let state = state.clone();
        let area = area.clone();
        gesture.connect_down(move |g, x, y| {
            let pressure = g.axis(gtk4::gdk::AxisUse::Pressure).unwrap_or(0.5) as f32;
            let tilt_x = g.axis(gtk4::gdk::AxisUse::Xtilt).unwrap_or(0.0) as f32;
            let tilt_y = g.axis(gtk4::gdk::AxisUse::Ytilt).unwrap_or(0.0) as f32;
            begin_stroke(&state, x, y, pressure, tilt_x, tilt_y);
            area.queue_draw();
        });
    }

    {
        let state = state.clone();
        let area = area.clone();
        gesture.connect_motion(move |g, x, y| {
            let pressure = g.axis(gtk4::gdk::AxisUse::Pressure).unwrap_or(0.5) as f32;
            let tilt_x = g.axis(gtk4::gdk::AxisUse::Xtilt).unwrap_or(0.0) as f32;
            let tilt_y = g.axis(gtk4::gdk::AxisUse::Ytilt).unwrap_or(0.0) as f32;
            extend_stroke(&state, x, y, pressure, tilt_x, tilt_y);
            area.queue_draw();
        });
    }

    {
        let state = state.clone();
        let area = area.clone();
        gesture.connect_up(move |_g, _x, _y| {
            finish_stroke(&state);
            area.queue_draw();
        });
    }

    area.add_controller(gesture);
}

pub fn attach_mouse(area: &DrawingArea, state: SharedState) {
    let gesture = gtk4::GestureDrag::new();
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk4::PropagationPhase::Bubble);

    {
        let state = state.clone();
        let area = area.clone();
        gesture.connect_drag_begin(move |_g, x, y| {
            begin_stroke(&state, x, y, 0.5, 0.0, 0.0);
            area.queue_draw();
        });
    }

    {
        let state = state.clone();
        let area = area.clone();
        gesture.connect_drag_update(move |g, dx, dy| {
            if let Some((sx, sy)) = g.start_point() {
                extend_stroke(&state, sx + dx, sy + dy, 0.5, 0.0, 0.0);
                area.queue_draw();
            }
        });
    }

    {
        let state = state.clone();
        let area = area.clone();
        gesture.connect_drag_end(move |_g, _dx, _dy| {
            finish_stroke(&state);
            area.queue_draw();
        });
    }

    area.add_controller(gesture);
}

pub fn attach_pan_zoom(area: &DrawingArea, state: SharedState) {
    let pan = GestureDrag::new();
    pan.set_button(gtk4::gdk::BUTTON_MIDDLE);

    {
        let state = state.clone();
        let area = area.clone();
        pan.connect_drag_update(move |g, dx, dy| {
            if let Some(_) = g.start_point() {
                state.borrow_mut().transform.pan(-dx, -dy);
                area.queue_draw();
            }
        });
    }
    area.add_controller(pan);

    #[derive(Clone, Copy, PartialEq)]
    enum GestureMode {
        Undecided,
        Pan,
        Zoom,
    }

    let zoom = GestureZoom::new();
    let last_scale: Rc<Cell<f64>> = Rc::new(Cell::new(1.0));
    let last_center: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));
    let start_center: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));
    let mode: Rc<Cell<GestureMode>> = Rc::new(Cell::new(GestureMode::Undecided));

    const PAN_THRESHOLD_PX: f64 = 12.0;
    const ZOOM_THRESHOLD: f64 = 0.08;

    {
        let state = state.clone();
        let area = area.clone();
        let last_scale = last_scale.clone();
        let last_center = last_center.clone();
        let start_center = start_center.clone();
        let mode = mode.clone();
        zoom.connect_scale_changed(move |g, scale| {
            let (cx, cy) = match g.bounding_box_center() {
                Some(c) => c,
                None => return,
            };

            if mode.get() == GestureMode::Undecided {
                let scale_delta = (scale - 1.0).abs();
                let center_drift = match start_center.get() {
                    Some((sx, sy)) => ((cx - sx).powi(2) + (cy - sy).powi(2)).sqrt(),
                    None => 0.0,
                };
                if scale_delta > ZOOM_THRESHOLD {
                    mode.set(GestureMode::Zoom);
                } else if center_drift > PAN_THRESHOLD_PX {
                    mode.set(GestureMode::Pan);
                }
            }

            let mut s = state.borrow_mut();
            match mode.get() {
                GestureMode::Pan => {
                    if let Some((px, py)) = last_center.get() {
                        let dx = cx - px;
                        let dy = cy - py;
                        if dx.abs() > 0.0 || dy.abs() > 0.0 {
                            s.transform.pan(dx, dy);
                        }
                    }
                }
                GestureMode::Zoom => {
                    let prev = last_scale.get();
                    let factor = scale / prev;
                    if (factor - 1.0).abs() > 1e-6 {
                        s.transform.zoom_at((cx, cy), factor);
                    }
                }
                GestureMode::Undecided => {}
            }

            last_scale.set(scale);
            last_center.set(Some((cx, cy)));
            area.queue_draw();
        });
    }
    {
        let last_scale = last_scale.clone();
        let last_center = last_center.clone();
        let start_center = start_center.clone();
        let mode = mode.clone();
        zoom.connect_begin(move |g, _| {
            last_scale.set(1.0);
            let c = g.bounding_box_center();
            last_center.set(c);
            start_center.set(c);
            mode.set(GestureMode::Undecided);
        });
    }
    {
        let last_center = last_center.clone();
        let start_center = start_center.clone();
        let mode = mode.clone();
        zoom.connect_end(move |_, _| {
            last_center.set(None);
            start_center.set(None);
            mode.set(GestureMode::Undecided);
        });
    }
    area.add_controller(zoom);

    let scroll = EventControllerScroll::new(EventControllerScrollFlags::BOTH_AXES);
    {
        let state = state.clone();
        let area = area.clone();
        scroll.connect_scroll(move |ctrl, dx, dy| {
            let mods = ctrl.current_event_state();
            if mods.contains(ModifierType::CONTROL_MASK) {
                let factor = if dy > 0.0 { 0.9 } else { 1.1 };
                let (cx, cy) = (area.width() as f64 / 2.0, area.height() as f64 / 2.0);
                state.borrow_mut().transform.zoom_at((cx, cy), factor);
            } else {
                state.borrow_mut().transform.pan(dx * 30.0, dy * 30.0);
            }
            area.queue_draw();
            glib::Propagation::Stop
        });
    }
    area.add_controller(scroll);
}

fn begin_stroke(state: &SharedState, sx: f64, sy: f64, pressure: f32, tx: f32, ty: f32) {
    let mut s = state.borrow_mut();
    if s.current_page_id.is_none() {
        return;
    }
    let canvas = s.transform.screen_to_canvas((sx, sy));
    let pt = StrokePoint {
        x: canvas.x,
        y: canvas.y,
        pressure,
        tilt_x: tx,
        tilt_y: ty,
        timestamp_ms: now_ms(),
    };
    let bbox = Rect { x: pt.x, y: pt.y, width: 0.0, height: 0.0 };
    s.current_stroke = Some(Stroke {
        id: Uuid::new_v4(),
        points: vec![pt],
        pen: s.pen,
        zoom_at_creation: s.transform.zoom(),
        bounding_box: bbox,
    });
}

fn extend_stroke(state: &SharedState, sx: f64, sy: f64, pressure: f32, tx: f32, ty: f32) {
    let mut s = state.borrow_mut();
    if s.current_stroke.is_none() {
        return;
    }
    let canvas = s.transform.screen_to_canvas((sx, sy));
    let pt = StrokePoint {
        x: canvas.x,
        y: canvas.y,
        pressure,
        tilt_x: tx,
        tilt_y: ty,
        timestamp_ms: now_ms(),
    };
    if let Some(cs) = s.current_stroke.as_mut() {
        extend_bbox(&mut cs.bounding_box, &pt);
        cs.points.push(pt);
    }
}

fn finish_stroke(state: &SharedState) {
    let (saved, db_opt, page_opt) = {
        let mut s = state.borrow_mut();
        if let Some(mut stroke) = s.current_stroke.take() {
            if stroke.points.len() >= 2 {
                let half_width = stroke.pen.base_width / stroke.zoom_at_creation.max(1e-6);
                pad_bbox(&mut stroke.bounding_box, half_width);
                let saved = stroke.clone();
                s.strokes.push(stroke);
                (Some(saved), Some(s.db.clone()), s.current_page_id)
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        }
    };

    if let (Some(stroke), Some(db), Some(page_id)) = (saved, db_opt, page_opt) {
        if let Err(e) = stroke_store::insert_stroke(db.borrow().conn(), &stroke, page_id) {
            tracing::error!("failed to persist stroke for {:?}: {}", page_id, e);
        }
    }
}

