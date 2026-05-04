//! Floating "Tool options" panel.
//!
//! When developer mode is on (`AppConfig::developer_mode = true` or env
//! `JOURNAL_DEV=1`), a separate top-level window appears alongside the
//! main app window showing the currently-selected tool's full settings —
//! default size, opacity / width multipliers, blend mode, brush-style
//! override, and the per-brush-style internal tuning that matches the
//! tool's current brush_style. The window watches `state.tool` via a
//! frame-clock tick callback and rebuilds its content whenever the user
//! switches tools.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, DropDown, Label, Orientation, ScrolledWindow, Separator,
    SpinButton, StringList, Window,
};
use journal_canvas::vello_renderer::{
    BrushParams, CalligraphyParams, PaintbrushParams, PenParams, PencilParams, SprayParams,
};
use journal_core::{BlendMode, BrushStyle};

use crate::state::{SharedState, Tool};
use crate::tool_settings::{default_settings_for, settable_tools, tool_key, tool_label, ToolSettings};

const BLEND_MODES: &[(&str, BlendMode)] = &[
    ("Normal", BlendMode::Normal),
    ("Multiply", BlendMode::Multiply),
    ("Screen", BlendMode::Screen),
    ("Overlay", BlendMode::Overlay),
    ("Darken", BlendMode::Darken),
    ("Lighten", BlendMode::Lighten),
    ("Erase", BlendMode::Erase),
];

