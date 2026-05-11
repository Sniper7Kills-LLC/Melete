//! Floating "Tool options" panel.
//!
//! When developer mode is on (`AppConfig::developer_mode = true` or env
//! `MELETE_DEV=1`), a separate top-level window appears alongside the
//! main app window showing the currently-selected tool's full settings —
//! default size, opacity / width multipliers, blend mode, brush-style
//! override, and the per-brush-style internal tuning that matches the
//! tool's current brush_style. The window watches `state.tool` via a
//! frame-clock tick callback and rebuilds its content whenever the user
//! switches tools.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, DropDown, Label, Orientation, ScrolledWindow, Separator,
    SpinButton, Stack, StackTransitionType, StringList, Window,
};
use melete_canvas::vello_renderer::{
    CalligraphyParams, CalligraphyShape, PaintbrushParams, PaintbrushShape, PenParams, PenShape,
    PencilParams, PencilShape, SprayParams, SprayShape, ToolStyleParams,
};
use melete_core::{BlendMode, ToolStyle};

use crate::state::{SharedState, Tool};
use crate::tool_settings::{
    default_settings_for, settable_tools, tool_key, tool_label, ToolSettings,
};

const BLEND_MODES: &[(&str, BlendMode)] = &[
    ("Normal", BlendMode::Normal),
    ("Multiply", BlendMode::Multiply),
    ("Screen", BlendMode::Screen),
    ("Overlay", BlendMode::Overlay),
    ("Darken", BlendMode::Darken),
    ("Lighten", BlendMode::Lighten),
    ("Erase", BlendMode::Erase),
];

fn blend_index(b: BlendMode) -> u32 {
    BLEND_MODES.iter().position(|(_, m)| *m == b).unwrap_or(0) as u32
}

/// Owns everything needed to show the Tool Options either as a floating
/// `Window` or docked into the right side of the canvas. The whole
/// outer box (including the dock toggle) gets reparented between the
/// floating window and the dock slot when the user toggles docking.
pub struct ToolOptionsPanel {
    pub window: Window,
    pub dock_slot: GtkBox,
    pub outer: GtkBox,
    pub docked: Rc<Cell<bool>>,
    /// Caller-controlled desired-visibility flag. The view-change
    /// listener in `window.rs` flips this with `show()` / `hide()`;
    /// the dock toggle reads it inside `set_docked` so reparenting
    /// doesn't force the panel back into view on Home / template
    /// editor screens.
    wanted_visible: Rc<Cell<bool>>,
}

