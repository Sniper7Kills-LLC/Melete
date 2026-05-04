use std::cell::Cell;
use std::rc::Rc;

use chrono::Utc;
use gtk4::gdk::ModifierType;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    EventControllerMotion, EventControllerScroll, EventControllerScrollFlags,
    GestureDrag, GestureStylus, GestureZoom,
};
use journal_canvas::{hit_test_handle, selection_combined_bbox};
use journal_core::{Rect, Stroke, StrokePoint};
// StrokeStore methods are reached via the dyn JournalBackend in state — no import needed.
use uuid::Uuid;

use crate::history::Op;
use crate::state::{EraserMode, HandlePos, SharedState, Tool};

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

pub fn attach_stylus(area_in: &impl IsA<gtk4::Widget>, state: SharedState) {
    let area: gtk4::Widget = area_in.clone().upcast();
    let area = &area;
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

pub fn attach_mouse(area_in: &impl IsA<gtk4::Widget>, state: SharedState) {
    let area: gtk4::Widget = area_in.clone().upcast();
    let area = &area;
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

/// Hover-pointer tracker — drives the brush cursor overlay. Stylus + mouse
/// gestures already update `pointer_screen` while pressed; this controller
/// fills in the gap when no button is held so the cursor circle follows
/// the pointer at all times.
pub fn attach_hover(area_in: &impl IsA<gtk4::Widget>, state: SharedState) {
    let area: gtk4::Widget = area_in.clone().upcast();
    let area = &area;
    let motion = EventControllerMotion::new();
    {
        let state = state.clone();
        let area = area.clone();
        motion.connect_motion(move |_, x, y| {
            state.borrow_mut().pointer_screen = Some((x, y));
            area.queue_draw();
        });
    }
    {
        let state = state.clone();
        let area = area.clone();
        motion.connect_enter(move |_, x, y| {
            state.borrow_mut().pointer_screen = Some((x, y));
            area.queue_draw();
        });
    }
    {
        let state = state.clone();
        let area = area.clone();
        motion.connect_leave(move |_| {
            state.borrow_mut().pointer_screen = None;
            area.queue_draw();
        });
    }
    area.add_controller(motion);
}

pub fn attach_pan_zoom(area_in: &impl IsA<gtk4::Widget>, state: SharedState) {
    let area: gtk4::Widget = area_in.clone().upcast();
    let area = &area;
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
    if crate::state::tool_is_drawing(tool) {
        begin_stroke(state, sx, sy, pressure, tx, ty);
        state.borrow_mut().pointer_drawing = true;
        return;
    }
    match tool {
        Tool::Eraser(_) => {}
        Tool::Selection => begin_selection(state, sx, sy),
        _ => {}
    }
}

fn handle_motion(state: &SharedState, sx: f64, sy: f64, pressure: f32, tx: f32, ty: f32, area: &gtk4::Widget) {
    {
        let mut s = state.borrow_mut();
        s.pointer_screen = Some((sx, sy));
    }
    let tool = state.borrow().tool;
    if crate::state::tool_is_drawing(tool) {
        extend_stroke(state, sx, sy, pressure, tx, ty);
        return;
    }
    match tool {
        Tool::Eraser(EraserMode::Stroke) => erase_at(state, sx, sy, area),
        Tool::Eraser(EraserMode::Partial) => partial_erase_at(state, sx, sy, area),
        Tool::Selection => extend_selection(state, sx, sy),
        _ => {}
    }
}

fn handle_end(state: &SharedState) {
    let tool = state.borrow().tool;
    state.borrow_mut().pointer_drawing = false;
    if crate::state::tool_is_drawing(tool) {
        finish_stroke(state);
        return;
    }
    match tool {
        Tool::Eraser(_) => {}
        Tool::Selection => finish_selection(state),
        _ => {}
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
    let (opacity, mult, blend, brush) = crate::state::tool_brush_params(&s, s.tool);
    pen.opacity = opacity;
    pen.base_width *= mult;
    pen.blend_mode = blend;
    pen.brush_style = brush;

    let brush_recipe = s.active_brush_recipe.clone();
    s.current_stroke = Some(Stroke {
        id: Uuid::new_v4(),
        points: vec![pt],
        pen,
        zoom_at_creation: s.transform.zoom(),
        bounding_box: bbox,
        brush_recipe,
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
                (Some(saved), Some(s.backend.clone()), s.current_page_id)
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        }
    };

    if let (Some(stroke), Some(db), Some(page_id)) = (saved, db_opt, page_opt) {
        if let Err(e) = db.borrow_mut().insert_stroke(&stroke, page_id) {
            tracing::error!("failed to persist stroke for {:?}: {}", page_id, e);
        }
    }
}

fn erase_at(state: &SharedState, sx: f64, sy: f64, area: &gtk4::Widget) {
    let (page_id, canvas_pos, zoom, db) = {
        let s = state.borrow();
        if s.current_page_id.is_none() {
            return;
        }
        let cp = s.transform.screen_to_canvas((sx, sy));
        (s.current_page_id.unwrap(), cp, s.transform.zoom(), s.backend.clone())
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
        if let Err(e) = db.borrow_mut().delete_stroke(stroke.id) {
            tracing::warn!("erase: failed to delete stroke {} for page {:?}: {}", stroke.id, page_id, e);
        }
    }

    area.queue_draw();
}

fn compute_stroke_bbox(points: &[StrokePoint], half_width: f64) -> Rect {
    if points.is_empty() {
        return Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
    }
    let mut x0 = points[0].x;
    let mut y0 = points[0].y;
    let mut x1 = x0;
    let mut y1 = y0;
    for p in points.iter().skip(1) {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Rect {
        x: x0 - half_width,
        y: y0 - half_width,
        width: (x1 - x0) + half_width * 2.0,
        height: (y1 - y0) + half_width * 2.0,
    }
}

/// Split a stroke at eraser circle: returns the child sub-strokes (runs of points
/// NOT within the eraser radius). Returns empty vec if nothing survives.
fn split_stroke_by_eraser(stroke: &Stroke, cx: f64, cy: f64, r: f64) -> Vec<Stroke> {
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let half_w = stroke.pen.base_width / zoc * 0.5;
    let r2 = r * r;
    let inside = |p: &StrokePoint| (p.x - cx).powi(2) + (p.y - cy).powi(2) <= r2;

    let mut children: Vec<Stroke> = Vec::new();
    let mut run: Vec<StrokePoint> = Vec::new();

    for pt in &stroke.points {
        if inside(pt) {
            if run.len() >= 2 {
                let bbox = compute_stroke_bbox(&run, half_w);
                children.push(Stroke {
                    id: Uuid::new_v4(),
                    points: run.clone(),
                    pen: stroke.pen,
                    zoom_at_creation: stroke.zoom_at_creation,
                    bounding_box: bbox,
                    brush_recipe: stroke.brush_recipe.clone(),
                });
            }
            run.clear();
        } else {
            run.push(pt.clone());
        }
    }
    if run.len() >= 2 {
        let bbox = compute_stroke_bbox(&run, half_w);
        children.push(Stroke {
            id: Uuid::new_v4(),
            points: run,
            pen: stroke.pen,
            zoom_at_creation: stroke.zoom_at_creation,
            bounding_box: bbox,
            brush_recipe: stroke.brush_recipe.clone(),
        });
    }
    children
}

fn partial_erase_at(state: &SharedState, sx: f64, sy: f64, area: &gtk4::Widget) {
    let (page_id, canvas_pos, zoom, db) = {
        let s = state.borrow();
        if s.current_page_id.is_none() {
            return;
        }
        let cp = s.transform.screen_to_canvas((sx, sy));
        (s.current_page_id.unwrap(), cp, s.transform.zoom(), s.backend.clone())
    };

    let radius_canvas = 10.0 / zoom.max(1e-6);
    let cx = canvas_pos.x;
    let cy = canvas_pos.y;

    let candidates: Vec<Stroke> = {
        let s = state.borrow();
        s.strokes
            .iter()
            .filter(|st| stroke_bbox_intersects_circle(st, cx, cy, radius_canvas))
            .cloned()
            .collect()
    };

    if candidates.is_empty() {
        return;
    }

    let mut did_split = false;

    for stroke in &candidates {
        let children = split_stroke_by_eraser(stroke, cx, cy, radius_canvas);
        let all_survive = children.len() == 1 && children[0].points == stroke.points;
        if all_survive {
            continue;
        }
        did_split = true;

        {
            let mut s = state.borrow_mut();
            s.strokes.retain(|st| st.id != stroke.id);
            for child in &children {
                s.strokes.push(child.clone());
            }
            s.history.push_replace(stroke.clone(), children.clone());
        }

        if let Err(e) = db.borrow_mut().replace_stroke(stroke.id, &children, page_id) {
            tracing::warn!("partial_erase: replace_stroke failed for {}: {}", stroke.id, e);
        }
    }

    if did_split {
        area.queue_draw();
    }
}

/// Split a stroke by lasso polygon: contiguous runs of inside points → selected children,
/// outside runs → unselected children. Returns (selected_children, unselected_children).
fn split_stroke_by_lasso(
    stroke: &Stroke,
    lasso: &[(f64, f64)],
) -> Option<(Vec<Stroke>, Vec<Stroke>)> {
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let half_w = stroke.pen.base_width / zoc * 0.5;

    let inside_flags: Vec<bool> = stroke.points.iter()
        .map(|p| point_in_polygon(p.x, p.y, lasso))
        .collect();

    let any_inside = inside_flags.iter().any(|&b| b);
    let all_inside = inside_flags.iter().all(|&b| b);

    if !any_inside {
        return None;
    }
    if all_inside {
        return None;
    }

    let mut selected_children: Vec<Stroke> = Vec::new();
    let mut unselected_children: Vec<Stroke> = Vec::new();

    let n = stroke.points.len();
    let mut i = 0;
    while i < n {
        let current_inside = inside_flags[i];
        let mut run: Vec<StrokePoint> = vec![stroke.points[i].clone()];
        i += 1;
        while i < n && inside_flags[i] == current_inside {
            run.push(stroke.points[i].clone());
            i += 1;
        }
        if run.len() < 2 {
            continue;
        }
        let bbox = compute_stroke_bbox(&run, half_w);
        let child = Stroke {
            id: Uuid::new_v4(),
            points: run,
            pen: stroke.pen,
            zoom_at_creation: stroke.zoom_at_creation,
            bounding_box: bbox,
            brush_recipe: stroke.brush_recipe.clone(),
        };
        if current_inside {
            selected_children.push(child);
        } else {
            unselected_children.push(child);
        }
    }

    Some((selected_children, unselected_children))
}

fn handle_idx_to_pos(idx: usize) -> HandlePos {
    match idx {
        0 => HandlePos::TL,
        1 => HandlePos::T,
        2 => HandlePos::TR,
        3 => HandlePos::R,
        4 => HandlePos::BR,
        5 => HandlePos::B,
        6 => HandlePos::BL,
        _ => HandlePos::L,
    }
}

fn begin_selection(state: &SharedState, sx: f64, sy: f64) {
    let mut s = state.borrow_mut();
    if s.current_page_id.is_none() {
        return;
    }

    // Check resize handles first (only when there's a selection).
    if !s.selected_stroke_ids.is_empty() {
        if let Some(bbox) = selection_combined_bbox(&s.strokes, &s.selected_stroke_ids) {
            if let Some(idx) = hit_test_handle(&s.transform, bbox, sx, sy) {
                let handle = handle_idx_to_pos(idx);
                let (_dummy_sx, _dummy_sy, anchor_x, anchor_y) = compute_scale_factors(handle, bbox, 0.0, 0.0);
                s.selection_resize_handle = Some(handle);
                s.selection_resize_start = Some((sx, sy));
                s.selection_resize_bbox_orig = Some(bbox);
                s.selection_resize_cumulative = (1.0, 1.0);
                s.selection_resize_anchor = (anchor_x, anchor_y);
                s.selection_drag_start = None;
                return;
            }
        }
    }

    let canvas_pos = s.transform.screen_to_canvas((sx, sy));

    let hit_selected = !s.selected_stroke_ids.is_empty() && s.selected_stroke_ids.iter().any(|id| {
        s.strokes.iter().filter(|st| st.id == *id).any(|st| {
            stroke_bbox_intersects_circle(st, canvas_pos.x, canvas_pos.y, 10.0 / s.transform.zoom().max(1e-6))
        })
    });

    if hit_selected {
        s.selection_drag_start = Some((sx, sy));
        s.selection_drag_total_canvas = (0.0, 0.0);
        s.selection_resize_handle = None;
    } else {
        s.selected_stroke_ids.clear();
        s.lasso_points = vec![(sx, sy)];
        s.lasso_active = true;
        s.selection_drag_start = None;
        s.selection_resize_handle = None;
        s.selection_resize_start = None;
        s.selection_resize_bbox_orig = None;
    }
}

fn extend_selection(state: &SharedState, sx: f64, sy: f64) {
    let mut s = state.borrow_mut();

    if s.selection_resize_handle.is_some() {
        let handle = s.selection_resize_handle.unwrap();
        let (prev_sx, prev_sy) = match s.selection_resize_start {
            Some(p) => p,
            None => return,
        };
        let orig_bbox = match s.selection_resize_bbox_orig {
            Some(b) => b,
            None => return,
        };

        let dx_screen = sx - prev_sx;
        let dy_screen = sy - prev_sy;
        let zoom = s.transform.zoom().max(1e-6);
        let dx = dx_screen / zoom;
        let dy = dy_screen / zoom;

        let (sx_factor, sy_factor, anchor_x, anchor_y) = compute_scale_factors(handle, orig_bbox, dx, dy);

        if sx_factor.abs() < 1e-6 || sy_factor.abs() < 1e-6 {
            return;
        }

        s.selection_resize_cumulative.0 *= sx_factor;
        s.selection_resize_cumulative.1 *= sy_factor;
        s.selection_resize_anchor = (anchor_x, anchor_y);

        let selected_ids: Vec<Uuid> = s.selected_stroke_ids.iter().cloned().collect();
        for st in s.strokes.iter_mut() {
            if selected_ids.contains(&st.id) {
                for pt in st.points.iter_mut() {
                    pt.x = anchor_x + (pt.x - anchor_x) * sx_factor;
                    pt.y = anchor_y + (pt.y - anchor_y) * sy_factor;
                }
                let bb = &mut st.bounding_box;
                let new_x = anchor_x + (bb.x - anchor_x) * sx_factor;
                let new_y = anchor_y + (bb.y - anchor_y) * sy_factor;
                bb.width *= sx_factor.abs();
                bb.height *= sy_factor.abs();
                bb.x = new_x.min(new_x + bb.width);
                bb.y = new_y.min(new_y + bb.height);
            }
        }

        s.selection_resize_start = Some((sx, sy));
        s.selection_resize_bbox_orig = selection_combined_bbox(&s.strokes, &s.selected_stroke_ids);

        return;
    }

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

/// Given the handle being dragged, return (sx, sy, anchor_x, anchor_y) for the
/// incremental delta (dx, dy in canvas units) applied from the original bbox.
fn compute_scale_factors(
    handle: HandlePos,
    orig: Rect,
    dx: f64,
    dy: f64,
) -> (f64, f64, f64, f64) {
    let l = orig.x;
    let r = orig.x + orig.width;
    let t = orig.y;
    let b = orig.y + orig.height;

    match handle {
        HandlePos::TL => {
            let new_w = (orig.width - dx).max(1.0);
            let new_h = (orig.height - dy).max(1.0);
            (new_w / orig.width, new_h / orig.height, r, b)
        }
        HandlePos::T => {
            let new_h = (orig.height - dy).max(1.0);
            (1.0, new_h / orig.height, (l + r) * 0.5, b)
        }
        HandlePos::TR => {
            let new_w = (orig.width + dx).max(1.0);
            let new_h = (orig.height - dy).max(1.0);
            (new_w / orig.width, new_h / orig.height, l, b)
        }
        HandlePos::R => {
            let new_w = (orig.width + dx).max(1.0);
            (new_w / orig.width, 1.0, l, (t + b) * 0.5)
        }
        HandlePos::BR => {
            let new_w = (orig.width + dx).max(1.0);
            let new_h = (orig.height + dy).max(1.0);
            (new_w / orig.width, new_h / orig.height, l, t)
        }
        HandlePos::B => {
            let new_h = (orig.height + dy).max(1.0);
            (1.0, new_h / orig.height, (l + r) * 0.5, t)
        }
        HandlePos::BL => {
            let new_w = (orig.width - dx).max(1.0);
            let new_h = (orig.height + dy).max(1.0);
            (new_w / orig.width, new_h / orig.height, r, t)
        }
        HandlePos::L => {
            let new_w = (orig.width - dx).max(1.0);
            (new_w / orig.width, 1.0, r, (t + b) * 0.5)
        }
    }
}

fn finish_selection(state: &SharedState) {
    // Handle resize commit.
    let resize_ids_and_bbox = {
        let s = state.borrow();
        if s.selection_resize_handle.is_some() {
            let ids: Vec<Uuid> = s.selected_stroke_ids.iter().cloned().collect();
            let orig = s.selection_resize_bbox_orig;
            Some((ids, orig))
        } else {
            None
        }
    };

    if let Some((ids, _orig_bbox_opt)) = resize_ids_and_bbox {
        let (db, page_id, strokes_to_update, history_args) = {
            let mut s = state.borrow_mut();
            s.selection_resize_handle = None;
            s.selection_resize_start = None;
            s.selection_resize_bbox_orig = None;

            let (csx, csy) = s.selection_resize_cumulative;
            let (ax, ay) = s.selection_resize_anchor;
            s.selection_resize_cumulative = (1.0, 1.0);

            let strokes_to_update: Vec<Stroke> = s.strokes
                .iter()
                .filter(|st| ids.contains(&st.id))
                .cloned()
                .collect();

            (s.backend.clone(), s.current_page_id, strokes_to_update, (ids.clone(), ax, ay, csx, csy))
        };

        if let Some(pid) = page_id {
            for st in &strokes_to_update {
                if let Err(e) = db.borrow_mut().update_stroke(st, pid) {
                    tracing::warn!("resize: update_stroke {}: {}", st.id, e);
                }
            }
        }

        let (h_ids, h_ax, h_ay, h_sx, h_sy) = history_args;
        if h_sx != 1.0 || h_sy != 1.0 {
            let mut s = state.borrow_mut();
            s.history.push_transform(h_ids, h_ax, h_ay, h_sx, h_sy);
        }

        return;
    }

    let (page_id, db, strokes_to_update, moved_ids, lasso_replacements) = {
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

        let mut lasso_replacements: Vec<(Stroke, Vec<Stroke>)> = Vec::new();

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

            let all_strokes: Vec<Stroke> = s.strokes.clone();
            for stroke in &all_strokes {
                if let Some((selected_children, unselected_children)) =
                    split_stroke_by_lasso(stroke, &lasso_canvas)
                {
                    let mut all_children = selected_children.clone();
                    all_children.extend(unselected_children.clone());
                    lasso_replacements.push((stroke.clone(), all_children));

                    s.strokes.retain(|st| st.id != stroke.id);
                    for child in &selected_children {
                        s.selected_stroke_ids.insert(child.id);
                        s.strokes.push(child.clone());
                    }
                    for child in &unselected_children {
                        s.strokes.push(child.clone());
                    }
                } else if stroke_bbox_in_polygon(stroke, &lasso_canvas) {
                    s.selected_stroke_ids.insert(stroke.id);
                }
            }

            for (old, new) in &lasso_replacements {
                s.history.push_replace(old.clone(), new.clone());
            }
        }

        s.lasso_points.clear();
        s.lasso_active = false;

        let page_id = s.current_page_id;
        let db = s.backend.clone();
        let strokes_to_update: Vec<Stroke> = if !moved_ids.is_empty() {
            s.strokes.iter().filter(|st| moved_ids.contains(&st.id)).cloned().collect()
        } else {
            Vec::new()
        };

        (page_id, db, strokes_to_update, moved_ids, lasso_replacements)
    };

    if let Some(pid) = page_id {
        for stroke in &strokes_to_update {
            if let Err(e) = db.borrow_mut().update_stroke(stroke, pid) {
                tracing::warn!("selection move: failed to update stroke {}: {}", stroke.id, e);
            }
        }
        for (old, new) in &lasso_replacements {
            if let Err(e) = db.borrow_mut().replace_stroke(old.id, new, pid) {
                tracing::warn!("lasso split: replace_stroke failed for {}: {}", old.id, e);
            }
        }
        let _ = moved_ids;
    }
}

pub fn delete_selection(state: &SharedState, area: &impl IsA<gtk4::Widget>) {
    let area: &gtk4::Widget = &area.clone().upcast();
    let (ids, db, page_id) = {
        let s = state.borrow();
        if s.selected_stroke_ids.is_empty() || s.current_page_id.is_none() {
            return;
        }
        let ids: Vec<Uuid> = s.selected_stroke_ids.iter().cloned().collect();
        (ids, s.backend.clone(), s.current_page_id.unwrap())
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
        if let Err(e) = db.borrow_mut().delete_stroke(*id) {
            tracing::warn!("delete_selection: failed to delete stroke {} for page {:?}: {}", id, page_id, e);
        }
    }

    area.queue_draw();
}

pub fn undo(state: &SharedState, area: &impl IsA<gtk4::Widget>) {
    let area: &gtk4::Widget = &area.clone().upcast();
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
                (s.backend.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                if let Err(e) = db.borrow_mut().delete_stroke(stroke.id) {
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
                (s.backend.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                if let Err(e) = db.borrow_mut().insert_stroke(&stroke, pid) {
                    tracing::warn!("undo RemoveStroke: insert failed for {}: {}", stroke.id, e);
                }
            }
        }
        Op::MoveStrokes { ids, dx, dy } => {
            apply_move(state, &ids, -dx, -dy);
            state.borrow_mut().history.redo.push(Op::MoveStrokes { ids, dx, dy });
        }
        Op::ReplaceStroke { old, new } => {
            let (db, page_id) = {
                let mut s = state.borrow_mut();
                for child in &new {
                    s.strokes.retain(|st| st.id != child.id);
                }
                s.strokes.push(old.clone());
                s.history.redo.push(Op::ReplaceStroke { old: old.clone(), new: new.clone() });
                (s.backend.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                for child in &new {
                    let _ = db.borrow_mut().delete_stroke(child.id);
                }
                if let Err(e) = db.borrow_mut().insert_stroke(&old, pid) {
                    tracing::warn!("undo ReplaceStroke: insert original failed for {}: {}", old.id, e);
                }
            }
        }
        Op::TransformStrokes { ids, anchor_x, anchor_y, sx, sy } => {
            apply_scale(state, &ids, anchor_x, anchor_y, 1.0 / sx.max(1e-9), 1.0 / sy.max(1e-9));
            state.borrow_mut().history.redo.push(Op::TransformStrokes { ids, anchor_x, anchor_y, sx, sy });
        }
    }

    area.queue_draw();
}

pub fn redo(state: &SharedState, area: &impl IsA<gtk4::Widget>) {
    let area: &gtk4::Widget = &area.clone().upcast();
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
                (s.backend.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                if let Err(e) = db.borrow_mut().insert_stroke(&stroke, pid) {
                    tracing::warn!("redo AddStroke: insert failed for {}: {}", stroke.id, e);
                }
            }
        }
        Op::RemoveStroke(stroke) => {
            let (db, page_id) = {
                let mut s = state.borrow_mut();
                s.strokes.retain(|st| st.id != stroke.id);
                s.history.undo.push(Op::RemoveStroke(stroke.clone()));
                (s.backend.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                if let Err(e) = db.borrow_mut().delete_stroke(stroke.id) {
                    tracing::warn!("redo RemoveStroke: delete failed for {}: {}", stroke.id, e);
                }
                let _ = pid;
            }
        }
        Op::MoveStrokes { ids, dx, dy } => {
            apply_move(state, &ids, dx, dy);
            state.borrow_mut().history.undo.push(Op::MoveStrokes { ids, dx, dy });
        }
        Op::ReplaceStroke { old, new } => {
            let (db, page_id) = {
                let mut s = state.borrow_mut();
                s.strokes.retain(|st| st.id != old.id);
                for child in &new {
                    s.strokes.push(child.clone());
                }
                s.history.undo.push(Op::ReplaceStroke { old: old.clone(), new: new.clone() });
                (s.backend.clone(), s.current_page_id)
            };
            if let Some(pid) = page_id {
                if let Err(e) = db.borrow_mut().replace_stroke(old.id, &new, pid) {
                    tracing::warn!("redo ReplaceStroke: failed for {}: {}", old.id, e);
                }
            }
        }
        Op::TransformStrokes { ids, anchor_x, anchor_y, sx, sy } => {
            apply_scale(state, &ids, anchor_x, anchor_y, sx, sy);
            state.borrow_mut().history.undo.push(Op::TransformStrokes { ids, anchor_x, anchor_y, sx, sy });
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
        (s.backend.clone(), s.current_page_id, updated)
    };
    if let Some(pid) = page_id {
        for st in &updated {
            if let Err(e) = db.borrow_mut().update_stroke(st, pid) {
                tracing::warn!("apply_move update_stroke {}: {}", st.id, e);
            }
        }
    }
}

fn apply_scale(state: &SharedState, ids: &[Uuid], anchor_x: f64, anchor_y: f64, sx: f64, sy: f64) {
    let (db, page_id, updated) = {
        let mut s = state.borrow_mut();
        for st in s.strokes.iter_mut() {
            if ids.contains(&st.id) {
                for pt in st.points.iter_mut() {
                    pt.x = anchor_x + (pt.x - anchor_x) * sx;
                    pt.y = anchor_y + (pt.y - anchor_y) * sy;
                }
                let bb = &mut st.bounding_box;
                let new_x = anchor_x + (bb.x - anchor_x) * sx;
                let new_y = anchor_y + (bb.y - anchor_y) * sy;
                bb.width = (bb.width * sx.abs()).max(0.0);
                bb.height = (bb.height * sy.abs()).max(0.0);
                bb.x = new_x.min(new_x + bb.width);
                bb.y = new_y.min(new_y + bb.height);
            }
        }
        let updated: Vec<Stroke> = s.strokes.iter().filter(|st| ids.contains(&st.id)).cloned().collect();
        (s.backend.clone(), s.current_page_id, updated)
    };
    if let Some(pid) = page_id {
        for st in &updated {
            if let Err(e) = db.borrow_mut().update_stroke(st, pid) {
                tracing::warn!("apply_scale update_stroke {}: {}", st.id, e);
            }
        }
    }
}

/// Copy all currently selected strokes into the per-app stroke clipboard.
/// Does nothing when nothing is selected.
pub fn copy_selection(state: &SharedState) {
    let mut s = state.borrow_mut();
    if s.selected_stroke_ids.is_empty() {
        return;
    }
    let clipboard: Vec<Stroke> = s
        .strokes
        .iter()
        .filter(|st| s.selected_stroke_ids.contains(&st.id))
        .cloned()
        .collect();
    let count = clipboard.len();
    s.stroke_clipboard = clipboard;
    tracing::info!("copied {} stroke(s) to clipboard", count);
}

/// Paste strokes from the per-app clipboard onto the current page.
///
/// Each pasted stroke gets a fresh UUID and is offset by ~10 mm in both axes
/// so it doesn't land exactly on top of the original. The pasted strokes are
/// immediately selected so the user can drag them into position.
pub fn paste_clipboard(state: &SharedState, area: &impl IsA<gtk4::Widget>) {
    let area: &gtk4::Widget = &area.clone().upcast();
    const PASTE_OFFSET: f64 = 10.0; // canvas units (≈ mm for A4-sized pages)

    let (page_id, backend, clipboard) = {
        let s = state.borrow();
        let page_id = match s.current_page_id {
            Some(id) => id,
            None => return,
        };
        if s.stroke_clipboard.is_empty() {
            return;
        }
        (page_id, s.backend.clone(), s.stroke_clipboard.clone())
    };

    let mut new_strokes: Vec<Stroke> = Vec::with_capacity(clipboard.len());
    for src in &clipboard {
        let mut clone = src.clone();
        clone.id = Uuid::new_v4();
        // Offset all points.
        for pt in clone.points.iter_mut() {
            pt.x += PASTE_OFFSET;
            pt.y += PASTE_OFFSET;
        }
        // Offset bounding box.
        clone.bounding_box.x += PASTE_OFFSET;
        clone.bounding_box.y += PASTE_OFFSET;
        new_strokes.push(clone);
    }

    // Persist and update in-memory state.
    {
        let mut s = state.borrow_mut();
        s.selected_stroke_ids.clear();
        for st in &new_strokes {
            s.selected_stroke_ids.insert(st.id);
            s.strokes.push(st.clone());
            s.history.push_add(st.clone());
        }
    }

    for st in &new_strokes {
        if let Err(e) = backend.borrow_mut().insert_stroke(st, page_id) {
            tracing::error!("paste_clipboard: insert_stroke failed for {}: {}", st.id, e);
        }
    }

    tracing::info!("pasted {} stroke(s)", new_strokes.len());
    area.queue_draw();
}
