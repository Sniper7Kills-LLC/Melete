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

use crate::history::Op;
use crate::state::{EraserMode, SharedState, Tool};

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

fn stroke_bbox_intersects_circle(stroke: &Stroke, cx: f64, cy: f64, r: f64) -> bool {
    let bb = stroke.bounding_box;
    let nearest_x = cx.clamp(bb.x, bb.x + bb.width);
    let nearest_y = cy.clamp(bb.y, bb.y + bb.height);
    let dx = cx - nearest_x;
    let dy = cy - nearest_y;
    dx * dx + dy * dy <= r * r
}

fn point_in_polygon(px: f64, py: f64, polygon: &[(f64, f64)]) -> bool {
    let n = polygon.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn stroke_bbox_in_polygon(stroke: &Stroke, polygon: &[(f64, f64)]) -> bool {
    let bb = stroke.bounding_box;
    let corners = [
        (bb.x, bb.y),
        (bb.x + bb.width, bb.y),
        (bb.x, bb.y + bb.height),
        (bb.x + bb.width, bb.y + bb.height),
    ];
    corners.iter().all(|&(x, y)| point_in_polygon(x, y, polygon))
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
            handle_begin(&state, x, y, pressure, tilt_x, tilt_y);
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
            handle_motion(&state, x, y, pressure, tilt_x, tilt_y, &area);
            area.queue_draw();
        });
    }

    {
        let state = state.clone();
        let area = area.clone();
        gesture.connect_up(move |_g, _x, _y| {
            handle_end(&state);
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
            handle_begin(&state, x, y, 0.5, 0.0, 0.0);
            area.queue_draw();
        });
    }

    {
        let state = state.clone();
        let area = area.clone();
        gesture.connect_drag_update(move |g, dx, dy| {
            if let Some((sx, sy)) = g.start_point() {
                handle_motion(&state, sx + dx, sy + dy, 0.5, 0.0, 0.0, &area);
                area.queue_draw();
            }
        });
    }

    {
        let state = state.clone();
        let area = area.clone();
        gesture.connect_drag_end(move |_g, _dx, _dy| {
            handle_end(&state);
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

fn handle_begin(state: &SharedState, sx: f64, sy: f64, pressure: f32, tx: f32, ty: f32) {
    let tool = state.borrow().tool;
    match tool {
        Tool::Pen | Tool::Highlighter => begin_stroke(state, sx, sy, pressure, tx, ty),
        Tool::Eraser(_) => {}
        Tool::Selection => begin_selection(state, sx, sy),
    }
}

fn handle_motion(state: &SharedState, sx: f64, sy: f64, pressure: f32, tx: f32, ty: f32, area: &DrawingArea) {
    let tool = state.borrow().tool;
    match tool {
        Tool::Pen | Tool::Highlighter => extend_stroke(state, sx, sy, pressure, tx, ty),
        Tool::Eraser(EraserMode::Stroke) => erase_at(state, sx, sy, area),
        Tool::Selection => extend_selection(state, sx, sy),
    }
}

fn handle_end(state: &SharedState) {
    let tool = state.borrow().tool;
    match tool {
        Tool::Pen | Tool::Highlighter => finish_stroke(state),
        Tool::Eraser(_) => {}
        Tool::Selection => finish_selection(state),
    }
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

    let mut pen = s.pen;
    if s.tool == Tool::Highlighter {
        pen.opacity = 0.35;
        pen.base_width *= 4.0;
        pen.blend_mode = journal_core::BlendMode::Multiply;
    }

    s.current_stroke = Some(Stroke {
        id: Uuid::new_v4(),
        points: vec![pt],
        pen,
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
                s.history.push_add(saved.clone());
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

fn erase_at(state: &SharedState, sx: f64, sy: f64, area: &DrawingArea) {
    let (page_id, canvas_pos, zoom, db) = {
        let s = state.borrow();
        if s.current_page_id.is_none() {
            return;
        }
        let cp = s.transform.screen_to_canvas((sx, sy));
        (s.current_page_id.unwrap(), cp, s.transform.zoom(), s.db.clone())
    };

    let radius_canvas = 5.0 / zoom.max(1e-6);
    let cx = canvas_pos.x;
    let cy = canvas_pos.y;

    let to_remove: Vec<Stroke> = {
        let s = state.borrow();
        s.strokes
            .iter()
            .filter(|st| stroke_bbox_intersects_circle(st, cx, cy, radius_canvas))
            .cloned()
            .collect()
    };

    if to_remove.is_empty() {
        return;
    }

    {
        let mut s = state.borrow_mut();
        for stroke in &to_remove {
            s.strokes.retain(|st| st.id != stroke.id);
            s.history.push_remove(stroke.clone());
        }
    }

    for stroke in &to_remove {
        if let Err(e) = stroke_store::delete_stroke(db.borrow().conn(), stroke.id) {
            tracing::warn!("erase: failed to delete stroke {} for page {:?}: {}", stroke.id, page_id, e);
        }
    }

    area.queue_draw();
}

fn begin_selection(state: &SharedState, sx: f64, sy: f64) {
    let mut s = state.borrow_mut();
    if s.current_page_id.is_none() {
        return;
    }

    let canvas_pos = s.transform.screen_to_canvas((sx, sy));

    let hit_selected = s.selected_stroke_ids.iter().any(|id| {
        s.strokes.iter().filter(|st| st.id == *id).any(|st| {
            stroke_bbox_intersects_circle(st, canvas_pos.x, canvas_pos.y, 10.0 / s.transform.zoom().max(1e-6))
        })
    });

    if hit_selected {
        s.selection_drag_start = Some((sx, sy));
        s.selection_drag_total_canvas = (0.0, 0.0);
    } else {
        s.selected_stroke_ids.clear();
        s.lasso_points = vec![(sx, sy)];
        s.lasso_active = true;
        s.selection_drag_start = None;
    }
}

fn extend_selection(state: &SharedState, sx: f64, sy: f64) {
    let mut s = state.borrow_mut();

    if let Some((ox, oy)) = s.selection_drag_start {
        let dx_screen = sx - ox;
        let dy_screen = sy - oy;
        let zoom = s.transform.zoom().max(1e-6);
        let dx_canvas = dx_screen / zoom;
        let dy_canvas = dy_screen / zoom;
        s.selection_drag_start = Some((sx, sy));
        s.selection_drag_total_canvas.0 += dx_canvas;
        s.selection_drag_total_canvas.1 += dy_canvas;

        let selected_ids: Vec<Uuid> = s.selected_stroke_ids.iter().cloned().collect();
        for st in s.strokes.iter_mut() {
            if selected_ids.contains(&st.id) {
                for pt in st.points.iter_mut() {
                    pt.x += dx_canvas;
                    pt.y += dy_canvas;
                }
                st.bounding_box.x += dx_canvas;
                st.bounding_box.y += dy_canvas;
            }
        }
    } else if s.lasso_active {
        s.lasso_points.push((sx, sy));
    }
}

fn finish_selection(state: &SharedState) {
    let (page_id, db, strokes_to_update, moved_ids) = {
        let mut s = state.borrow_mut();

        let moved_ids: Vec<Uuid> = if s.selection_drag_start.is_some() {
            s.selection_drag_start = None;
            s.selected_stroke_ids.iter().cloned().collect()
        } else {
            Vec::new()
        };

        if !moved_ids.is_empty() {
            let (tx, ty) = s.selection_drag_total_canvas;
            s.selection_drag_total_canvas = (0.0, 0.0);
            s.history.push_move(moved_ids.clone(), tx, ty);
        }

        if s.lasso_active && s.lasso_points.len() >= 3 {
            let lasso = s.lasso_points.clone();
            let transform = s.transform;
            let lasso_canvas: Vec<(f64, f64)> = lasso
                .iter()
                .map(|&(sx, sy)| {
                    let cp = transform.screen_to_canvas((sx, sy));
                    (cp.x, cp.y)
                })
                .collect();

            let newly_selected: Vec<Uuid> = s.strokes
                .iter()
                .filter(|st| stroke_bbox_in_polygon(st, &lasso_canvas))
                .map(|st| st.id)
                .collect();
            for id in newly_selected {
                s.selected_stroke_ids.insert(id);
            }
        }

        s.lasso_points.clear();
        s.lasso_active = false;

        let page_id = s.current_page_id;
        let db = s.db.clone();
        let strokes_to_update: Vec<Stroke> = if !moved_ids.is_empty() {
            s.strokes.iter().filter(|st| moved_ids.contains(&st.id)).cloned().collect()
        } else {
            Vec::new()
        };

        (page_id, db, strokes_to_update, moved_ids)
    };

    if let Some(pid) = page_id {
        for stroke in &strokes_to_update {
            if let Err(e) = stroke_store::update_stroke(db.borrow().conn(), stroke, pid) {
                tracing::warn!("selection move: failed to update stroke {}: {}", stroke.id, e);
            }
        }
        let _ = moved_ids;
    }
}

pub fn delete_selection(state: &SharedState, area: &DrawingArea) {
    let (ids, db, page_id) = {
        let s = state.borrow();
        if s.selected_stroke_ids.is_empty() || s.current_page_id.is_none() {
            return;
        }
        let ids: Vec<Uuid> = s.selected_stroke_ids.iter().cloned().collect();
        (ids, s.db.clone(), s.current_page_id.unwrap())
    };

    let strokes_removed: Vec<Stroke> = {
        let s = state.borrow();
        s.strokes.iter().filter(|st| ids.contains(&st.id)).cloned().collect()
    };

    {
        let mut s = state.borrow_mut();
        for id in &ids {
            s.strokes.retain(|st| st.id != *id);
        }
        for stroke in &strokes_removed {
            s.history.push_remove(stroke.clone());
        }
        s.selected_stroke_ids.clear();
    }

    for id in &ids {
        if let Err(e) = stroke_store::delete_stroke(db.borrow().conn(), *id) {
            tracing::warn!("delete_selection: failed to delete stroke {} for page {:?}: {}", id, page_id, e);
        }
    }

    area.queue_draw();
}

pub fn undo(state: &SharedState, area: &DrawingArea) {
    let op = {
        let mut s = state.borrow_mut();
        s.history.undo.pop()
    };
    let Some(op) = op else { return };

    match op {
        Op::AddStroke(stroke) => {
            let (db, page_id) = {
                let mut s = state.borrow_mut();
                s.strokes.retain(|st| st.id != stroke.id);
                s.history.redo.push(Op::AddStroke(stroke.clone()));
                (s.db.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                if let Err(e) = stroke_store::delete_stroke(db.borrow().conn(), stroke.id) {
                    tracing::warn!("undo AddStroke: delete failed for {}: {}", stroke.id, e);
                }
                let _ = pid;
            }
        }
        Op::RemoveStroke(stroke) => {
            let (db, page_id) = {
                let mut s = state.borrow_mut();
                s.strokes.push(stroke.clone());
                s.history.redo.push(Op::RemoveStroke(stroke.clone()));
                (s.db.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                if let Err(e) = stroke_store::insert_stroke(db.borrow().conn(), &stroke, pid) {
                    tracing::warn!("undo RemoveStroke: insert failed for {}: {}", stroke.id, e);
                }
            }
        }
        Op::MoveStrokes { ids, dx, dy } => {
            apply_move(state, &ids, -dx, -dy);
            state.borrow_mut().history.redo.push(Op::MoveStrokes { ids, dx, dy });
        }
    }

    area.queue_draw();
}

pub fn redo(state: &SharedState, area: &DrawingArea) {
    let op = {
        let mut s = state.borrow_mut();
        s.history.redo.pop()
    };
    let Some(op) = op else { return };

    match op {
        Op::AddStroke(stroke) => {
            let (db, page_id) = {
                let mut s = state.borrow_mut();
                s.strokes.push(stroke.clone());
                s.history.undo.push(Op::AddStroke(stroke.clone()));
                (s.db.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                if let Err(e) = stroke_store::insert_stroke(db.borrow().conn(), &stroke, pid) {
                    tracing::warn!("redo AddStroke: insert failed for {}: {}", stroke.id, e);
                }
            }
        }
        Op::RemoveStroke(stroke) => {
            let (db, page_id) = {
                let mut s = state.borrow_mut();
                s.strokes.retain(|st| st.id != stroke.id);
                s.history.undo.push(Op::RemoveStroke(stroke.clone()));
                (s.db.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                if let Err(e) = stroke_store::delete_stroke(db.borrow().conn(), stroke.id) {
                    tracing::warn!("redo RemoveStroke: delete failed for {}: {}", stroke.id, e);
                }
                let _ = pid;
            }
        }
        Op::MoveStrokes { ids, dx, dy } => {
            apply_move(state, &ids, dx, dy);
            state.borrow_mut().history.undo.push(Op::MoveStrokes { ids, dx, dy });
        }
    }

    area.queue_draw();
}

fn apply_move(state: &SharedState, ids: &[Uuid], dx: f64, dy: f64) {
    let (db, page_id, updated) = {
        let mut s = state.borrow_mut();
        for st in s.strokes.iter_mut() {
            if ids.contains(&st.id) {
                for pt in st.points.iter_mut() {
                    pt.x += dx;
                    pt.y += dy;
                }
                st.bounding_box.x += dx;
                st.bounding_box.y += dy;
            }
        }
        let updated: Vec<Stroke> = s.strokes.iter().filter(|st| ids.contains(&st.id)).cloned().collect();
        (s.db.clone(), s.current_page_id, updated)
    };
    if let Some(pid) = page_id {
        for st in &updated {
            if let Err(e) = stroke_store::update_stroke(db.borrow().conn(), st, pid) {
                tracing::warn!("apply_move update_stroke {}: {}", st.id, e);
            }
        }
    }
}