const BRUSH_STYLES: &[(&str, BrushStyle)] = &[
    ("Pen", BrushStyle::Pen),
    ("Pencil", BrushStyle::Pencil),
    ("Highlighter", BrushStyle::Highlighter),
    ("Paintbrush", BrushStyle::Paintbrush),
    ("Spray Can", BrushStyle::SprayCan),
    ("Calligraphy", BrushStyle::Calligraphy),
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

    let scroll = ScrolledWindow::builder().hexpand(true).vexpand(true).build();
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(12)
        .margin_end(12)
        .build();
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
    rebuild_for_tool(&body, &state, initial_tool);

    let last_tool: Rc<Cell<Tool>> = Rc::new(Cell::new(initial_tool));
    {
        let body = body.clone();
        let state = state.clone();
        let last_tool = last_tool.clone();
        outer.add_tick_callback(move |_, _| {
            let cur = state.borrow().tool;
            if std::mem::discriminant(&cur) != std::mem::discriminant(&last_tool.get()) {
                last_tool.set(cur);
                rebuild_for_tool(&body, &state, cur);
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
pub fn build_tool_options_window(
    parent: &gtk4::ApplicationWindow,
    state: SharedState,
) -> Window {
    let dock_slot = GtkBox::builder().orientation(Orientation::Vertical).build();
    let panel = build_tool_options_panel(parent, state, dock_slot);
    panel.window.clone()
}

fn rebuild_for_tool(body: &GtkBox, state: &SharedState, tool: Tool) {
    while let Some(c) = body.first_child() {
        body.remove(&c);
    }

    let Some(_key) = tool_key(tool) else {
        let lbl = Label::builder()
            .label("This tool has no editable options.")
            .wrap(true)
            .xalign(0.0)
            .build();
        body.append(&lbl);
        return;
    };

    let header = Label::builder()
        .label(&format!("<b>{}</b>", tool_label(tool)))
        .use_markup(true)
        .xalign(0.0)
        .build();
    body.append(&header);

    add_preset_picker(body, state, tool);
    add_tool_settings_section(body, state, tool);

    body.append(&Separator::new(Orientation::Horizontal));
    add_palette_section(body, state, tool);

    body.append(&Separator::new(Orientation::Horizontal));
    add_brush_internal_section(body, state, tool);

    // Quick "jump to tool list" links so the panel stays useful when the
    // user has many tools open and wants to switch without going back to
    // the toolbar (panel is itself a separate window).
    body.append(&Separator::new(Orientation::Horizontal));
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
    body.append(&switch_row);
}

fn add_preset_picker(body: &GtkBox, state: &SharedState, tool: Tool) {
    use gtk4::Entry;
    let key = match tool_key(tool) {
        Some(k) => k.to_string(),
        None => return,
    };

    let row = GtkBox::builder().orientation(Orientation::Horizontal).spacing(6).build();
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
    let active_idx = presets
        .iter()
        .position(|n| n == &active_name)
        .unwrap_or(0) as u32;

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
            let parent = btn
                .root()
                .and_then(|r| r.downcast::<gtk4::Window>().ok());
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
            let row2 = GtkBox::builder().orientation(Orientation::Horizontal).spacing(6).build();
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
                        .unwrap_or_else(|| {
                            crate::tool_settings::default_settings_for(tool)
                        });
                    {
                        let mut s = state.borrow_mut();
                        let entry_list = s.tool_presets.entry(key.clone()).or_insert_with(Vec::new);
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
                    .unwrap_or_else(|| {
                        crate::tool_settings::default_settings_for(tool)
                    });
                let entry_list = s.tool_presets.entry(key.clone()).or_insert_with(Vec::new);
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

    let grid = gtk4::Grid::builder().row_spacing(4).column_spacing(10).build();

    grid.attach(&Label::builder().label("Default size (mm)").xalign(1.0).build(), 0, 0, 1, 1);
    let bw = SpinButton::with_range(0.1, 60.0, 0.5);
    bw.set_digits(1);
    bw.set_value(initial.default_base_width);
    bw.set_hexpand(true);
    grid.attach(&bw, 1, 0, 1, 1);

    grid.attach(&Label::builder().label("Opacity ×").xalign(1.0).build(), 0, 1, 1, 1);
    let op = SpinButton::with_range(0.0, 2.0, 0.05);
    op.set_digits(2);
    op.set_value(initial.opacity_mult as f64);
    op.set_hexpand(true);
    grid.attach(&op, 1, 1, 1, 1);

    grid.attach(&Label::builder().label("Width ×").xalign(1.0).build(), 0, 2, 1, 1);
    let w = SpinButton::with_range(0.05, 12.0, 0.1);
    w.set_digits(2);
    w.set_value(initial.width_mult);
    w.set_hexpand(true);
    grid.attach(&w, 1, 2, 1, 1);

    grid.attach(&Label::builder().label("Blend").xalign(1.0).build(), 0, 3, 1, 1);
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
            let swatch = build_swatch_button(*color_rgba, state.clone(), key.to_string(), idx, tool);
            row.append(&swatch);
        }
    }

    render_swatches(&row, state, &key, tool);
    body.append(&row);

    let add_row = GtkBox::builder().orientation(Orientation::Horizontal).spacing(6).build();
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
                .or_insert_with(Vec::new)
                .push(bytes);
            crate::state::persist_tool_state(&state);
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
                state.borrow_mut().pen.color = journal_core::Color { r, g, b, a };
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
                crate::state::persist_tool_state(&state);
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

fn add_brush_internal_section(body: &GtkBox, state: &SharedState, tool: Tool) {
    // Show the internal tuning matching whatever brush_style this tool
    // currently uses. Keep this function in sync with the per-brush
    // sections in `settings_dialogs::add_brush_param_sections` — they
    // edit the same `state.brush_params`.
    let style = state
        .borrow()
        .tool_settings
        .get(tool_key(tool).unwrap_or(""))
        .map(|s| s.brush_style)
        .unwrap_or(default_settings_for(tool).brush_style);

    let header = Label::builder()
        .label(&format!(
            "<b>{} internals</b>",
            BRUSH_STYLES.iter().find(|(_, s)| *s == style).map(|(n, _)| *n).unwrap_or("")
        ))
        .use_markup(true)
        .xalign(0.0)
        .build();
    body.append(&header);
    body.append(
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
        BrushStyle::Pen | BrushStyle::Highlighter => append_pen_internals(body, state),
        BrushStyle::Pencil => append_pencil_internals(body, state),
        BrushStyle::Paintbrush => append_paintbrush_internals(body, state),
        BrushStyle::SprayCan => append_spray_internals(body, state),
        BrushStyle::Calligraphy => append_calligraphy_internals(body, state),
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
    crate::state::persist_tool_state(state);
}

fn append_pen_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.pen;
    let g = gtk4::Grid::builder().row_spacing(4).column_spacing(10).build();
    g.attach(&row("Width floor"), 0, 0, 1, 1);
    let floor = spin(0.0, 1.5, 0.05, 2, p.width_floor);
    g.attach(&floor, 1, 0, 1, 1);
    g.attach(&row("Pressure amplitude"), 0, 1, 1, 1);
    let amp = spin(0.0, 1.5, 0.05, 2, p.width_pressure_amplitude);
    g.attach(&amp, 1, 1, 1, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Pen internals");
    {
        let floor = floor.clone();
        let amp = amp.clone();
        reset.connect_clicked(move |_| {
            let d = PenParams::default();
            floor.set_value(d.width_floor);
            amp.set_value(d.width_pressure_amplitude);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let floor = floor.clone();
        let amp = amp.clone();
        move || {
            state.borrow_mut().brush_params.pen = PenParams {
                width_floor: floor.value(),
                width_pressure_amplitude: amp.value(),
            };
            persist(&state);
        }
    };
    {
        let a = apply.clone();
        floor.connect_value_changed(move |_| a());
    }
    {
        let a = apply.clone();
        amp.connect_value_changed(move |_| a());
    }
}

fn append_pencil_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.pencil;
    let g = gtk4::Grid::builder().row_spacing(4).column_spacing(10).build();
    g.attach(&row("Core min"), 0, 0, 1, 1);
    let cmin = spin(0.05, 3.0, 0.05, 2, p.core_clamp_min);
    g.attach(&cmin, 1, 0, 1, 1);
    g.attach(&row("Core max"), 0, 1, 1, 1);
    let cmax = spin(0.05, 5.0, 0.05, 2, p.core_clamp_max);
    g.attach(&cmax, 1, 1, 1, 1);
    g.attach(&row("Tilt threshold"), 0, 2, 1, 1);
    let thr = spin(0.0, 1.0, 0.02, 2, p.tilt_threshold);
    g.attach(&thr, 1, 2, 1, 1);
    g.attach(&row("Tilt band ×"), 0, 3, 1, 1);
    let tband = spin(0.0, 30.0, 0.5, 1, p.tilt_band_mult);
    g.attach(&tband, 1, 3, 1, 1);
    g.attach(&row("Tilt alpha"), 0, 4, 1, 1);
    let talpha = spin(0.0, 1.0, 0.02, 2, p.tilt_alpha_scale);
    g.attach(&talpha, 1, 4, 1, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Pencil internals");
    {
        let (cmin, cmax, thr, tband, talpha) =
            (cmin.clone(), cmax.clone(), thr.clone(), tband.clone(), talpha.clone());
        reset.connect_clicked(move |_| {
            let d = PencilParams::default();
            cmin.set_value(d.core_clamp_min);
            cmax.set_value(d.core_clamp_max);
            thr.set_value(d.tilt_threshold);
            tband.set_value(d.tilt_band_mult);
            talpha.set_value(d.tilt_alpha_scale);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let (cmin, cmax, thr, tband, talpha) =
            (cmin.clone(), cmax.clone(), thr.clone(), tband.clone(), talpha.clone());
        move || {
            state.borrow_mut().brush_params.pencil = PencilParams {
                core_clamp_min: cmin.value(),
                core_clamp_max: cmax.value(),
                tilt_threshold: thr.value(),
                tilt_band_mult: tband.value(),
                tilt_alpha_scale: talpha.value(),
            };
            persist(&state);
        }
    };
    for s in [&cmin, &cmax, &thr, &tband, &talpha] {
        let a = apply.clone();
        s.connect_value_changed(move |_| a());
    }
}

fn append_paintbrush_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.paintbrush;
    let g = gtk4::Grid::builder().row_spacing(4).column_spacing(10).build();
    g.attach(&row("Halo width ×"), 0, 0, 1, 1);
    let hw = spin(1.0, 5.0, 0.05, 2, p.halo_width_mult);
    g.attach(&hw, 1, 0, 1, 1);
    g.attach(&row("Outer halo ×"), 0, 1, 1, 1);
    let oh = spin(0.5, 4.0, 0.05, 2, p.outer_halo_mult);
    g.attach(&oh, 1, 1, 1, 1);
    g.attach(&row("Mid halo ×"), 0, 2, 1, 1);
    let mh = spin(0.2, 3.0, 0.05, 2, p.mid_halo_mult);
    g.attach(&mh, 1, 2, 1, 1);
    g.attach(&row("Outer alpha"), 0, 3, 1, 1);
    let oa = spin(0.0, 1.0, 0.01, 2, p.outer_alpha);
    g.attach(&oa, 1, 3, 1, 1);
    g.attach(&row("Mid alpha"), 0, 4, 1, 1);
    let ma = spin(0.0, 1.0, 0.01, 2, p.mid_alpha);
    g.attach(&ma, 1, 4, 1, 1);
    g.attach(&row("Core alpha"), 0, 5, 1, 1);
    let ca = spin(0.0, 1.0, 0.01, 2, p.core_alpha);
    g.attach(&ca, 1, 5, 1, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Paintbrush internals");
    {
        let (hw, oh, mh, oa, ma, ca) =
            (hw.clone(), oh.clone(), mh.clone(), oa.clone(), ma.clone(), ca.clone());
        reset.connect_clicked(move |_| {
            let d = PaintbrushParams::default();
            hw.set_value(d.halo_width_mult);
            oh.set_value(d.outer_halo_mult);
            mh.set_value(d.mid_halo_mult);
            oa.set_value(d.outer_alpha);
            ma.set_value(d.mid_alpha);
            ca.set_value(d.core_alpha);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let (hw, oh, mh, oa, ma, ca) =
            (hw.clone(), oh.clone(), mh.clone(), oa.clone(), ma.clone(), ca.clone());
        move || {
            state.borrow_mut().brush_params.paintbrush = PaintbrushParams {
                halo_width_mult: hw.value(),
                outer_halo_mult: oh.value(),
                mid_halo_mult: mh.value(),
                outer_alpha: oa.value(),
                mid_alpha: ma.value(),
                core_alpha: ca.value(),
            };
            persist(&state);
        }
    };
    for s in [&hw, &oh, &mh, &oa, &ma, &ca] {
        let a = apply.clone();
        s.connect_value_changed(move |_| a());
    }
}

fn append_spray_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.spray;
    let g = gtk4::Grid::builder().row_spacing(4).column_spacing(10).build();
    g.attach(&row("Dots/point"), 0, 0, 1, 1);
    let dpp = spin(1.0, 200.0, 1.0, 0, p.dots_per_point as f64);
    g.attach(&dpp, 1, 0, 1, 1);
    g.attach(&row("Dot factor"), 0, 1, 1, 1);
    let drf = spin(0.01, 1.0, 0.01, 2, p.dot_radius_factor);
    g.attach(&drf, 1, 1, 1, 1);
    g.attach(&row("Min radius"), 0, 2, 1, 1);
    let mdr = spin(0.05, 4.0, 0.05, 2, p.min_dot_radius);
    g.attach(&mdr, 1, 2, 1, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Spray internals");
    {
        let (dpp, drf, mdr) = (dpp.clone(), drf.clone(), mdr.clone());
        reset.connect_clicked(move |_| {
            let d = SprayParams::default();
            dpp.set_value(d.dots_per_point as f64);
            drf.set_value(d.dot_radius_factor);
            mdr.set_value(d.min_dot_radius);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let (dpp, drf, mdr) = (dpp.clone(), drf.clone(), mdr.clone());
        move || {
            state.borrow_mut().brush_params.spray = SprayParams {
                dots_per_point: dpp.value() as u32,
                dot_radius_factor: drf.value(),
                min_dot_radius: mdr.value(),
            };
            persist(&state);
        }
    };
    for s in [&dpp, &drf, &mdr] {
        let a = apply.clone();
        s.connect_value_changed(move |_| a());
    }
}

fn append_calligraphy_internals(body: &GtkBox, state: &SharedState) {
    let p = state.borrow().brush_params.calligraphy;
    let g = gtk4::Grid::builder().row_spacing(4).column_spacing(10).build();
    g.attach(&row("Nib angle (°)"), 0, 0, 1, 1);
    let nib = spin(-90.0, 90.0, 1.0, 0, p.nib_angle_deg);
    g.attach(&nib, 1, 0, 1, 1);
    g.attach(&row("Min ratio"), 0, 1, 1, 1);
    let mr = spin(0.0, 1.0, 0.02, 2, p.min_ratio);
    g.attach(&mr, 1, 1, 1, 1);
    g.attach(&row("Resample step ×"), 0, 2, 1, 1);
    let rs = spin(0.05, 2.0, 0.05, 2, p.resample_step_mult);
    g.attach(&rs, 1, 2, 1, 1);
    let smooth = CheckButton::with_label("Smooth outline");
    smooth.set_active(p.smooth_outline);
    g.attach(&smooth, 0, 3, 2, 1);
    body.append(&g);

    let reset = Button::with_label("Reset Calligraphy internals");
    {
        let (nib, mr, rs, smooth) = (nib.clone(), mr.clone(), rs.clone(), smooth.clone());
        reset.connect_clicked(move |_| {
            let d = CalligraphyParams::default();
            nib.set_value(d.nib_angle_deg);
            mr.set_value(d.min_ratio);
            rs.set_value(d.resample_step_mult);
            smooth.set_active(d.smooth_outline);
        });
    }
    body.append(&reset);

    let apply = {
        let state = state.clone();
        let (nib, mr, rs, smooth) = (nib.clone(), mr.clone(), rs.clone(), smooth.clone());
        move || {
            state.borrow_mut().brush_params.calligraphy = CalligraphyParams {
                nib_angle_deg: nib.value(),
                min_ratio: mr.value(),
                resample_step_mult: rs.value(),
                smooth_outline: smooth.is_active(),
            };
            persist(&state);
        }
    };
    for s in [&nib, &mr, &rs] {
        let a = apply.clone();
        s.connect_value_changed(move |_| a());
    }
    {
        let a = apply.clone();
        smooth.connect_toggled(move |_| a());
    }

    // Avoid unused-warning when this brush is the only one selected.
    let _ = BrushParams::default();
}
