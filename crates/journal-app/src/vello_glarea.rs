//! Vello GLArea overlay (Phase 0 spike step 2, Path A per docs/renderer-vello-migration.md).
//!
//! Renders strokes via Vello (GPU compute on wgpu) into an offscreen RGBA
//! buffer, uploads the buffer to a GL texture, and presents it on the
//! GLArea via a fullscreen-quad shader. The GLArea sits in the canvas
//! overlay above the existing DrawingArea and is event-passthrough
//! (`set_can_target(false)`), so input still reaches the Cairo widget.
//!
//! Activated only when the `vello` Cargo feature is on AND the env var
//! `JOURNAL_VELLO=1` is set at runtime. Off → returns None and window.rs
//! adds nothing to the overlay, so the default Cairo path is unchanged.

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_void;
use std::rc::Rc;

use glow::HasContext;
use gtk4::prelude::*;
use gtk4::{glib, GLArea};
use journal_canvas::vello_renderer::VelloRenderer;
use libloading::{Library, Symbol};

use crate::state::SharedState;

pub fn enabled() -> bool {
    // Vello is the primary renderer when this feature is compiled. The
    // legacy `JOURNAL_VELLO=0` opt-out remains so the Cairo fallback can be
    // exercised for diagnostics; everything else defaults to on.
    !matches!(
        std::env::var("JOURNAL_VELLO").as_deref(),
        Ok("0") | Ok("false") | Ok("FALSE")
    )
}