pub fn build_tool_options_panel(
    parent: &gtk4::ApplicationWindow,
    state: SharedState,
    dock_slot: GtkBox,
    tools_open: Rc<RefCell<Option<Rc<dyn Fn(Option<melete_core::Brush>)>>>>,
) -> Rc<ToolOptionsPanel> {
    let win = Window::builder()
        .transient_for(parent)
        .destroy_with_parent(true)
        .title("Tool options")
        .default_width(340)
        .default_height(540)
        .build();

    let outer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(0)
        .build();

    // Top header: dock toggle. Persists across tool changes since it
    // sits outside the rebuilt body.
    let toggle_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();
    toggle_row.append(
        &Label::builder()
            .label("Dock to right side")
            .hexpand(true)
            .xalign(0.0)
            .build(),
    );
    let dock_chk = CheckButton::new();
    dock_chk.set_active(crate::config::load().tool_options_docked);
    toggle_row.append(&dock_chk);
    outer.append(&toggle_row);
    outer.append(&Separator::new(Orientation::Horizontal));

    let scroll = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(12)
        .margin_end(12)
        .build();

    // Three-zone layout so tool switches don't destroy widgets that are
    // identical across tools (audit §2): the brush-style internals are
    // editor-equivalent for every tool that maps to the same ToolStyle,
    // so they live in a Stack built once at panel creation. The dynamic
    // top/bottom zones still get cleared and rebuilt per tool, but the
    // expensive grids of spinners stay alive.
    let top_dyn = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .build();
    let internals_stack = Stack::builder()
        .transition_type(StackTransitionType::Crossfade)
        .transition_duration(140)
        .hhomogeneous(true)
        .vhomogeneous(false)
        .build();
    let bottom_dyn = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .build();

    body.append(&top_dyn);
    body.append(&internals_stack);
    body.append(&bottom_dyn);

    populate_internals_stack(&internals_stack, &state);

    scroll.set_child(Some(&body));
    outer.append(&scroll);

    let docked = Rc::new(Cell::new(crate::config::load().tool_options_docked));

    // Initial parent based on persisted preference.
    if docked.get() {
        dock_slot.append(&outer);
        dock_slot.set_visible(true);
    } else {
        win.set_child(Some(&outer));
    }

    let initial_tool = state.borrow().tool;
    rebuild_for_tool(
        &top_dyn,
        &bottom_dyn,
        &internals_stack,
        &scroll,
        &state,
        initial_tool,
        tools_open.clone(),
    );

    let last_tool: Rc<Cell<Tool>> = Rc::new(Cell::new(initial_tool));
    {
        let top_dyn = top_dyn.clone();
        let bottom_dyn = bottom_dyn.clone();
        let internals_stack = internals_stack.clone();
        let scroll = scroll.clone();
        let state = state.clone();
        let last_tool = last_tool.clone();
        let tools_open = tools_open.clone();
        outer.add_tick_callback(move |_, _| {
            let cur = state.borrow().tool;
            if std::mem::discriminant(&cur) != std::mem::discriminant(&last_tool.get()) {
                last_tool.set(cur);
                rebuild_for_tool(
                    &top_dyn,
                    &bottom_dyn,
                    &internals_stack,
                    &scroll,
                    &state,
                    cur,
                    tools_open.clone(),
                );
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    // Don't let closing the window via the system X destroy it —
    // hide instead. Otherwise the next dock-toggle would try to
    // reparent the outer box out of a destroyed window, leaving
    // an empty dock slot.
    {
        let outer_w = outer.clone();
        win.connect_close_request(move |w| {
            w.set_visible(false);
            // Move outer somewhere safe so a stray destroy doesn't
            // take it with the window. We re-attach on next show().
            if outer_w.parent().is_some() {
                outer_w.unparent();
            }
            gtk4::glib::Propagation::Stop
        });
    }

    let panel = Rc::new(ToolOptionsPanel {
        window: win,
        dock_slot,
        outer,
        docked,
        wanted_visible: Rc::new(Cell::new(false)),
    });

    // Wire the toggle now that we have the panel in Rc form.
    {
        let weak = Rc::downgrade(&panel);
        dock_chk.connect_toggled(move |c| {
            if let Some(p) = weak.upgrade() {
                p.set_docked(c.is_active());
            }
        });
    }

    panel
}

impl ToolOptionsPanel {
    pub fn show(&self) {
        self.wanted_visible.set(true);
        self.apply_visibility();
    }

    pub fn hide(&self) {
        self.wanted_visible.set(false);
        self.apply_visibility();
    }

    /// Move the outer container between dock slot and floating window.
    pub fn set_docked(&self, on: bool) {
        if self.docked.get() == on {
            return;
        }
        self.docked.set(on);

        // Cleanly detach outer from whichever side currently owns it.
        // GtkWindow's `set_child(None)` releases the child without
        // ambiguity; `Box::remove` is the dock_slot equivalent. After
        // both, `outer` is parentless and safe to attach to the new
        // home regardless of which path it came from.
        self.window.set_child(None::<&gtk4::Widget>);
        if let Some(parent) = self.outer.parent() {
            if let Ok(b) = parent.downcast::<GtkBox>() {
                b.remove(&self.outer);
            } else {
                self.outer.unparent();
            }
        }
        if on {
            self.dock_slot.append(&self.outer);
        } else {
            self.window.set_child(Some(&self.outer));
        }

        let mut cfg = crate::config::load();
        cfg.tool_options_docked = on;
        if let Err(e) = crate::config::save(&cfg) {
            tracing::warn!("save tool_options_docked: {e}");
        }

        self.apply_visibility();
    }

    fn apply_visibility(&self) {
        let want = self.wanted_visible.get();
        if !want {
            self.window.set_visible(false);
            self.dock_slot.set_visible(false);
            return;
        }
        // Defensive reparent — ensure `outer` lives in the right
        // container before flipping visibility. Covers the case where
        // the close-X handler unparented outer or the window was
        // recycled across show/hide cycles.
        if self.docked.get() {
            if self.outer.parent().is_none() {
                self.dock_slot.append(&self.outer);
            }
            self.dock_slot.set_visible(true);
            self.window.set_visible(false);
        } else {
            if self.outer.parent().is_none() {
                self.window.set_child(Some(&self.outer));
            }
            self.dock_slot.set_visible(false);
            self.window.present();
        }
    }
}

/// Back-compat shim — returns the floating window. Docking unavailable
/// when called via this entry point.
#[allow(dead_code)]
pub fn build_tool_options_window(parent: &gtk4::ApplicationWindow, state: SharedState) -> Window {
    let dock_slot = GtkBox::builder().orientation(Orientation::Vertical).build();
    // Stub closure cell — back-compat callers don't have a Tool
    // Editor opener. The "Open in Tool Editor" button no-ops in
    // this entry point.
    let tools_open: Rc<RefCell<Option<Rc<dyn Fn(Option<melete_core::Brush>)>>>> =
        Rc::new(RefCell::new(None));
    let panel = build_tool_options_panel(parent, state, dock_slot, tools_open);
    panel.window.clone()
}

fn rebuild_for_tool(
    top: &GtkBox,
    bottom: &GtkBox,
    internals_stack: &Stack,
    scroll: &ScrolledWindow,
    state: &SharedState,
    tool: Tool,
    tools_open: Rc<RefCell<Option<Rc<dyn Fn(Option<melete_core::Brush>)>>>>,
) {
    // Snapshot scroll position so a tool change doesn't kick the user
    // back to the top of the popup (audit §2). Restored on the next
    // idle tick once the dynamic zones have re-laid out.
    let scroll_y = scroll.vadjustment().value();

    while let Some(c) = top.first_child() {
        top.remove(&c);
    }
    while let Some(c) = bottom.first_child() {
        bottom.remove(&c);
    }

    let Some(_key) = tool_key(tool) else {
        internals_stack.set_visible(false);
        let lbl = Label::builder()
            .label("This tool has no editable options.")
            .wrap(true)
            .xalign(0.0)
            .build();
        top.append(&lbl);
        return;
    };

    let header = Label::builder()
        .label(format!("<b>{}</b>", tool_label(tool)))
        .use_markup(true)
        .xalign(0.0)
        .build();
    top.append(&header);

    // Composable-brush summary + "Open in Tool Editor" link. Sits at
    // the top of the popup so dev-mode users can always reach the
    // full editor regardless of which tool is active. See
    // docs/brush-engine.md §Phase-4 / §4.5.
    add_brush_recipe_section(top, state, tool, tools_open);

    add_preset_picker(top, state, tool);
    add_tool_settings_section(top, state, tool);

    top.append(&Separator::new(Orientation::Horizontal));
    add_palette_section(top, state, tool);

    top.append(&Separator::new(Orientation::Horizontal));

    // Brush internals: lazily populate the becoming-active page from
    // current state, then crossfade to it. Inactive pages stay alive
    // but empty — populated next time their style is selected. Audit
    // §2: avoids the body-wide rebuild flash on every tool switch.
    let style = state
        .borrow()
        .tool_settings
        .get(tool_key(tool).unwrap_or(""))
        .map(|s| s.brush_style)
        .unwrap_or(default_settings_for(tool).brush_style);
    repopulate_internals_page(internals_stack, state, style);
    internals_stack.set_visible(true);
    internals_stack.set_visible_child_name(brush_style_stack_name(style));

    bottom.append(&Separator::new(Orientation::Horizontal));

    // Quick "jump to tool list" links so the panel stays useful when the
    // user has many tools open and wants to switch without going back to
    // the toolbar (panel is itself a separate window).
    let switch_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .build();
    switch_row.append(
        &Label::builder()
            .label("Switch tool:")
            .xalign(0.0)
            .hexpand(false)
            .build(),
    );
    for t in settable_tools().iter().copied() {
        if std::mem::discriminant(&t) == std::mem::discriminant(&tool) {
            continue;
        }
        let b = Button::with_label(tool_label(t));
        let state_c = state.clone();
        b.connect_clicked(move |_| {
            crate::state::set_tool(&state_c, t);
        });
        switch_row.append(&b);
    }
    bottom.append(&switch_row);

    // Restore scroll position after layout settles.
    {
        let scroll = scroll.clone();
        gtk4::glib::idle_add_local_once(move || {
            let adj = scroll.vadjustment();
            let clamped = scroll_y.clamp(
                adj.lower(),
                (adj.upper() - adj.page_size()).max(adj.lower()),
            );
            adj.set_value(clamped);
        });
    }
}

fn brush_style_stack_name(style: ToolStyle) -> &'static str {
    match style {
        ToolStyle::Pen => "pen",
        ToolStyle::Pencil => "pencil",
        ToolStyle::Highlighter => "highlighter",
        ToolStyle::Paintbrush => "paintbrush",
        ToolStyle::SprayCan => "spray",
        ToolStyle::Calligraphy => "calligraphy",
    }
}

/// Add an empty container page for each brush style to `stack`. Pages
/// are populated lazily by `repopulate_active_internals_page` whenever a
/// tool switch makes that page visible — this keeps the Stack
/// crossfade transition (so the pop-rebuild flash audited in §2 is
/// hidden) without the stale-widget/state desync that comes with
/// permanently caching every spinner.
fn populate_internals_stack(stack: &Stack, _state: &SharedState) {
    for name in [
        "pen",
        "highlighter",
        "pencil",
        "paintbrush",
        "spray",
        "calligraphy",
    ] {
        let page = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .build();
        stack.add_named(&page, Some(name));
    }
}

fn repopulate_internals_page(stack: &Stack, state: &SharedState, style: ToolStyle) {
    let name = brush_style_stack_name(style);
    let Some(page_widget) = stack.child_by_name(name) else {
        return;
    };
    let page: GtkBox = match page_widget.downcast::<GtkBox>() {
        Ok(b) => b,
        Err(_) => return,
    };
    while let Some(c) = page.first_child() {
        page.remove(&c);
    }

    let title = match style {
        ToolStyle::Pen => "Pen",
        ToolStyle::Highlighter => "Highlighter",
        ToolStyle::Pencil => "Pencil",
        ToolStyle::Paintbrush => "Paintbrush",
        ToolStyle::SprayCan => "Spray Can",
        ToolStyle::Calligraphy => "Calligraphy",
    };
    let header = Label::builder()
        .label(format!("<b>{} internals</b>", title))
        .use_markup(true)
        .xalign(0.0)
        .build();
    page.append(&header);
    page.append(
        &Label::builder()
            .label(
                "Shared globally per brush style — editing these affects every stroke that \
                 uses this style.",
            )
            .wrap(true)
            .xalign(0.0)
            .build(),
    );
    match style {
        ToolStyle::Pen | ToolStyle::Highlighter => append_pen_internals(&page, state),
        ToolStyle::Pencil => append_pencil_internals(&page, state),
        ToolStyle::Paintbrush => append_paintbrush_internals(&page, state),
        ToolStyle::SprayCan => append_spray_internals(&page, state),
        ToolStyle::Calligraphy => append_calligraphy_internals(&page, state),
    }
}

/// Compact composable-brush summary for the dev-mode popup. Shows
/// the active tool's brush recipe (built-in or assigned custom) +
/// per-layer one-liners + a button that opens the full Tool Editor
/// seeded with that brush. Edits are not made here — the popup is a
/// while-drawing surface, the full editor is for designing.
fn add_brush_recipe_section(
    body: &GtkBox,
    state: &SharedState,
    tool: Tool,
    tools_open: Rc<RefCell<Option<Rc<dyn Fn(Option<melete_core::Brush>)>>>>,
) {
    use melete_core::{Geometry, TipShape, WidthMode};

    let recipe: Option<melete_core::Brush> = {
        let s = state.borrow();
        let key = tool_key(tool).unwrap_or("pen");
        s.tool_brushes
            .get(key)
            .cloned()
            .or_else(|| s.active_brush_recipe.clone())
            .or_else(|| {
                // Fall back to the built-in for the tool's ToolStyle
                // so the popup always has something to display.
                use melete_canvas::built_in_brushes as bi;
                Some(match tool {
                    Tool::Pen => bi::pen(0.6, 0.4),
                    Tool::Pencil => bi::pencil(0.4, 0.9, 0.12, 8.0, 0.22),
                    Tool::Highlighter => bi::highlighter(0.6, 0.4),
                    Tool::Paintbrush => bi::paintbrush(1.6, 1.4, 0.95, 0.07, 0.20, 0.95),
                    Tool::SprayCan => bi::spray(36, 0.06, 0.35),
                    Tool::Calligraphy => bi::calligraphy(45.0, 0.18, 0.5, true),
                    _ => return None,
                })
            })
    };

    let title = Label::builder().label("Brush recipe").xalign(0.0).build();
    title.add_css_class("dim-label");
    body.append(&title);

    let name = recipe
        .as_ref()
        .map(|b| b.name.clone())
        .unwrap_or_else(|| "(none)".to_string());
    let name_lbl = Label::builder()
        .label(format!("<b>{}</b>", glib_escape(&name)))
        .use_markup(true)
        .xalign(0.0)
        .build();
    body.append(&name_lbl);

    if let Some(brush) = recipe.as_ref() {
        for (i, layer) in brush.layers.iter().enumerate() {
            let geo = match &layer.geometry {
                Geometry::Smooth { .. } => "Smooth",
                Geometry::Outline { .. } => "Outline",
                Geometry::Scatter { .. } => "Scatter",
                Geometry::DabStamp { .. } => "DabStamp",
                Geometry::FanOffset { .. } => "FanOffset",
            };
            let w = match &layer.width {
                WidthMode::Constant { .. } => "Const",
                WidthMode::ClampedConstant { .. } => "Clamped",
                WidthMode::Pressure { .. } => "Pressure",
                WidthMode::DirectionAngled { .. } => "Angled",
                WidthMode::TiltBand { .. } => "TiltBand",
            };
            let tip = match &layer.tip {
                TipShape::Round => "Round",
                TipShape::Square => "Square",
                TipShape::FlatNib { .. } => "FlatNib",
                TipShape::Diamond => "Diamond",
                TipShape::StarN { .. } => "Star",
                TipShape::Custom { .. } => "Custom",
            };
            let mark = if layer.enabled { "•" } else { "○" };
            let row = Label::builder()
                .label(format!("{} L{} — {} · {} · {}", mark, i + 1, geo, w, tip))
                .xalign(0.0)
                .build();
            row.add_css_class("dim-label");
            body.append(&row);
        }
    }

    let edit_btn = Button::with_label("Open in Tool Editor…");
    {
        let recipe_for = recipe.clone();
        let tools_open = tools_open.clone();
        edit_btn.connect_clicked(move |_| {
            if let Some(f) = tools_open.borrow().as_ref().cloned() {
                f(recipe_for.clone());
            }
        });
    }
    body.append(&edit_btn);

    body.append(&Separator::new(Orientation::Horizontal));
}

/// Minimal Pango markup escape — the recipe `name` flows from a
/// user-controlled brush field, so escape `<>&` to avoid markup
/// injection in the popup label.
fn glib_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn add_preset_picker(body: &GtkBox, state: &SharedState, tool: Tool) {
    use gtk4::Entry;
    let key = match tool_key(tool) {
        Some(k) => k.to_string(),
        None => return,
    };

    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    row.append(&Label::builder().label("Preset:").xalign(0.0).build());

    let presets: Vec<String> = state
        .borrow()
        .tool_presets
        .get(&key)
        .map(|v| v.iter().map(|p| p.name.clone()).collect())
        .unwrap_or_default();
    let active_name = state
        .borrow()
        .active_tool_preset
        .get(&key)
        .cloned()
        .unwrap_or_else(|| "Default".into());
    let active_idx = presets.iter().position(|n| n == &active_name).unwrap_or(0) as u32;

    let names_strs: Vec<&str> = presets.iter().map(|s| s.as_str()).collect();
    let strs = StringList::new(&names_strs);
    let dd = DropDown::builder().model(&strs).hexpand(true).build();
    dd.set_selected(active_idx);
    row.append(&dd);

    let save_as = Button::with_label("New…");
    let dup_btn = Button::with_label("Duplicate");
    let del_btn = Button::with_label("Delete");
    row.append(&save_as);
    row.append(&dup_btn);
    row.append(&del_btn);
    body.append(&row);

    // Activate selected preset when dropdown changes.
    {
        let state = state.clone();
        let presets_clone = presets.clone();
        dd.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            if let Some(name) = presets_clone.get(idx) {
                crate::state::activate_tool_preset(&state, tool, name);
            }
        });
    }

    // "New…" — prompt for a name, snapshot current ToolSettings as a
    // new preset, activate it.
    {
        let state = state.clone();
        let key = key.clone();
        save_as.connect_clicked(move |btn| {
            let parent = btn.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
            let dialog = gtk4::Window::builder()
                .modal(true)
                .title("New preset")
                .default_width(280)
                .build();
            if let Some(p) = parent.as_ref() {
                dialog.set_transient_for(Some(p));
            }
            let inner = GtkBox::builder()
                .orientation(Orientation::Vertical)
                .spacing(8)
                .margin_top(12)
                .margin_bottom(12)
                .margin_start(12)
                .margin_end(12)
                .build();
            inner.append(&Label::builder().label("Name").xalign(0.0).build());
            let entry = Entry::builder().placeholder_text("e.g. Bold pen").build();
            inner.append(&entry);
            let row2 = GtkBox::builder()
                .orientation(Orientation::Horizontal)
                .spacing(6)
                .build();
            let cancel = Button::with_label("Cancel");
            let ok = Button::with_label("Create");
            row2.append(&cancel);
            row2.append(&ok);
            inner.append(&row2);
            dialog.set_child(Some(&inner));
            {
                let dialog = dialog.clone();
                cancel.connect_clicked(move |_| dialog.close());
            }
            {
                let dialog = dialog.clone();
                let state = state.clone();
                let key = key.clone();
                let entry = entry.clone();
                ok.connect_clicked(move |_| {
                    let name = entry.text().to_string();
                    if name.trim().is_empty() {
                        return;
                    }
                    let snapshot = state
                        .borrow()
                        .tool_settings
                        .get(&key)
                        .copied()
                        .unwrap_or_else(|| crate::tool_settings::default_settings_for(tool));
                    {
                        let mut s = state.borrow_mut();
                        let entry_list = s.tool_presets.entry(key.clone()).or_default();
                        if entry_list.iter().any(|p| p.name == name) {
                            return; // duplicate name — silent ignore
                        }
                        entry_list.push(crate::tool_settings::NamedToolSettings {
                            name: name.clone(),
                            settings: snapshot,
                        });
                    }
                    crate::state::activate_tool_preset(&state, tool, &name);
                    dialog.close();
                });
            }
            dialog.present();
        });
    }

    // Duplicate — copy current preset under a "<name> copy" name.
    {
        let state = state.clone();
        let key = key.clone();
        dup_btn.connect_clicked(move |_| {
            let new_name;
            {
                let mut s = state.borrow_mut();
                let active = s
                    .active_tool_preset
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| "Default".into());
                let snapshot = s
                    .tool_presets
                    .get(&key)
                    .and_then(|v| v.iter().find(|p| p.name == active).cloned())
                    .map(|p| p.settings)
                    .unwrap_or_else(|| crate::tool_settings::default_settings_for(tool));
                let entry_list = s.tool_presets.entry(key.clone()).or_default();
                let mut candidate = format!("{active} copy");
                let mut n = 2;
                while entry_list.iter().any(|p| p.name == candidate) {
                    candidate = format!("{active} copy {n}");
                    n += 1;
                }
                entry_list.push(crate::tool_settings::NamedToolSettings {
                    name: candidate.clone(),
                    settings: snapshot,
                });
                new_name = candidate;
            }
            crate::state::activate_tool_preset(&state, tool, &new_name);
        });
    }

    // Delete — drop the active preset, fall back to first remaining.
    {
        let state = state.clone();
        let key = key.clone();
        del_btn.connect_clicked(move |_| {
            let fallback;
            {
                let mut s = state.borrow_mut();
                let active = s
                    .active_tool_preset
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| "Default".into());
                let entry_list = match s.tool_presets.get_mut(&key) {
                    Some(v) => v,
                    None => return,
                };
                if entry_list.len() <= 1 {
                    return; // refuse to delete the last preset
                }
                entry_list.retain(|p| p.name != active);
                fallback = entry_list
                    .first()
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Default".into());
            }
            crate::state::activate_tool_preset(&state, tool, &fallback);
        });
    }
}

