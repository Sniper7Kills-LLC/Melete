use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, CheckButton, ColorDialog, ColorDialogButton,
    DrawingArea, Entry, Label, Orientation, ScrolledWindow, Separator, SpinButton, Window,
};
use journal_core::{NotebookId, PageTemplate, SectionId, TemplateId};
// {Notebook,Section}Store methods reached via dyn JournalBackend.

use crate::config::PenPreset;
use crate::state::SharedState;

fn modal(parent: &ApplicationWindow, title: &str) -> Window {
    Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(420)
        .default_height(480)
        .build()
}

fn sorted_templates(state: &SharedState) -> Vec<PageTemplate> {
    let s = state.borrow();
    let reg = s.templates.borrow();
    let mut v: Vec<PageTemplate> = reg.list().iter().map(|t| (*t).clone()).collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v
}

pub fn open_notebook_settings(
    parent: &ApplicationWindow,
    state: SharedState,
    notebook_id: NotebookId,
    on_saved: Box<dyn Fn()>,
) {
    let win = modal(parent, "Notebook settings");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let title = Label::builder()
        .label("Available templates")
        .halign(gtk4::Align::Start)
        .build();
    title.add_css_class("title-3");
    body.append(&title);

    let hint = Label::builder()
        .label("Select which page templates appear when creating new pages in this notebook. Leave all unchecked to allow every template.")
        .wrap(true)
        .halign(gtk4::Align::Start)
        .build();
    hint.add_css_class("dim-label");
    body.append(&hint);

    let nb = match state.borrow().backend.borrow_mut().get_notebook(notebook_id) {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("failed to load notebook for settings: {}", e);
            return;
        }
    };
    let assigned: std::collections::HashSet<TemplateId> = nb.assigned_templates.iter().copied().collect();

    let scroller = ScrolledWindow::builder().hexpand(true).vexpand(true).build();
    let list = GtkBox::builder().orientation(Orientation::Vertical).spacing(2).build();
    scroller.set_child(Some(&list));
    body.append(&scroller);

    let templates = sorted_templates(&state);
    let mut checks: Vec<(TemplateId, CheckButton)> = Vec::with_capacity(templates.len());
    for t in templates {
        let cb = CheckButton::with_label(&t.name);
        cb.set_active(assigned.contains(&t.id));
        list.append(&cb);
        checks.push((t.id, cb));
    }
    let checks_rc = Rc::new(checks);

    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build();
    let cancel = Button::with_label("Cancel");
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");
    {
        let win = win.clone();
        let state = state.clone();
        let checks = checks_rc.clone();
        let on_saved = Rc::new(on_saved);
        save.connect_clicked(move |_| {
            let selected: Vec<TemplateId> = checks
                .iter()
                .filter_map(|(id, cb)| if cb.is_active() { Some(*id) } else { None })
                .collect();
            let backend = state.borrow().backend.clone();
            let mut updated = match backend.borrow_mut().get_notebook(notebook_id) {
                Ok(n) => n,
                Err(e) => {
                    tracing::error!("failed to load notebook for save: {}", e);
                    win.close();
                    return;
                }
            };
            updated.assigned_templates = selected;
            if let Err(e) = backend.borrow_mut().update_notebook(&updated) {
                tracing::error!("failed to update notebook: {}", e);
            }
            (on_saved)();
            win.close();
        });
    }
    row.append(&cancel);
    row.append(&save);
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}