pub fn build(state: SharedState) -> Option<GLArea> {
    if !enabled() {
        // JOURNAL_VELLO=0 → opt-out, fall back to the Cairo DrawingArea.
        return None;
    }
    let area = GLArea::builder()
        .hexpand(true)
        .vexpand(true)
        .has_depth_buffer(false)
        .has_stencil_buffer(false)
        .build();
    // GLArea owns input now (DrawingArea is no longer in the overlay tree),
    // so leave can_target at its default `true`.
    area.set_can_target(true);
    area.set_focusable(true);

    let gl_cell: Rc<RefCell<Option<glow::Context>>> = Rc::new(RefCell::new(None));
    let gl_state_cell: Rc<RefCell<Option<GlState>>> = Rc::new(RefCell::new(None));
    let vello_cell: Rc<RefCell<Option<VelloRenderer>>> = Rc::new(RefCell::new(None));
    let widgets_cell: Rc<RefCell<journal_widgets::WidgetRenderer>> =
        Rc::new(RefCell::new(journal_widgets::WidgetRenderer::new()));
    // Cached Vello scene of the page's template widgets (drawn at
    // canvas-space identity transform). Re-laying out parley text and
    // walking every widget every frame is the dominant CPU cost during
    // a stroke on widget-heavy pages (Daily Planner, Cornell, etc).
    // We rebuild only when the cache key (template widget ids + page
    // rect + bound date + dark mode + now-minute + overrides) changes,
    // and `Scene::append` composes the cached scene under the live
    // `world_to_screen` transform on every frame.
    let widgets_cache: Rc<RefCell<Option<(u64, journal_widgets::VelloScene)>>> =
        Rc::new(RefCell::new(None));

    {
        let gl_cell = gl_cell.clone();
        let gl_state_cell_realize = gl_state_cell.clone();
        area.connect_realize(move |area| {
            area.make_current();
            if let Some(err) = area.error() {
                tracing::error!("GLArea realize error: {err}");
                return;
            }
            // GTK can destroy + recreate the GL context (e.g. when the
            // canvas overlay is reparented across stack-page swaps), in
            // which case any GL objects we created against the old context
            // are stale. Drop them and reload the loader each realize.
            *gl_state_cell_realize.borrow_mut() = None;
            *gl_cell.borrow_mut() = None;
            match unsafe { load_gl_context() } {
                Ok(gl) => *gl_cell.borrow_mut() = Some(gl),
                Err(e) => tracing::error!("failed to load GL via libEGL: {e}"),
            }
        });
    }
    {
        let gl_cell = gl_cell.clone();
        let gl_state_cell = gl_state_cell.clone();
        area.connect_unrealize(move |_| {
            // Drop GL context + GL objects when GTK tears the context down.
            // Without this, the next realize would briefly try to use stale
            // resources and fall through to a blank frame.
            *gl_state_cell.borrow_mut() = None;
            *gl_cell.borrow_mut() = None;
        });
    }

    {
        let gl_cell = gl_cell.clone();
        let gl_state_cell = gl_state_cell.clone();
        let vello_cell = vello_cell.clone();
        let widgets_cell = widgets_cell.clone();
        let state = state.clone();
        area.connect_render(move |area, _ctx| {
            // Defensive: ensure GLArea's context is current. wgpu's Vulkan
            // backend doesn't touch GL state, but other libs in the process
            // may have done make_current on a different context between
            // frames.
            area.make_current();

            let gl_borrow = gl_cell.borrow();
            let Some(gl) = gl_borrow.as_ref() else {
                return glib::Propagation::Stop;
            };

            // ViewportTransform's screen_size is in logical pixels (set
            // by canvas_widget's DrawingArea draw_func), so render Vello at
            // logical and let GL upscale to the physical viewport. Avoids
            // strokes shifting on zoom from a coord-vs-texture mismatch.
            let scale = area.scale_factor().max(1);
            let w = area.width().max(1) as u32;
            let h = area.height().max(1) as u32;
            let phys_w = (w as i32 * scale) as u32;
            let phys_h = (h as i32 * scale) as u32;

            // Update transform's screen size now that the GLArea drives all
            // canvas rendering (no DrawingArea draw_func sets it). Then
            // snapshot the immutable state pieces the renderer needs.
            {
                let mut s = state.borrow_mut();
                s.transform.set_size(w as f64, h as f64);
            }

            // No page selected → render placeholder scene (cream/dark fill +
            // centered prompt text) and short-circuit the bg/widgets/strokes
            // pipeline.
            let placeholder_info = {
                let s = state.borrow();
                if s.current_page_id.is_none() {
                    Some((crate::is_dark_mode(), s.placeholder_text.clone()))
                } else {
                    None
                }
            };
            if let Some((dark_mode, text)) = placeholder_info {
                if vello_cell.borrow().is_none() {
                    match VelloRenderer::new() {
                        Ok(r) => *vello_cell.borrow_mut() = Some(r),
                        Err(e) => {
                            tracing::error!("VelloRenderer init failed: {e}");
                            return glib::Propagation::Stop;
                        }
                    }
                }
                let widgets_cell_p = widgets_cell.clone();
                let rgba = match vello_cell
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .render_placeholder(w, h, dark_mode, move |scene, sw, sh| {
                        widgets_cell_p
                            .borrow_mut()
                            .draw_placeholder(scene, sw as f64, sh as f64, dark_mode, &text);
                    }) {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!("Vello placeholder render: {e}");
                        return glib::Propagation::Stop;
                    }
                };
                if let Err(e) = ensure_gl_state(gl, &mut gl_state_cell.borrow_mut(), w, h) {
                    tracing::error!("GL init: {e}");
                    return glib::Propagation::Stop;
                }
                unsafe {
                    gl.viewport(0, 0, phys_w as i32, phys_h as i32);
                    gl.clear_color(0.0, 0.0, 0.0, 0.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                    gl.enable(glow::BLEND);
                    gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

                    let gs = gl_state_cell.borrow();
                    let gs = gs.as_ref().unwrap();
                    gl.use_program(Some(gs.program));
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(gs.texture));
                    gl.tex_sub_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        0,
                        0,
                        w as i32,
                        h as i32,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(&rgba)),
                    );
                    gl.bind_vertex_array(Some(gs.vao));
                    gl.draw_arrays(glow::TRIANGLES, 0, 6);
                }
                return glib::Propagation::Stop;
            }

            let (
                transform,
                strokes,
                background,
                page_rect,
                selected_ids,
                widgets,
                widget_ctx,
                overlays,
                brush_params,
            ) = {
                let s = state.borrow();
                let mut frame: Vec<journal_core::Stroke> = s.strokes.clone();
                if let Some(cs) = s.current_stroke.clone() {
                    frame.push(cs);
                }
                let bg = journal_canvas::scale_background(&s.background, s.bg_scale);
                let widgets: Vec<journal_core::TemplateWidget> = s
                    .current_template
                    .as_ref()
                    .map(|t| t.widgets.clone())
                    .unwrap_or_default();
                let widget_ctx = journal_widgets::WidgetRenderContext {
                    date: s.current_page_date,
                    overrides: s.current_page_overrides.clone(),
                };
                let selection_bbox = if s.selected_stroke_ids.is_empty() {
                    None
                } else {
                    journal_canvas::selection_combined_bbox(&s.strokes, &s.selected_stroke_ids)
                };
                let cursor_radius = compute_cursor_radius(&s);
                let (cursor_shape, cursor_tip) = match s.active_brush_recipe.as_ref() {
                    Some(b) => (
                        Some(b.cursor.clone()),
                        b.layers.first().map(|l| l.tip.clone()),
                    ),
                    None => (None, None),
                };
                // Page-change fade-in: ramp opacity from 0 → 1 over
                // PAGE_FADE_MS after `set_current_page` stamps a new
                // transition_started_at.
                const PAGE_FADE_MS: f32 = 180.0;
                let fade_alpha = match s.page_transition_started_at {
                    Some(start) => {
                        let elapsed = start.elapsed().as_secs_f32() * 1000.0;
                        (elapsed / PAGE_FADE_MS).clamp(0.0, 1.0)
                    }
                    None => 1.0,
                };
                let overlays = journal_canvas::vello_renderer::OverlayState {
                    selection_bbox,
                    lasso_screen_points: s.lasso_points.clone(),
                    pointer_screen: s.pointer_screen,
                    pointer_drawing: s.pointer_drawing,
                    cursor_radius,
                    cursor_color: s.pen.color,
                    cursor_opacity: s.pen.opacity,
                    show_page_bounds: s.show_page_bounds,
                    dark_mode: crate::is_dark_mode(),
                    cursor_shape,
                    cursor_tip,
                    fade_alpha,
                };
                (
                    s.transform,
                    frame,
                    bg,
                    s.page_rect,
                    s.selected_stroke_ids.clone(),
                    widgets,
                    widget_ctx,
                    overlays,
                    s.brush_params,
                )
            };

            // Lazy init Vello renderer.
            if vello_cell.borrow().is_none() {
                match VelloRenderer::new() {
                    Ok(r) => *vello_cell.borrow_mut() = Some(r),
                    Err(e) => {
                        tracing::error!("VelloRenderer init failed: {e}");
                        return glib::Propagation::Stop;
                    }
                }
            }

            let widget_key = widget_cache_key(
                &widgets,
                &widget_ctx,
                page_rect,
                overlays.dark_mode,
            );
            let widgets_cache_inner = widgets_cache.clone();
            let widgets_cell_inner = widgets_cell.clone();

            // Render Vello scene to RGBA8.
            let rgba = match vello_cell.borrow_mut().as_mut().unwrap().render_rgba(
                &transform,
                &background,
                page_rect,
                &strokes,
                &selected_ids,
                &overlays,
                &brush_params,
                w,
                h,
                |scene, world_to_screen, pr| {
                    // Cache hit: append the pre-built widget scene under
                    // the live world-to-screen transform.
                    let mut cache = widgets_cache_inner.borrow_mut();
                    if let Some((k, cached)) = cache.as_ref() {
                        if *k == widget_key {
                            scene.append(cached, Some(world_to_screen));
                            return;
                        }
                    }
                    // Cache miss: rebuild the widget scene at IDENTITY
                    // (canvas space) so it can be re-appended under any
                    // world_to_screen on later frames.
                    let mut fresh = journal_widgets::VelloScene::new();
                    widgets_cell_inner.borrow_mut().draw_widgets(
                        &mut fresh,
                        journal_widgets::VelloAffine::IDENTITY,
                        &widgets,
                        pr,
                        &widget_ctx,
                    );
                    scene.append(&fresh, Some(world_to_screen));
                    *cache = Some((widget_key, fresh));
                },
            ) {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("Vello render: {e}");
                    return glib::Propagation::Stop;
                }
            };

            // Lazy init / resize GL state.
            if let Err(e) = ensure_gl_state(gl, &mut gl_state_cell.borrow_mut(), w, h) {
                tracing::error!("GL init: {e}");
                return glib::Propagation::Stop;
            }

            unsafe {
                gl.viewport(0, 0, phys_w as i32, phys_h as i32);
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

                let gs = gl_state_cell.borrow();
                let gs = gs.as_ref().unwrap();
                gl.use_program(Some(gs.program));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(gs.texture));
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    w as i32,
                    h as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&rgba)),
                );
                gl.bind_vertex_array(Some(gs.vao));
                gl.draw_arrays(glow::TRIANGLES, 0, 6);
            }

            glib::Propagation::Stop
        });
    }

    // Tick callback: queue_render only when canvas state has changed since
    // the last frame. Avoids burning a wgpu render+readback every 16ms when
    // the user is idle. Production should signal state changes explicitly;
    // this fingerprint is the spike-grade equivalent.
    let last_fp: Rc<RefCell<u64>> = Rc::new(RefCell::new(u64::MAX));
    {
        let area_weak = area.downgrade();
        let state = state.clone();
        let last_fp = last_fp.clone();
        area.add_tick_callback(move |_area, _clock| {
            let fp = {
                let s = state.borrow();
                state_fingerprint(&s)
            };
            if *last_fp.borrow() != fp {
                *last_fp.borrow_mut() = fp;
                if let Some(a) = area_weak.upgrade() {
                    a.queue_render();
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // When the canvas is re-shown after being unmapped (e.g. user switched
    // away to home and back into a different notebook), the tick callback's
    // last fingerprint can match the new visible state purely by accident,
    // suppressing the redraw. Forcing a queue_render + fingerprint reset on
    // (re-)map guarantees the first frame after visibility change paints.
    {
        let area_weak = area.downgrade();
        let last_fp = last_fp.clone();
        area.connect_map(move |_| {
            *last_fp.borrow_mut() = u64::MAX;
            if let Some(a) = area_weak.upgrade() {
                a.queue_render();
            }
        });
    }

    // Page-change fade ticker: while a transition is active, force a
    // render every frame so the alpha ramp completes; clear the
    // transition stamp once the fade has run its course so the ticker
    // stops queue_render'ing on idle.
    {
        let state = state.clone();
        let area_weak = area.downgrade();
        const PAGE_FADE_MS_TICK: f32 = 180.0;
        area.add_tick_callback(move |_, _| {
            // Cheap immutable borrow first — only escalate to a mut borrow
            // when there's actually a transition to clear, so the tick
            // doesn't contend with input-handler borrows on every frame.
            let start_opt = state.borrow().page_transition_started_at;
            let Some(start) = start_opt else {
                return gtk4::glib::ControlFlow::Continue;
            };
            let elapsed = start.elapsed().as_secs_f32() * 1000.0;
            if elapsed >= PAGE_FADE_MS_TICK {
                state.borrow_mut().page_transition_started_at = None;
            }
            if let Some(a) = area_weak.upgrade() {
                a.queue_render();
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    Some(area)
}

struct GlState {
    program: glow::Program,
    vao: glow::VertexArray,
    _vbo: glow::Buffer,
    texture: glow::Texture,
    tex_w: u32,
    tex_h: u32,
}

fn ensure_gl_state(
    gl: &glow::Context,
    cell: &mut Option<GlState>,
    w: u32,
    h: u32,
) -> Result<(), String> {
    if let Some(gs) = cell.as_mut() {
        if gs.tex_w != w || gs.tex_h != h {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(gs.texture));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w as i32,
                    h as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
            gs.tex_w = w;
            gs.tex_h = h;
        }
        return Ok(());
    }

    unsafe {
        let program = compile_program(gl)?;
        let vao = gl
            .create_vertex_array()
            .map_err(|e| format!("create_vertex_array: {e}"))?;
        let vbo = gl
            .create_buffer()
            .map_err(|e| format!("create_buffer: {e}"))?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        // Two triangles covering NDC [-1,1]^2.
        let verts: [f32; 12] = [
            -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0,
        ];
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck_slice(&verts),
            glow::STATIC_DRAW,
        );
        let pos_attr = gl
            .get_attrib_location(program, "a_pos")
            .ok_or("a_pos attrib")?;
        gl.enable_vertex_attrib_array(pos_attr);
        gl.vertex_attrib_pointer_f32(pos_attr, 2, glow::FLOAT, false, 8, 0);
        let tex_loc = gl.get_uniform_location(program, "u_tex");
        gl.use_program(Some(program));
        gl.uniform_1_i32(tex_loc.as_ref(), 0);

        let texture = gl
            .create_texture()
            .map_err(|e| format!("create_texture: {e}"))?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w as i32,
            h as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );

        *cell = Some(GlState {
            program,
            vao,
            _vbo: vbo,
            texture,
            tex_w: w,
            tex_h: h,
        });
    }
    Ok(())
}

unsafe fn compile_program(gl: &glow::Context) -> Result<glow::Program, String> {
    // GTK4 GLArea on Mesa/Wayland gives a GLES 3 context, not desktop GL.
    // Use GLSL ES 300 — same `in`/`out`/`texture()` syntax as 3.30 core, but
    // requires `precision` in fragment and the explicit `es` profile tag.
    const VS: &str = r#"#version 300 es
in vec2 a_pos;
out vec2 v_uv;
void main() {
    vec2 uv = a_pos * 0.5 + 0.5;
    uv.y = 1.0 - uv.y;
    v_uv = uv;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;
    const FS: &str = r#"#version 300 es
precision mediump float;
in vec2 v_uv;
out vec4 frag;
uniform sampler2D u_tex;
void main() {
    frag = texture(u_tex, v_uv);
}
"#;

    let program = gl
        .create_program()
        .map_err(|e| format!("create_program: {e}"))?;
    let vs = compile_shader(gl, glow::VERTEX_SHADER, VS)?;
    let fs = compile_shader(gl, glow::FRAGMENT_SHADER, FS)?;
    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        return Err(format!("link: {}", gl.get_program_info_log(program)));
    }
    gl.detach_shader(program, vs);
    gl.detach_shader(program, fs);
    gl.delete_shader(vs);
    gl.delete_shader(fs);
    Ok(program)
}

unsafe fn compile_shader(gl: &glow::Context, kind: u32, src: &str) -> Result<glow::Shader, String> {
    let shader = gl
        .create_shader(kind)
        .map_err(|e| format!("create_shader: {e}"))?;
    gl.shader_source(shader, src);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        return Err(format!(
            "compile shader (kind {kind:#x}): {}",
            gl.get_shader_info_log(shader)
        ));
    }
    Ok(shader)
}

fn compute_cursor_radius(s: &crate::state::CanvasState) -> f64 {
    use crate::state::{tool_brush_params, tool_is_drawing, EraserMode, Tool};
    if tool_is_drawing(s.tool) {
        let (_, mult, _, _) = tool_brush_params(s, s.tool);
        (s.pen.base_width * mult * 0.5).max(2.0)
    } else {
        match s.tool {
            Tool::Eraser(EraserMode::Stroke) => 6.0,
            Tool::Eraser(EraserMode::Partial) => 11.0,
            _ => 5.0,
        }
    }
}

/// Hash of the bits that affect template-widget rendering output. When
/// this matches the cached value, the widget Scene from last frame is
/// re-appended under the live transform instead of rebuilt from scratch
/// — saves the parley layout work on every stroke point.
///
/// Including `now_minute()` when any widget cares about "today" means
/// the cache invalidates once per minute on a page bound to today's
/// date; one parley rebuild per minute is a fraction of the cost of
/// rebuilding every frame at 60Hz.
fn widget_cache_key(
    widgets: &[journal_core::TemplateWidget],
    ctx: &journal_widgets::WidgetRenderContext,
    page_rect: journal_core::Rect,
    dark_mode: bool,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    widgets.len().hash(&mut h);
    for w in widgets {
        w.id.as_u128().hash(&mut h);
        std::mem::discriminant(&w.kind).hash(&mut h);
        // Hash the rect so widget moves invalidate.
        w.rect.x.to_bits().hash(&mut h);
        w.rect.y.to_bits().hash(&mut h);
        w.rect.width.to_bits().hash(&mut h);
        w.rect.height.to_bits().hash(&mut h);
        // Hash variant bytes for kinds whose payload changes
        // rendering output (text, item count, hour ranges).
        format!("{:?}", w.kind).hash(&mut h);
    }
    if let Some(d) = ctx.date {
        use chrono::Datelike;
        d.year().hash(&mut h);
        d.month().hash(&mut h);
        d.day().hash(&mut h);
    } else {
        0u32.hash(&mut h);
    }
    ctx.overrides.len().hash(&mut h);
    for (id, ov) in &ctx.overrides {
        id.as_u128().hash(&mut h);
        format!("{:?}", ov).hash(&mut h);
    }
    page_rect.x.to_bits().hash(&mut h);
    page_rect.y.to_bits().hash(&mut h);
    page_rect.width.to_bits().hash(&mut h);
    page_rect.height.to_bits().hash(&mut h);
    dark_mode.hash(&mut h);
    // "Now"-line position: invalidate once per minute when widgets
    // could care (Timeline / DailyAppointments / CalendarMonth on
    // today's page).
    let needs_now = ctx
        .date
        .is_some_and(|d| d == chrono::Local::now().date_naive());
    if needs_now {
        let now = chrono::Local::now();
        use chrono::Timelike;
        (now.hour() * 60 + now.minute()).hash(&mut h);
    }
    h.finish()
}

/// Cheap hash of the bits the renderer cares about. If this doesn't change,
/// the on-screen image won't change either, so we skip queuing a redraw.
fn state_fingerprint(s: &crate::state::CanvasState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.strokes.len().hash(&mut h);
    if let Some(last) = s.strokes.last() {
        last.id.as_u128().hash(&mut h);
        last.points.len().hash(&mut h);
    }
    if let Some(cs) = &s.current_stroke {
        true.hash(&mut h);
        cs.points.len().hash(&mut h);
    } else {
        false.hash(&mut h);
    }
    let v = s.transform.viewport();
    v.zoom.to_bits().hash(&mut h);
    v.center.x.to_bits().hash(&mut h);
    v.center.y.to_bits().hash(&mut h);
    let (sw, sh) = s.transform.screen_size();
    sw.to_bits().hash(&mut h);
    sh.to_bits().hash(&mut h);
    s.bg_scale.to_bits().hash(&mut h);
    crate::is_dark_mode().hash(&mut h);
    s.selected_stroke_ids.len().hash(&mut h);
    for id in &s.selected_stroke_ids {
        id.as_u128().hash(&mut h);
    }
    if let Some(pid) = s.current_page_id {
        pid.0.as_u128().hash(&mut h);
    } else {
        0u128.hash(&mut h);
    }
    if let Some(t) = &s.current_template {
        t.id.0.as_u128().hash(&mut h);
        t.widgets.len().hash(&mut h);
    } else {
        0u128.hash(&mut h);
    }
    s.current_page_overrides.len().hash(&mut h);
    s.lasso_points.len().hash(&mut h);
    if let Some(last) = s.lasso_points.last() {
        last.0.to_bits().hash(&mut h);
        last.1.to_bits().hash(&mut h);
    }
    if let Some((px, py)) = s.pointer_screen {
        px.to_bits().hash(&mut h);
        py.to_bits().hash(&mut h);
    }
    s.pointer_drawing.hash(&mut h);
    s.show_page_bounds.hash(&mut h);
    s.pen.base_width.to_bits().hash(&mut h);
    s.pen.opacity.to_bits().hash(&mut h);
    s.pen.color.r.hash(&mut h);
    s.pen.color.g.hash(&mut h);
    s.pen.color.b.hash(&mut h);
    s.pen.color.a.hash(&mut h);
    std::mem::discriminant(&s.tool).hash(&mut h);
    s.page_rect.x.to_bits().hash(&mut h);
    s.page_rect.y.to_bits().hash(&mut h);
    s.page_rect.width.to_bits().hash(&mut h);
    s.page_rect.height.to_bits().hash(&mut h);
    bg_fingerprint(&s.background, &mut h);
    h.finish()
}

fn bg_fingerprint<H: std::hash::Hasher>(bg: &journal_canvas::BackgroundConfig, h: &mut H) {
    use journal_canvas::BackgroundConfig as B;
    use std::hash::Hash;
    match bg {
        B::Blank => 0u8.hash(h),
        B::Dots { spacing, tiling } => {
            1u8.hash(h);
            spacing.to_bits().hash(h);
            tiling.hash(h);
        }
        B::Lines { spacing, tiling } => {
            2u8.hash(h);
            spacing.to_bits().hash(h);
            tiling.hash(h);
        }
        B::Grid(g) => {
            3u8.hash(h);
            g.base_spacing.to_bits().hash(h);
            g.subdivisions.hash(h);
            g.color.r.hash(h);
            g.color.g.hash(h);
            g.color.b.hash(h);
            g.color.a.hash(h);
        }
        B::Isometric { spacing } => {
            4u8.hash(h);
            spacing.to_bits().hash(h);
        }
        B::Hexagonal { spacing } => {
            5u8.hash(h);
            spacing.to_bits().hash(h);
        }
        B::Image { path, size_canvas } => {
            6u8.hash(h);
            path.hash(h);
            size_canvas.0.to_bits().hash(h);
            size_canvas.1.to_bits().hash(h);
        }
        B::Pdf {
            path,
            page,
            size_canvas,
        } => {
            7u8.hash(h);
            path.hash(h);
            page.hash(h);
            size_canvas.0.to_bits().hash(h);
            size_canvas.1.to_bits().hash(h);
        }
    }
}

fn bytemuck_slice(verts: &[f32]) -> &[u8] {
    let len = std::mem::size_of_val(verts);
    unsafe { std::slice::from_raw_parts(verts.as_ptr() as *const u8, len) }
}

unsafe fn load_gl_context() -> Result<glow::Context, String> {
    // GTK4 on Wayland uses EGL. eglGetProcAddress is a real exported function
    // (libepoxy's epoxy_eglGetProcAddress is a dispatch-table data symbol, not
    // callable via dlsym). dlopen libEGL and resolve via eglGetProcAddress.
    let lib = Library::new("libEGL.so.1")
        .or_else(|_| Library::new("libEGL.so"))
        .map_err(|e| format!("dlopen libEGL: {e}"))?;
    let lib: &'static Library = Box::leak(Box::new(lib));
    let egl_get_proc: Symbol<'static, unsafe extern "C" fn(*const i8) -> *const c_void> = lib
        .get(b"eglGetProcAddress\0")
        .map_err(|e| format!("dlsym eglGetProcAddress: {e}"))?;
    let egl_get_proc = *egl_get_proc;
    Ok(glow::Context::from_loader_function(move |name| {
        let cname = match CString::new(name) {
            Ok(c) => c,
            Err(_) => return std::ptr::null(),
        };
        egl_get_proc(cname.as_ptr()) as *const _
    }))
}