fn add_tool_settings_section(body: &GtkBox, state: &SharedState, tool: Tool) {
    let key = match tool_key(tool) {
        Some(k) => k.to_string(),
        None => return,
    };
    let initial: ToolSettings = state
        .borrow()
        .tool_settings
        .get(&key)
        .copied()
        .unwrap_or_else(|| default_settings_for(tool));

    let grid = gtk4::Grid::builder()
        .row_spacing(4)
        .column_spacing(10)
        .build();

    grid.attach(
        &Label::builder()
            .label("Default size (mm)")
            .xalign(1.0)
            .build(),
        0,
        0,
        1,
        1,
    );
    let bw = SpinButton::with_range(0.1, 60.0, 0.5);
    bw.set_digits(1);
    bw.set_value(initial.default_base_width);
    bw.set_hexpand(true);
    grid.attach(&bw, 1, 0, 1, 1);

    grid.attach(
        &Label::builder().label("Opacity ×").xalign(1.0).build(),
        0,
        1,
        1,
        1,
    );
    let op = SpinButton::with_range(0.0, 2.0, 0.05);
    op.set_digits(2);
    op.set_value(initial.opacity_mult as f64);
    op.set_hexpand(true);
    grid.attach(&op, 1, 1, 1, 1);

    grid.attach(
        &Label::builder().label("Width ×").xalign(1.0).build(),
        0,
        2,
        1,
        1,
    );
    let w = SpinButton::with_range(0.05, 12.0, 0.1);
    w.set_digits(2);
    w.set_value(initial.width_mult);
    w.set_hexpand(true);
    grid.attach(&w, 1, 2, 1, 1);

    grid.attach(
        &Label::builder().label("Blend").xalign(1.0).build(),
        0,
        3,
        1,
        1,
    );
    let blend_strs = StringList::new(&BLEND_MODES.iter().map(|(s, _)| *s).collect::<Vec<_>>());
    let blend_dd = DropDown::builder().model(&blend_strs).hexpand(true).build();
    blend_dd.set_selected(blend_index(initial.blend_mode));
    grid.attach(&blend_dd, 1, 3, 1, 1);

    body.append(&grid);

    let reset = Button::with_label("Reset tool settings");
    {
        let bw = bw.clone();
        let op = op.clone();
        let w = w.clone();
        let blend_dd = blend_dd.clone();
        reset.connect_clicked(move |_| {
            let d = default_settings_for(tool);
            bw.set_value(d.default_base_width);
            op.set_value(d.opacity_mult as f64);
            w.set_value(d.width_mult);
            blend_dd.set_selected(blend_index(d.blend_mode));
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let key = key.clone();
        let bw = bw.clone();
        let op = op.clone();
        let w = w.clone();
        let blend_dd = blend_dd.clone();
        move || {
            let canonical = default_settings_for(tool).brush_style;
            let new_settings = ToolSettings {
                opacity_mult: op.value() as f32,
                width_mult: w.value(),
                blend_mode: BLEND_MODES[blend_dd.selected() as usize].1,
                brush_style: canonical,
                default_base_width: bw.value(),
            };
            {
                let mut s = state.borrow_mut();
                s.tool_settings.insert(key.clone(), new_settings);
                // Write through to the active preset so the change
                // sticks when the user switches presets and back.
                let active = s
                    .active_tool_preset
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| "Default".into());
                if let Some(list) = s.tool_presets.get_mut(&key) {
                    for p in list.iter_mut() {
                        if p.name == active {
                            p.settings = new_settings;
                        }
                    }
                }
            }
            persist(&state);
        }
    };

    {
        let a = apply.clone();
        bw.connect_value_changed(move |_| a());
    }
    {
        let a = apply.clone();
        op.connect_value_changed(move |_| a());
    }
    {
        let a = apply.clone();
        w.connect_value_changed(move |_| a());
    }
    {
        let a = apply.clone();
        blend_dd.connect_selected_notify(move |_| a());
    }
}

fn add_palette_section(body: &GtkBox, state: &SharedState, tool: Tool) {
    use gtk4::{ColorDialog, ColorDialogButton, FlowBox, GestureClick, SelectionMode};

    let key = match tool_key(tool) {
        Some(k) => k.to_string(),
        None => return,
    };

    body.append(
        &Label::builder()
            .label("<b>Palette</b>")
            .use_markup(true)
            .xalign(0.0)
            .build(),
    );
    body.append(
        &Label::builder()
            .label("Click a swatch to apply. Right-click to remove.")
            .wrap(true)
            .xalign(0.0)
            .build(),
    );

    let row = FlowBox::builder()
        .orientation(Orientation::Horizontal)
        .row_spacing(4)
        .column_spacing(4)
        .selection_mode(SelectionMode::None)
        .max_children_per_line(20)
        .build();

    fn render_swatches(row: &FlowBox, state: &SharedState, key: &str, tool: Tool) {
        while let Some(c) = row.first_child() {
            row.remove(&c);
        }
        let palette = state
            .borrow()
            .tool_palettes
            .get(key)
            .cloned()
            .unwrap_or_default();
        for (idx, color_rgba) in palette.iter().enumerate() {
            let swatch =
                build_swatch_button(*color_rgba, state.clone(), key.to_string(), idx, tool);
            row.append(&swatch);
        }
    }

    render_swatches(&row, state, &key, tool);
    body.append(&row);

    let add_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    let dialog = ColorDialog::builder().with_alpha(false).build();
    let picker = ColorDialogButton::builder().dialog(&dialog).build();
    add_row.append(&picker);
    let add_btn = Button::with_label("Add to palette");
    add_row.append(&add_btn);
    body.append(&add_row);

    {
        let state = state.clone();
        let key = key.clone();
        let row = row.clone();
        let picker = picker.clone();
        add_btn.connect_clicked(move |_| {
            let rgba = picker.rgba();
            let bytes = [
                (rgba.red() * 255.0).round() as u8,
                (rgba.green() * 255.0).round() as u8,
                (rgba.blue() * 255.0).round() as u8,
                255,
            ];
            state
                .borrow_mut()
                .tool_palettes
                .entry(key.clone())
                .or_default()
                .push(bytes);
            let _ = crate::state::persist_tool_state(&state);
            render_swatches(&row, &state, &key, tool);
        });
    }

    fn build_swatch_button(
        rgba: [u8; 4],
        state: SharedState,
        key: String,
        idx: usize,
        tool: Tool,
    ) -> Button {
        use gtk4::DrawingArea;
        let btn = Button::builder().build();
        btn.add_css_class("flat");
        let area = DrawingArea::builder()
            .width_request(28)
            .height_request(28)
            .build();
        let [r, g, b, a] = rgba;
        area.set_draw_func(move |_, ctx, w, h| {
            ctx.set_source_rgba(
                r as f64 / 255.0,
                g as f64 / 255.0,
                b as f64 / 255.0,
                a as f64 / 255.0,
            );
            ctx.rectangle(0.0, 0.0, w as f64, h as f64);
            let _ = ctx.fill();
            ctx.set_source_rgba(0.0, 0.0, 0.0, 0.4);
            ctx.set_line_width(0.8);
            ctx.rectangle(0.5, 0.5, w as f64 - 1.0, h as f64 - 1.0);
            let _ = ctx.stroke();
        });
        btn.set_child(Some(&area));

        // Left-click → set pen color.
        {
            let state = state.clone();
            btn.connect_clicked(move |_| {
                state.borrow_mut().pen.color = melete_core::Color { r, g, b, a };
            });
        }

        // Right-click → remove this swatch.
        {
            let state = state.clone();
            let key = key.clone();
            let gesture = GestureClick::new();
            gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
            let btn_weak = btn.downgrade();
            gesture.connect_pressed(move |_, _, _, _| {
                {
                    let mut s = state.borrow_mut();
                    if let Some(list) = s.tool_palettes.get_mut(&key) {
                        if idx < list.len() {
                            list.remove(idx);
                        }
                    }
                }
                let _ = crate::state::persist_tool_state(&state);
                // Trigger a rebuild by re-rendering the parent FlowBox.
                if let Some(b) = btn_weak.upgrade() {
                    if let Some(parent) = b.parent() {
                        if let Some(fb) = parent
                            .ancestor(gtk4::FlowBox::static_type())
                            .and_then(|a| a.downcast::<gtk4::FlowBox>().ok())
                        {
                            // Tell our caller to re-render via a tick.
                            let key_c = key.clone();
                            let state_c = state.clone();
                            let fb = fb.clone();
                            gtk4::glib::idle_add_local_once(move || {
                                render_swatches(&fb, &state_c, &key_c, tool);
                            });
                        }
                    }
                }
            });
            btn.add_controller(gesture);
        }
        btn
    }
}

fn row(label: &str) -> Label {
    Label::builder().label(label).xalign(1.0).build()
}
fn spin(min: f64, max: f64, step: f64, digits: u32, val: f64) -> SpinButton {
    let s = SpinButton::with_range(min, max, step);
    s.set_digits(digits);
    s.set_value(val);
    s.set_hexpand(true);
    s
}

fn persist(state: &SharedState) {
    let _ = crate::state::persist_tool_state(state);
}

const PEN_SHAPES: &[(&str, PenShape)] = &[
    ("Round", PenShape::Round),
    ("Flat (variable-width polygon)", PenShape::Flat),
    ("Marker (chunky tip)", PenShape::Marker),
];
fn pen_shape_idx(s: PenShape) -> u32 {
    PEN_SHAPES.iter().position(|(_, v)| *v == s).unwrap_or(0) as u32
}

fn append_pen_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.pen;
    let g = gtk4::Grid::builder()
        .row_spacing(4)
        .column_spacing(10)
        .build();
    g.attach(&row("Tip shape"), 0, 0, 1, 1);
    let shape_strs = StringList::new(&PEN_SHAPES.iter().map(|(s, _)| *s).collect::<Vec<_>>());
    let shape_dd = DropDown::builder().model(&shape_strs).hexpand(true).build();
    shape_dd.set_selected(pen_shape_idx(p.shape));
    g.attach(&shape_dd, 1, 0, 1, 1);
    g.attach(&row("Width floor"), 0, 1, 1, 1);
    let floor = spin(0.0, 1.5, 0.05, 2, p.width_floor);
    g.attach(&floor, 1, 1, 1, 1);
    g.attach(&row("Pressure amplitude"), 0, 2, 1, 1);
    let amp = spin(0.0, 1.5, 0.05, 2, p.width_pressure_amplitude);
    g.attach(&amp, 1, 2, 1, 1);
    g.attach(&row("Marker width ×"), 0, 3, 1, 1);
    let marker = spin(0.5, 5.0, 0.05, 2, p.marker_width_mult);
    g.attach(&marker, 1, 3, 1, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Pen internals");
    {
        let (shape_dd, floor, amp, marker) =
            (shape_dd.clone(), floor.clone(), amp.clone(), marker.clone());
        reset.connect_clicked(move |_| {
            let d = PenParams::default();
            shape_dd.set_selected(pen_shape_idx(d.shape));
            floor.set_value(d.width_floor);
            amp.set_value(d.width_pressure_amplitude);
            marker.set_value(d.marker_width_mult);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let (shape_dd, floor, amp, marker) =
            (shape_dd.clone(), floor.clone(), amp.clone(), marker.clone());
        move || {
            state.borrow_mut().brush_params.pen = PenParams {
                shape: PEN_SHAPES[shape_dd.selected() as usize].1,
                width_floor: floor.value(),
                width_pressure_amplitude: amp.value(),
                marker_width_mult: marker.value(),
            };
            persist(&state);
        }
    };
    {
        let a = apply.clone();
        shape_dd.connect_selected_notify(move |_| a());
    }
    {
        let a = apply.clone();
        floor.connect_value_changed(move |_| a());
    }
    {
        let a = apply.clone();
        amp.connect_value_changed(move |_| a());
    }
    {
        let a = apply.clone();
        marker.connect_value_changed(move |_| a());
    }
}

const PENCIL_SHAPES: &[(&str, PencilShape)] = &[
    ("Cylindrical", PencilShape::Cylindrical),
    ("Carpenter (flat lead)", PencilShape::Carpenter),
    ("Mechanical (thin)", PencilShape::Mechanical),
];
fn pencil_shape_idx(s: PencilShape) -> u32 {
    PENCIL_SHAPES.iter().position(|(_, v)| *v == s).unwrap_or(0) as u32
}

fn append_pencil_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.pencil;
    let g = gtk4::Grid::builder()
        .row_spacing(4)
        .column_spacing(10)
        .build();
    g.attach(&row("Tip shape"), 0, 0, 1, 1);
    let shape_strs = StringList::new(&PENCIL_SHAPES.iter().map(|(s, _)| *s).collect::<Vec<_>>());
    let shape_dd = DropDown::builder().model(&shape_strs).hexpand(true).build();
    shape_dd.set_selected(pencil_shape_idx(p.shape));
    g.attach(&shape_dd, 1, 0, 1, 1);
    g.attach(&row("Core min"), 0, 1, 1, 1);
    let cmin = spin(0.05, 3.0, 0.05, 2, p.core_clamp_min);
    g.attach(&cmin, 1, 1, 1, 1);
    g.attach(&row("Core max"), 0, 2, 1, 1);
    let cmax = spin(0.05, 5.0, 0.05, 2, p.core_clamp_max);
    g.attach(&cmax, 1, 2, 1, 1);
    g.attach(&row("Tilt threshold"), 0, 3, 1, 1);
    let thr = spin(0.0, 1.0, 0.02, 2, p.tilt_threshold);
    g.attach(&thr, 1, 3, 1, 1);
    g.attach(&row("Tilt band ×"), 0, 4, 1, 1);
    let tband = spin(0.0, 30.0, 0.5, 1, p.tilt_band_mult);
    g.attach(&tband, 1, 4, 1, 1);
    g.attach(&row("Tilt alpha"), 0, 5, 1, 1);
    let talpha = spin(0.0, 1.0, 0.02, 2, p.tilt_alpha_scale);
    g.attach(&talpha, 1, 5, 1, 1);
    g.attach(&row("Carpenter ×"), 0, 6, 1, 1);
    let carp = spin(0.5, 5.0, 0.05, 2, p.carpenter_width_mult);
    g.attach(&carp, 1, 6, 1, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Pencil internals");
    {
        let (shape_dd, cmin, cmax, thr, tband, talpha, carp) = (
            shape_dd.clone(),
            cmin.clone(),
            cmax.clone(),
            thr.clone(),
            tband.clone(),
            talpha.clone(),
            carp.clone(),
        );
        reset.connect_clicked(move |_| {
            let d = PencilParams::default();
            shape_dd.set_selected(pencil_shape_idx(d.shape));
            cmin.set_value(d.core_clamp_min);
            cmax.set_value(d.core_clamp_max);
            thr.set_value(d.tilt_threshold);
            tband.set_value(d.tilt_band_mult);
            talpha.set_value(d.tilt_alpha_scale);
            carp.set_value(d.carpenter_width_mult);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let (shape_dd, cmin, cmax, thr, tband, talpha, carp) = (
            shape_dd.clone(),
            cmin.clone(),
            cmax.clone(),
            thr.clone(),
            tband.clone(),
            talpha.clone(),
            carp.clone(),
        );
        move || {
            state.borrow_mut().brush_params.pencil = PencilParams {
                shape: PENCIL_SHAPES[shape_dd.selected() as usize].1,
                core_clamp_min: cmin.value(),
                core_clamp_max: cmax.value(),
                tilt_threshold: thr.value(),
                tilt_band_mult: tband.value(),
                tilt_alpha_scale: talpha.value(),
                carpenter_width_mult: carp.value(),
            };
            persist(&state);
        }
    };
    {
        let a = apply.clone();
        shape_dd.connect_selected_notify(move |_| a());
    }
    for s in [&cmin, &cmax, &thr, &tband, &talpha, &carp] {
        let a = apply.clone();
        s.connect_value_changed(move |_| a());
    }
}

const PAINTBRUSH_SHAPES: &[(&str, PaintbrushShape)] = &[
    ("Round (3-pass halo)", PaintbrushShape::Round),
    ("Flat (sumi)", PaintbrushShape::Flat),
    ("Fan", PaintbrushShape::Fan),
];
fn paintbrush_shape_idx(s: PaintbrushShape) -> u32 {
    PAINTBRUSH_SHAPES
        .iter()
        .position(|(_, v)| *v == s)
        .unwrap_or(0) as u32
}

fn append_paintbrush_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.paintbrush;
    let g = gtk4::Grid::builder()
        .row_spacing(4)
        .column_spacing(10)
        .build();
    g.attach(&row("Bristle shape"), 0, 0, 1, 1);
    let shape_strs = StringList::new(
        &PAINTBRUSH_SHAPES
            .iter()
            .map(|(s, _)| *s)
            .collect::<Vec<_>>(),
    );
    let shape_dd = DropDown::builder().model(&shape_strs).hexpand(true).build();
    shape_dd.set_selected(paintbrush_shape_idx(p.shape));
    g.attach(&shape_dd, 1, 0, 1, 1);
    g.attach(&row("Halo width ×"), 0, 1, 1, 1);
    let hw = spin(1.0, 5.0, 0.05, 2, p.halo_width_mult);
    g.attach(&hw, 1, 1, 1, 1);
    g.attach(&row("Outer halo ×"), 0, 2, 1, 1);
    let oh = spin(0.5, 4.0, 0.05, 2, p.outer_halo_mult);
    g.attach(&oh, 1, 2, 1, 1);
    g.attach(&row("Mid halo ×"), 0, 3, 1, 1);
    let mh = spin(0.2, 3.0, 0.05, 2, p.mid_halo_mult);
    g.attach(&mh, 1, 3, 1, 1);
    g.attach(&row("Outer alpha"), 0, 4, 1, 1);
    let oa = spin(0.0, 1.0, 0.01, 2, p.outer_alpha);
    g.attach(&oa, 1, 4, 1, 1);
    g.attach(&row("Mid alpha"), 0, 5, 1, 1);
    let ma = spin(0.0, 1.0, 0.01, 2, p.mid_alpha);
    g.attach(&ma, 1, 5, 1, 1);
    g.attach(&row("Core alpha"), 0, 6, 1, 1);
    let ca = spin(0.0, 1.0, 0.01, 2, p.core_alpha);
    g.attach(&ca, 1, 6, 1, 1);
    g.attach(&row("Fan tines"), 0, 7, 1, 1);
    let fc = spin(2.0, 8.0, 1.0, 0, p.fan_count as f64);
    g.attach(&fc, 1, 7, 1, 1);
    g.attach(&row("Fan spread ×"), 0, 8, 1, 1);
    let fs = spin(0.5, 4.0, 0.1, 2, p.fan_spread_mult);
    g.attach(&fs, 1, 8, 1, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Paintbrush internals");
    {
        let (shape_dd, hw, oh, mh, oa, ma, ca, fc, fs) = (
            shape_dd.clone(),
            hw.clone(),
            oh.clone(),
            mh.clone(),
            oa.clone(),
            ma.clone(),
            ca.clone(),
            fc.clone(),
            fs.clone(),
        );
        reset.connect_clicked(move |_| {
            let d = PaintbrushParams::default();
            shape_dd.set_selected(paintbrush_shape_idx(d.shape));
            hw.set_value(d.halo_width_mult);
            oh.set_value(d.outer_halo_mult);
            mh.set_value(d.mid_halo_mult);
            oa.set_value(d.outer_alpha);
            ma.set_value(d.mid_alpha);
            ca.set_value(d.core_alpha);
            fc.set_value(d.fan_count as f64);
            fs.set_value(d.fan_spread_mult);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let (shape_dd, hw, oh, mh, oa, ma, ca, fc, fs) = (
            shape_dd.clone(),
            hw.clone(),
            oh.clone(),
            mh.clone(),
            oa.clone(),
            ma.clone(),
            ca.clone(),
            fc.clone(),
            fs.clone(),
        );
        move || {
            state.borrow_mut().brush_params.paintbrush = PaintbrushParams {
                shape: PAINTBRUSH_SHAPES[shape_dd.selected() as usize].1,
                halo_width_mult: hw.value(),
                outer_halo_mult: oh.value(),
                mid_halo_mult: mh.value(),
                outer_alpha: oa.value(),
                mid_alpha: ma.value(),
                core_alpha: ca.value(),
                fan_count: fc.value() as u32,
                fan_spread_mult: fs.value(),
            };
            persist(&state);
        }
    };
    {
        let a = apply.clone();
        shape_dd.connect_selected_notify(move |_| a());
    }
    for s in [&hw, &oh, &mh, &oa, &ma, &ca, &fc, &fs] {
        let a = apply.clone();
        s.connect_value_changed(move |_| a());
    }
}

const SPRAY_SHAPES: &[(&str, SprayShape)] = &[
    ("Circle scatter", SprayShape::Circle),
    ("Square stamp", SprayShape::Square),
    ("Cone (tilt-driven)", SprayShape::Cone),
];
fn spray_shape_idx(s: SprayShape) -> u32 {
    SPRAY_SHAPES.iter().position(|(_, v)| *v == s).unwrap_or(0) as u32
}

fn append_spray_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.spray;
    let g = gtk4::Grid::builder()
        .row_spacing(4)
        .column_spacing(10)
        .build();
    g.attach(&row("Spray shape"), 0, 0, 1, 1);
    let shape_strs = StringList::new(&SPRAY_SHAPES.iter().map(|(s, _)| *s).collect::<Vec<_>>());
    let shape_dd = DropDown::builder().model(&shape_strs).hexpand(true).build();
    shape_dd.set_selected(spray_shape_idx(p.shape));
    g.attach(&shape_dd, 1, 0, 1, 1);
    g.attach(&row("Dots/point"), 0, 1, 1, 1);
    let dpp = spin(1.0, 200.0, 1.0, 0, p.dots_per_point as f64);
    g.attach(&dpp, 1, 1, 1, 1);
    g.attach(&row("Dot factor"), 0, 2, 1, 1);
    let drf = spin(0.01, 1.0, 0.01, 2, p.dot_radius_factor);
    g.attach(&drf, 1, 2, 1, 1);
    g.attach(&row("Min radius"), 0, 3, 1, 1);
    let mdr = spin(0.05, 4.0, 0.05, 2, p.min_dot_radius);
    g.attach(&mdr, 1, 3, 1, 1);
    g.attach(&row("Cone spread (°)"), 0, 4, 1, 1);
    let cone = spin(5.0, 90.0, 1.0, 0, p.cone_spread_deg);
    g.attach(&cone, 1, 4, 1, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Spray internals");
    {
        let (shape_dd, dpp, drf, mdr, cone) = (
            shape_dd.clone(),
            dpp.clone(),
            drf.clone(),
            mdr.clone(),
            cone.clone(),
        );
        reset.connect_clicked(move |_| {
            let d = SprayParams::default();
            shape_dd.set_selected(spray_shape_idx(d.shape));
            dpp.set_value(d.dots_per_point as f64);
            drf.set_value(d.dot_radius_factor);
            mdr.set_value(d.min_dot_radius);
            cone.set_value(d.cone_spread_deg);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let (shape_dd, dpp, drf, mdr, cone) = (
            shape_dd.clone(),
            dpp.clone(),
            drf.clone(),
            mdr.clone(),
            cone.clone(),
        );
        move || {
            state.borrow_mut().brush_params.spray = SprayParams {
                shape: SPRAY_SHAPES[shape_dd.selected() as usize].1,
                dots_per_point: dpp.value() as u32,
                dot_radius_factor: drf.value(),
                min_dot_radius: mdr.value(),
                cone_spread_deg: cone.value(),
            };
            persist(&state);
        }
    };
    {
        let a = apply.clone();
        shape_dd.connect_selected_notify(move |_| a());
    }
    for s in [&dpp, &drf, &mdr, &cone] {
        let a = apply.clone();
        s.connect_value_changed(move |_| a());
    }
}

const CALLIGRAPHY_SHAPES: &[(&str, CalligraphyShape)] = &[
    ("Flat-cut nib", CalligraphyShape::FlatCut),
    ("Round nib", CalligraphyShape::Round),
    ("Brush nib (pressure)", CalligraphyShape::BrushNib),
];
fn calligraphy_shape_idx(s: CalligraphyShape) -> u32 {
    CALLIGRAPHY_SHAPES
        .iter()
        .position(|(_, v)| *v == s)
        .unwrap_or(0) as u32
}

fn append_calligraphy_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.calligraphy;
    let g = gtk4::Grid::builder()
        .row_spacing(4)
        .column_spacing(10)
        .build();
    g.attach(&row("Nib shape"), 0, 0, 1, 1);
    let shape_strs = StringList::new(
        &CALLIGRAPHY_SHAPES
            .iter()
            .map(|(s, _)| *s)
            .collect::<Vec<_>>(),
    );
    let shape_dd = DropDown::builder().model(&shape_strs).hexpand(true).build();
    shape_dd.set_selected(calligraphy_shape_idx(p.shape));
    g.attach(&shape_dd, 1, 0, 1, 1);
    g.attach(&row("Nib angle (°)"), 0, 1, 1, 1);
    let nib = spin(-90.0, 90.0, 1.0, 0, p.nib_angle_deg);
    g.attach(&nib, 1, 1, 1, 1);
    g.attach(&row("Min ratio"), 0, 2, 1, 1);
    let mr = spin(0.0, 1.0, 0.02, 2, p.min_ratio);
    g.attach(&mr, 1, 2, 1, 1);
    g.attach(&row("Resample step ×"), 0, 3, 1, 1);
    let rs = spin(0.05, 2.0, 0.05, 2, p.resample_step_mult);
    g.attach(&rs, 1, 3, 1, 1);
    let smooth = CheckButton::with_label("Smooth outline");
    smooth.set_active(p.smooth_outline);
    g.attach(&smooth, 0, 4, 2, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Calligraphy internals");
    {
        let (shape_dd, nib, mr, rs, smooth) = (
            shape_dd.clone(),
            nib.clone(),
            mr.clone(),
            rs.clone(),
            smooth.clone(),
        );
        reset.connect_clicked(move |_| {
            let d = CalligraphyParams::default();
            shape_dd.set_selected(calligraphy_shape_idx(d.shape));
            nib.set_value(d.nib_angle_deg);
            mr.set_value(d.min_ratio);
            rs.set_value(d.resample_step_mult);
            smooth.set_active(d.smooth_outline);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let (shape_dd, nib, mr, rs, smooth) = (
            shape_dd.clone(),
            nib.clone(),
            mr.clone(),
            rs.clone(),
            smooth.clone(),
        );
        move || {
            state.borrow_mut().brush_params.calligraphy = CalligraphyParams {
                shape: CALLIGRAPHY_SHAPES[shape_dd.selected() as usize].1,
                nib_angle_deg: nib.value(),
                min_ratio: mr.value(),
                resample_step_mult: rs.value(),
                smooth_outline: smooth.is_active(),
            };
            persist(&state);
        }
    };
    {
        let a = apply.clone();
        shape_dd.connect_selected_notify(move |_| a());
    }
    for s in [&nib, &mr, &rs] {
        let a = apply.clone();
        s.connect_value_changed(move |_| a());
    }
    {
        let a = apply.clone();
        smooth.connect_toggled(move |_| a());
    }

    // Avoid unused-warning when this brush is the only one selected.
    let _ = ToolStyleParams::default();
}