pub fn open_section_settings(
    parent: &ApplicationWindow,
    state: SharedState,
    section_id: SectionId,
    on_saved: Box<dyn Fn()>,
) {
    let win = modal(parent, "Section settings");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let title = Label::builder()
        .label("Section template scope")
        .halign(gtk4::Align::Start)
        .build();
    title.add_css_class("title-3");
    body.append(&title);

    let section = match state.borrow().backend.borrow_mut().get_section(section_id) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to load section: {}", e);
            return;
        }
    };

    let inherit = CheckButton::with_label("Inherit from notebook");
    inherit.set_active(section.allowed_templates.is_none());
    body.append(&inherit);

    let hint = Label::builder()
        .label("If enabled, this section uses the notebook's template list. Otherwise, choose which templates are allowed here.")
        .wrap(true)
        .halign(gtk4::Align::Start)
        .build();
    hint.add_css_class("dim-label");
    body.append(&hint);

    let scroller = ScrolledWindow::builder().hexpand(true).vexpand(true).build();
    let list = GtkBox::builder().orientation(Orientation::Vertical).spacing(2).build();
    scroller.set_child(Some(&list));
    body.append(&scroller);

    let templates = sorted_templates(&state);
    let allowed: std::collections::HashSet<TemplateId> = section
        .allowed_templates
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut checks: Vec<(TemplateId, CheckButton)> = Vec::with_capacity(templates.len());
    for t in templates {
        let cb = CheckButton::with_label(&t.name);
        cb.set_active(allowed.contains(&t.id));
        cb.set_sensitive(!inherit.is_active());
        list.append(&cb);
        checks.push((t.id, cb));
    }
    let checks_rc: Rc<RefCell<Vec<(TemplateId, CheckButton)>>> = Rc::new(RefCell::new(checks));

    {
        let checks = checks_rc.clone();
        inherit.connect_toggled(move |btn| {
            let on = btn.is_active();
            for (_, cb) in checks.borrow().iter() {
                cb.set_sensitive(!on);
            }
        });
    }

    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build();
    let cancel = Button::with_label("Cancel");
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");
    {
        let win = win.clone();
        let state = state.clone();
        let checks = checks_rc.clone();
        let inherit = inherit.clone();
        let on_saved = Rc::new(on_saved);
        save.connect_clicked(move |_| {
            let allowed = if inherit.is_active() {
                None
            } else {
                let selected: Vec<TemplateId> = checks
                    .borrow()
                    .iter()
                    .filter_map(|(id, cb)| if cb.is_active() { Some(*id) } else { None })
                    .collect();
                Some(selected)
            };
            let backend = state.borrow().backend.clone();
            let mut updated = match backend.borrow_mut().get_section(section_id) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("failed to load section for save: {}", e);
                    win.close();
                    return;
                }
            };
            updated.allowed_templates = allowed;
            if let Err(e) = backend.borrow_mut().update_section(&updated) {
                tracing::error!("failed to update section: {}", e);
            }
            (on_saved)();
            win.close();
        });
    }
    row.append(&cancel);
    row.append(&save);
    body.append(&row);

    win.set_child(Some(&body));
    win.present();
}

pub fn open_app_settings(
    parent: &ApplicationWindow,
    state: SharedState,
    on_saved: Box<dyn Fn()>,
) {
    use gtk4::{FileDialog, FileFilter};
    use std::path::PathBuf;

    let cfg = crate::config::load();
    let win = modal(parent, "App settings");
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    body.append(&Label::new(Some("No-page placeholder")));

    let path_label = Label::new(
        cfg.placeholder_image_path
            .as_ref()
            .and_then(|p| p.to_str())
            .or(Some("(no image set)")),
    );
    path_label.set_halign(gtk4::Align::Start);
    path_label.set_wrap(true);
    body.append(&path_label);

    let path_state: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(cfg.placeholder_image_path.clone()));

    let row1 = GtkBox::builder().orientation(Orientation::Horizontal).spacing(8).build();
    let pick_btn = Button::with_label("Choose image…");
    let clear_btn = Button::with_label("Clear");
    row1.append(&pick_btn);
    row1.append(&clear_btn);
    body.append(&row1);

    {
        let parent = parent.clone();
        let path_state = path_state.clone();
        let path_label = path_label.clone();
        pick_btn.connect_clicked(move |_| {
            let dialog = FileDialog::builder().title("Pick placeholder image").build();
            let filter = FileFilter::new();
            filter.add_mime_type("image/*");
            filter.set_name(Some("Images"));
            let store = gtk4::gio::ListStore::new::<FileFilter>();
            store.append(&filter);
            dialog.set_filters(Some(&store));
            let path_state = path_state.clone();
            let path_label = path_label.clone();
            dialog.open(Some(&parent), gtk4::gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(p) = file.path() {
                        path_label.set_text(p.to_str().unwrap_or(""));
                        *path_state.borrow_mut() = Some(p);
                    }
                }
            });
        });
    }
    {
        let path_state = path_state.clone();
        let path_label = path_label.clone();
        clear_btn.connect_clicked(move |_| {
            *path_state.borrow_mut() = None;
            path_label.set_text("(no image set)");
        });
    }

    body.append(&Label::new(Some("Placeholder text (used if no image)")));
    let text_entry = Entry::builder()
        .placeholder_text("Select a page to start drawing")
        .text(cfg.placeholder_text.as_deref().unwrap_or(""))
        .build();
    body.append(&text_entry);

    // ── Pen presets shortcut ─────────────────────────────────────────────
    let presets_sep = Separator::new(Orientation::Horizontal);
    body.append(&presets_sep);
    let presets_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    let presets_lbl = Label::builder()
        .label("Pen presets")
        .hexpand(true)
        .halign(gtk4::Align::Start)
        .build();
    let presets_btn = Button::with_label("Manage presets…");
    presets_row.append(&presets_lbl);
    presets_row.append(&presets_btn);
    body.append(&presets_row);
    {
        let parent = parent.clone();
        let state = state.clone();
        presets_btn.connect_clicked(move |_| {
            open_pen_presets_settings(&parent, state.clone());
        });
    }

    let row2 = GtkBox::builder().orientation(Orientation::Horizontal).spacing(8).halign(gtk4::Align::End).build();
    let cancel = Button::with_label("Cancel");
    let save = Button::with_label("Save");
    row2.append(&cancel);
    row2.append(&save);
    body.append(&row2);

    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    {
        let win = win.clone();
        let state = state.clone();
        let path_state = path_state.clone();
        save.connect_clicked(move |_| {
            let mut new_cfg = crate::config::load();
            new_cfg.placeholder_image_path = path_state.borrow().clone();
            new_cfg.placeholder_text = {
                let t = text_entry.text().to_string();
                if t.trim().is_empty() { None } else { Some(t) }
            };
            if let Err(e) = crate::config::save(&new_cfg) {
                tracing::error!("save config: {}", e);
            }
            crate::state::reload_placeholder(&state);
            (on_saved)();
            win.close();
        });
    }

    win.set_child(Some(&body));
    win.present();
}

// ── Pen presets settings dialog ───────────────────────────────────────────────

/// Small colored swatch `DrawingArea` (24×24) for a given RGBA.
fn make_color_swatch(color_rgba: [u8; 4]) -> DrawingArea {
    let [r, g, b, a] = color_rgba;
    let rf = r as f64 / 255.0;
    let gf = g as f64 / 255.0;
    let bf = b as f64 / 255.0;
    let af = a as f64 / 255.0;
    let da = DrawingArea::new();
    da.set_size_request(24, 24);
    da.set_draw_func(move |_, cr, w, h| {
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        let radius = (cx.min(cy) - 1.0).max(1.0);
        cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
        cr.set_source_rgba(rf, gf, bf, af);
        let _ = cr.fill_preserve();
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.25);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
    });
    da
}

/// Open the "Pen presets" management dialog.
///
/// Shows a table of presets (swatch + name + width) with up/down reorder,
/// delete, inline edit, and an "Add preset" button that captures the current
/// pen color + width.
pub fn open_pen_presets_settings(
    parent: &ApplicationWindow,
    state: SharedState,
) {
    let win = modal(parent, "Pen presets");

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let title_lbl = Label::builder()
        .label("Pen presets")
        .halign(gtk4::Align::Start)
        .build();
    title_lbl.add_css_class("title-3");
    body.append(&title_lbl);

    let hint_lbl = Label::builder()
        .label("One-click color + width combos shown on the toolbar. Click 'Add current pen' to save the active pen settings.")
        .wrap(true)
        .halign(gtk4::Align::Start)
        .build();
    hint_lbl.add_css_class("dim-label");
    body.append(&hint_lbl);

    // Shared mutable preset list.
    let presets: Rc<RefCell<Vec<PenPreset>>> =
        Rc::new(RefCell::new(crate::config::load().pen_presets));

    // Scrolled list box.
    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let list_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .build();
    scroller.set_child(Some(&list_box));
    body.append(&scroller);

    // ── Rebuild-slot pattern ──────────────────────────────────────────────
    // Buttons inside the rebuilt list need to trigger another rebuild.
    // Use a shared slot filled after `real_rebuild` is defined.
    let rebuild_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    let real_rebuild = {
        let list_box = list_box.clone();
        let presets = presets.clone();
        let win = win.clone();
        let rebuild_slot = rebuild_slot.clone();

        Rc::new(move || {
            // Remove all children.
            while let Some(child) = list_box.first_child() {
                list_box.remove(&child);
            }

            let items = presets.borrow().clone();
            let len = items.len();
            for (idx, preset) in items.iter().enumerate() {
                let row = GtkBox::builder()
                    .orientation(Orientation::Horizontal)
                    .spacing(6)
                    .build();

                row.append(&make_color_swatch(preset.color_rgba));

                let name_lbl = Label::builder()
                    .label(&preset.name)
                    .hexpand(true)
                    .halign(gtk4::Align::Start)
                    .build();
                row.append(&name_lbl);

                let width_lbl = Label::builder()
                    .label(format!("{:.1} mm", preset.width_mm))
                    .halign(gtk4::Align::End)
                    .build();
                width_lbl.add_css_class("dim-label");
                row.append(&width_lbl);

                // ── Up ────────────────────────────────────────────────────
                let up_btn = Button::builder()
                    .icon_name("go-up-symbolic")
                    .tooltip_text("Move up")
                    .sensitive(idx > 0)
                    .build();
                up_btn.add_css_class("flat");
                {
                    let presets = presets.clone();
                    let slot = rebuild_slot.clone();
                    up_btn.connect_clicked(move |_| {
                        let mut v = presets.borrow_mut();
                        if idx > 0 {
                            v.swap(idx - 1, idx);
                        }
                        drop(v);
                        save_presets_from(&presets);
                        if let Some(r) = slot.borrow().as_ref() {
                            r();
                        }
                    });
                }
                row.append(&up_btn);

                // ── Down ──────────────────────────────────────────────────
                let down_btn = Button::builder()
                    .icon_name("go-down-symbolic")
                    .tooltip_text("Move down")
                    .sensitive(idx + 1 < len)
                    .build();
                down_btn.add_css_class("flat");
                {
                    let presets = presets.clone();
                    let slot = rebuild_slot.clone();
                    down_btn.connect_clicked(move |_| {
                        let mut v = presets.borrow_mut();
                        if idx + 1 < v.len() {
                            v.swap(idx, idx + 1);
                        }
                        drop(v);
                        save_presets_from(&presets);
                        if let Some(r) = slot.borrow().as_ref() {
                            r();
                        }
                    });
                }
                row.append(&down_btn);

                // ── Edit ──────────────────────────────────────────────────
                let edit_btn = Button::builder()
                    .icon_name("document-edit-symbolic")
                    .tooltip_text("Edit preset")
                    .build();
                edit_btn.add_css_class("flat");
                {
                    let presets = presets.clone();
                    let slot = rebuild_slot.clone();
                    let win = win.clone();
                    let preset_clone = preset.clone();
                    edit_btn.connect_clicked(move |_| {
                        open_preset_editor(
                            &win,
                            idx,
                            preset_clone.clone(),
                            presets.clone(),
                            slot.clone(),
                        );
                    });
                }
                row.append(&edit_btn);

                // ── Delete ────────────────────────────────────────────────
                let delete_btn = Button::builder()
                    .icon_name("user-trash-symbolic")
                    .tooltip_text("Delete preset")
                    .build();
                delete_btn.add_css_class("flat");
                {
                    let presets = presets.clone();
                    let slot = rebuild_slot.clone();
                    delete_btn.connect_clicked(move |_| {
                        let mut v = presets.borrow_mut();
                        if idx < v.len() {
                            v.remove(idx);
                        }
                        drop(v);
                        save_presets_from(&presets);
                        if let Some(r) = slot.borrow().as_ref() {
                            r();
                        }
                    });
                }
                row.append(&delete_btn);

                list_box.append(&row);
            }
        })
    };

    // Fill the slot so button callbacks can trigger rebuilds.
    *rebuild_slot.borrow_mut() = Some(real_rebuild.clone());

    // Initial population.
    real_rebuild();

    // ── "Add current pen" button ──────────────────────────────────────────
    let add_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    let add_btn = Button::builder()
        .icon_name("list-add-symbolic")
        .label("Add current pen")
        .tooltip_text("Save the active pen color and width as a new preset")
        .build();
    add_btn.add_css_class("suggested-action");
    {
        let presets = presets.clone();
        let state = state.clone();
        let rebuild_slot = rebuild_slot.clone();
        add_btn.connect_clicked(move |_| {
            let s = state.borrow();
            let c = s.pen.color;
            let w = s.pen.base_width;
            drop(s);
            let mut v = presets.borrow_mut();
            let name = format!("Preset {}", v.len() + 1);
            v.push(PenPreset {
                name,
                color_rgba: [c.r, c.g, c.b, c.a],
                width_mm: w,
            });
            drop(v);
            save_presets_from(&presets);
            if let Some(r) = rebuild_slot.borrow().as_ref() {
                r();
            }
        });
    }
    add_row.append(&add_btn);
    body.append(&add_row);

    // Close button.
    let close_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(gtk4::Align::End)
        .build();
    let close_btn = Button::with_label("Close");
    close_row.append(&close_btn);
    body.append(&close_row);
    {
        let win = win.clone();
        close_btn.connect_clicked(move |_| win.close());
    }

    win.set_child(Some(&body));
    win.present();
}

/// Save the current preset list to config on disk.
fn save_presets_from(presets: &Rc<RefCell<Vec<PenPreset>>>) {
    let mut cfg = crate::config::load();
    cfg.pen_presets = presets.borrow().clone();
    if let Err(e) = crate::config::save(&cfg) {
        tracing::warn!("Failed to save pen presets: {}", e);
    }
}

/// Sub-dialog for editing a single preset (name, color, width).
fn open_preset_editor(
    parent: &Window,
    idx: usize,
    preset: PenPreset,
    presets: Rc<RefCell<Vec<PenPreset>>>,
    rebuild_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) {
    let ed_win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Edit preset")
        .default_width(320)
        .default_height(220)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    // Name.
    body.append(
        &Label::builder()
            .label("Name")
            .halign(gtk4::Align::Start)
            .build(),
    );
    let name_entry = Entry::builder().text(preset.name.as_str()).build();
    body.append(&name_entry);

    // Color.
    body.append(
        &Label::builder()
            .label("Color")
            .halign(gtk4::Align::Start)
            .build(),
    );
    let [r, g, b, a] = preset.color_rgba;
    let initial_rgba = gtk4::gdk::RGBA::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    );
    let color_dialog = ColorDialog::builder().with_alpha(true).build();
    let color_btn = ColorDialogButton::new(Some(color_dialog));
    color_btn.set_rgba(&initial_rgba);
    body.append(&color_btn);

    // Width.
    body.append(
        &Label::builder()
            .label("Width (mm)")
            .halign(gtk4::Align::Start)
            .build(),
    );
    let adj = gtk4::Adjustment::new(preset.width_mm, 0.5, 20.0, 0.5, 1.0, 0.0);
    let width_spin = SpinButton::new(Some(&adj), 0.5, 1);
    body.append(&width_spin);

    // Buttons.
    let btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build();
    let cancel_btn = Button::with_label("Cancel");
    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    btn_row.append(&cancel_btn);
    btn_row.append(&save_btn);
    body.append(&btn_row);

    {
        let ed_win = ed_win.clone();
        cancel_btn.connect_clicked(move |_| ed_win.close());
    }
    {
        let ed_win = ed_win.clone();
        save_btn.connect_clicked(move |_| {
            let new_name = name_entry.text().to_string();
            let rgba = color_btn.rgba();
            let new_color = [
                (rgba.red() * 255.0) as u8,
                (rgba.green() * 255.0) as u8,
                (rgba.blue() * 255.0) as u8,
                (rgba.alpha() * 255.0) as u8,
            ];
            let new_width = width_spin.value();
            {
                let mut v = presets.borrow_mut();
                if let Some(p) = v.get_mut(idx) {
                    p.name = new_name;
                    p.color_rgba = new_color;
                    p.width_mm = new_width;
                }
            }
            save_presets_from(&presets);
            if let Some(r) = rebuild_slot.borrow().as_ref() {
                r();
            }
            ed_win.close();
        });
    }

    ed_win.set_child(Some(&body));
    ed_win.present();
}
