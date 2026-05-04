use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, CheckButton, ColorDialog, ColorDialogButton,
    DrawingArea, Entry, Label, Orientation, ScrolledWindow, Separator, SpinButton, Window,
};
use libadwaita as adw;
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
    use adw::prelude::*;
    use gtk4::{FileDialog, FileFilter};
    use std::path::PathBuf;

    let cfg = crate::config::load();
    let prefs = adw::PreferencesWindow::builder()
        .title("App settings")
        .transient_for(parent)
        .modal(true)
        .default_width(560)
        .default_height(620)
        .build();

    let on_saved = Rc::new(on_saved);
    let path_state: Rc<RefCell<Option<PathBuf>>> =
        Rc::new(RefCell::new(cfg.placeholder_image_path.clone()));

    // Reusable persist closure. Reads the current widget values, merges
    // them into a freshly-loaded config, persists, and notifies the caller.
    let persist: Rc<dyn Fn(&PersistInputs)> = {
        let state = state.clone();
        let on_saved = on_saved.clone();
        Rc::new(move |inputs: &PersistInputs| {
            let mut new_cfg = crate::config::load();
            new_cfg.placeholder_image_path = inputs.image_path.clone();
            new_cfg.placeholder_text = match inputs.text.as_str() {
                "" => None,
                t => Some(t.to_string()),
            };
            new_cfg.developer_mode = inputs.developer_mode;
            if let Err(e) = crate::config::save(&new_cfg) {
                tracing::error!("save config: {e}");
            }
            crate::state::reload_placeholder(&state);
            (on_saved)();
        })
    };

    let page = adw::PreferencesPage::new();

    // ── Empty state ─────────────────────────────────────────────────
    let empty_group = adw::PreferencesGroup::builder()
        .title("Empty state")
        .description("How the canvas reads when no page is selected.")
        .build();

    let image_row = adw::ActionRow::builder()
        .title("Splash image")
        .subtitle(
            cfg.placeholder_image_path
                .as_ref()
                .and_then(|p| p.to_str())
                .unwrap_or("(no image set)"),
        )
        .build();
    let pick_btn = Button::with_label("Choose…");
    pick_btn.add_css_class("flat");
    pick_btn.set_valign(gtk4::Align::Center);
    let clear_btn = Button::with_label("Clear");
    clear_btn.add_css_class("flat");
    clear_btn.set_valign(gtk4::Align::Center);
    image_row.add_suffix(&pick_btn);
    image_row.add_suffix(&clear_btn);
    empty_group.add(&image_row);

    let text_row = adw::EntryRow::builder()
        .title("Placeholder text (used if no image)")
        .text(cfg.placeholder_text.as_deref().unwrap_or(""))
        .build();
    empty_group.add(&text_row);

    page.add(&empty_group);

    // ── Display ─────────────────────────────────────────────────────
    let display_group = adw::PreferencesGroup::builder()
        .title("Display")
        .description("Font used for headers, the wordmark, and notebook titles.")
        .build();
    let font_options = crate::config::DISPLAY_FONT_OPTIONS;
    let font_labels: Vec<&str> = font_options.iter().map(|(_, l, _)| *l).collect();
    let font_model = gtk4::StringList::new(&font_labels);
    let active_idx = font_options
        .iter()
        .position(|(slug, _, _)| {
            cfg.display_font.as_deref() == Some(*slug)
                || (cfg.display_font.is_none() && *slug == "default")
        })
        .unwrap_or(0) as u32;
    let font_row = adw::ComboRow::builder()
        .title("Display font")
        .subtitle("Falls back through a serif chain if the chosen face isn't installed.")
        .model(&font_model)
        .selected(active_idx)
        .build();
    display_group.add(&font_row);
    page.add(&display_group);
    {
        let font_row = font_row.clone();
        font_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            let slug = crate::config::DISPLAY_FONT_OPTIONS
                .get(idx)
                .map(|(s, _, _)| *s)
                .unwrap_or("default");
            let mut new_cfg = crate::config::load();
            new_cfg.display_font = if slug == "default" {
                None
            } else {
                Some(slug.to_string())
            };
            if let Err(e) = crate::config::save(&new_cfg) {
                tracing::error!("save display_font: {e}");
            }
            crate::reload_css();
        });
    }

    // ── Drawing ─────────────────────────────────────────────────────
    let drawing_group = adw::PreferencesGroup::builder()
        .title("Drawing")
        .build();
    let presets_row = adw::ActionRow::builder()
        .title("Pen presets")
        .subtitle("Saved width / opacity / color combos for the toolbar")
        .activatable(true)
        .build();
    let presets_arrow = gtk4::Image::from_icon_name("go-next-symbolic");
    presets_arrow.add_css_class("dim-label");
    presets_row.add_suffix(&presets_arrow);
    drawing_group.add(&presets_row);
    page.add(&drawing_group);
    {
        let parent = parent.clone();
        let state = state.clone();
        presets_row.connect_activated(move |_| {
            open_pen_presets_settings(&parent, state.clone());
        });
    }

    // ── Developer ────────────────────────────────────────────────────
    let dev_group = adw::PreferencesGroup::builder()
        .title("Developer")
        .build();
    let dev_row = adw::SwitchRow::builder()
        .title("Developer mode")
        .subtitle(
            "Enables the per-tool Tool Settings button in the menu and a floating Tool Options \
             panel that follows the currently-selected tool.",
        )
        .active(cfg.developer_mode)
        .build();
    dev_group.add(&dev_row);
    page.add(&dev_group);

    prefs.add(&page);

    // Wire change signals to auto-save (adw convention — no Save/Cancel).
    let snapshot_inputs = {
        let path_state = path_state.clone();
        let text_row = text_row.clone();
        let dev_row = dev_row.clone();
        move || PersistInputs {
            image_path: path_state.borrow().clone(),
            text: text_row.text().to_string(),
            developer_mode: dev_row.is_active(),
        }
    };

    {
        let parent = parent.clone();
        let path_state = path_state.clone();
        let image_row = image_row.clone();
        let persist = persist.clone();
        let snapshot_inputs = snapshot_inputs.clone();
        pick_btn.connect_clicked(move |_| {
            let dialog = FileDialog::builder().title("Pick placeholder image").build();
            let filter = FileFilter::new();
            filter.add_mime_type("image/*");
            filter.set_name(Some("Images"));
            let store = gtk4::gio::ListStore::new::<FileFilter>();
            store.append(&filter);
            dialog.set_filters(Some(&store));
            let path_state = path_state.clone();
            let image_row = image_row.clone();
            let persist = persist.clone();
            let snapshot_inputs = snapshot_inputs.clone();
            dialog.open(Some(&parent), gtk4::gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(p) = file.path() {
                        image_row.set_subtitle(p.to_str().unwrap_or(""));
                        *path_state.borrow_mut() = Some(p);
                        persist(&snapshot_inputs());
                    }
                }
            });
        });
    }
    {
        let path_state = path_state.clone();
        let image_row = image_row.clone();
        let persist = persist.clone();
        let snapshot_inputs = snapshot_inputs.clone();
        clear_btn.connect_clicked(move |_| {
            *path_state.borrow_mut() = None;
            image_row.set_subtitle("(no image set)");
            persist(&snapshot_inputs());
        });
    }
    {
        let persist = persist.clone();
        let snapshot_inputs = snapshot_inputs.clone();
        text_row.connect_changed(move |_| {
            persist(&snapshot_inputs());
        });
    }
    {
        let persist = persist.clone();
        let snapshot_inputs = snapshot_inputs.clone();
        dev_row.connect_active_notify(move |_| {
            persist(&snapshot_inputs());
        });
    }

    prefs.present();
}

struct PersistInputs {
    image_path: Option<std::path::PathBuf>,
    text: String,
    developer_mode: bool,
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

// ---------------------------------------------------------------------------
// Per-tool brush settings (developer mode only)
// ---------------------------------------------------------------------------

const BLEND_MODES: &[(&str, journal_core::BlendMode)] = &[
    ("Normal", journal_core::BlendMode::Normal),
    ("Multiply", journal_core::BlendMode::Multiply),
    ("Screen", journal_core::BlendMode::Screen),
    ("Overlay", journal_core::BlendMode::Overlay),
    ("Darken", journal_core::BlendMode::Darken),
    ("Lighten", journal_core::BlendMode::Lighten),
    ("Erase", journal_core::BlendMode::Erase),
];

#[allow(dead_code)]
const BRUSH_STYLES: &[(&str, journal_core::BrushStyle)] = &[
    ("Pen (smooth)", journal_core::BrushStyle::Pen),
    ("Pencil (sharp + tilt-shading)", journal_core::BrushStyle::Pencil),
    ("Highlighter", journal_core::BrushStyle::Highlighter),
    ("Paintbrush (3-pass halo)", journal_core::BrushStyle::Paintbrush),
    ("Spray Can", journal_core::BrushStyle::SprayCan),
    ("Calligraphy (variable-width polygon)", journal_core::BrushStyle::Calligraphy),
];

fn blend_index(b: journal_core::BlendMode) -> u32 {
    BLEND_MODES.iter().position(|(_, m)| *m == b).unwrap_or(0) as u32
}

#[allow(dead_code)]
fn style_index(s: journal_core::BrushStyle) -> u32 {
    BRUSH_STYLES.iter().position(|(_, m)| *m == s).unwrap_or(0) as u32
}

pub fn open_tool_settings(parent: &ApplicationWindow, state: SharedState) {
    use gtk4::{DropDown, StringList};

    let dialog = modal(parent, "Tool Settings (developer)");
    dialog.set_default_height(560);
    dialog.set_default_width(620);

    let scroll = ScrolledWindow::builder().hexpand(true).vexpand(true).build();
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(12)
        .margin_end(12)
        .build();

    let header = Label::builder()
        .label(
            "Per-tool brush-pipeline overrides. Width / opacity multipliers stack on top of the \
             toolbar's pen settings; blend mode and brush style override how the renderer \
             dispatches the stroke. Defaults reset to the built-in values.",
        )
        .wrap(true)
        .xalign(0.0)
        .build();
    body.append(&header);

    for tool in crate::tool_settings::settable_tools().iter().copied() {
        let key = match crate::tool_settings::tool_key(tool) {
            Some(k) => k.to_string(),
            None => continue,
        };
        let initial = state
            .borrow()
            .tool_settings
            .get(&key)
            .copied()
            .unwrap_or_else(|| crate::tool_settings::default_settings_for(tool));

        let row = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .build();
        let title = Label::builder()
            .label(&format!("<b>{}</b>", crate::tool_settings::tool_label(tool)))
            .use_markup(true)
            .xalign(0.0)
            .build();
        row.append(&title);

        let grid = gtk4::Grid::builder()
            .row_spacing(4)
            .column_spacing(10)
            .build();

        // Default base width (mm) — applied when the tool is selected
        grid.attach(
            &Label::builder().label("Default size (mm)").xalign(1.0).build(),
            0,
            0,
            1,
            1,
        );
        let bw_spin = SpinButton::with_range(0.1, 60.0, 0.5);
        bw_spin.set_digits(1);
        bw_spin.set_value(initial.default_base_width);
        bw_spin.set_hexpand(true);
        grid.attach(&bw_spin, 1, 0, 1, 1);

        // Opacity multiplier
        grid.attach(
            &Label::builder().label("Opacity ×").xalign(1.0).build(),
            0,
            1,
            1,
            1,
        );
        let op_spin = SpinButton::with_range(0.0, 2.0, 0.05);
        op_spin.set_digits(2);
        op_spin.set_value(initial.opacity_mult as f64);
        op_spin.set_hexpand(true);
        grid.attach(&op_spin, 1, 1, 1, 1);

        // Width multiplier
        grid.attach(
            &Label::builder().label("Width ×").xalign(1.0).build(),
            0,
            2,
            1,
            1,
        );
        let w_spin = SpinButton::with_range(0.05, 12.0, 0.1);
        w_spin.set_digits(2);
        w_spin.set_value(initial.width_mult);
        w_spin.set_hexpand(true);
        grid.attach(&w_spin, 1, 2, 1, 1);

        // Blend mode
        grid.attach(
            &Label::builder().label("Blend").xalign(1.0).build(),
            0,
            3,
            1,
            1,
        );
        let blend_strs = StringList::new(
            &BLEND_MODES.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        );
        let blend_dd = DropDown::builder().model(&blend_strs).hexpand(true).build();
        blend_dd.set_selected(blend_index(initial.blend_mode));
        grid.attach(&blend_dd, 1, 3, 1, 1);

        row.append(&grid);

        // Reset button
        let reset_btn = Button::with_label("Reset to defaults");
        {
            let bw_spin = bw_spin.clone();
            let op_spin = op_spin.clone();
            let w_spin = w_spin.clone();
            let blend_dd = blend_dd.clone();
            reset_btn.connect_clicked(move |_| {
                let d = crate::tool_settings::default_settings_for(tool);
                bw_spin.set_value(d.default_base_width);
                op_spin.set_value(d.opacity_mult as f64);
                w_spin.set_value(d.width_mult);
                blend_dd.set_selected(blend_index(d.blend_mode));
            });
        }
        row.append(&reset_btn);
        row.append(&Separator::new(Orientation::Horizontal));

        // Live updates: write back to state on every value change so the
        // canvas reflects the change immediately. Persist to config is
        // batched on dialog close.
        let apply = {
            let state = state.clone();
            let key = key.clone();
            let bw_spin = bw_spin.clone();
            let op_spin = op_spin.clone();
            let w_spin = w_spin.clone();
            let blend_dd = blend_dd.clone();
            move || {
                let canonical = crate::tool_settings::default_settings_for(tool).brush_style;
                let s = crate::tool_settings::ToolSettings {
                    opacity_mult: op_spin.value() as f32,
                    width_mult: w_spin.value(),
                    blend_mode: BLEND_MODES[blend_dd.selected() as usize].1,
                    brush_style: canonical,
                    default_base_width: bw_spin.value(),
                };
                state.borrow_mut().tool_settings.insert(key.clone(), s);
            }
        };
        {
            let apply = apply.clone();
            bw_spin.connect_value_changed(move |_| apply());
        }
        {
            let apply = apply.clone();
            op_spin.connect_value_changed(move |_| apply());
        }
        {
            let apply = apply.clone();
            w_spin.connect_value_changed(move |_| apply());
        }
        {
            let apply = apply.clone();
            blend_dd.connect_selected_notify(move |_| apply());
        }

        body.append(&row);
    }

    scroll.set_child(Some(&body));
    dialog.set_child(Some(&scroll));

    // Persist to config when the dialog closes.
    {
        let state = state.clone();
        dialog.connect_close_request(move |_| {
            let mut cfg = crate::config::load();
            cfg.tool_settings = state.borrow().tool_settings.clone();
            cfg.brush_params = Some(state.borrow().brush_params);
            if let Err(e) = crate::config::save(&cfg) {
                tracing::warn!("save tool settings: {e}");
            }
            gtk4::glib::Propagation::Proceed
        });
    }

    // ----------------------------------------------------------------
    // Per-brush-style internal tuning (the "edit brush guts" section)
    // ----------------------------------------------------------------
    body.append(&Separator::new(Orientation::Horizontal));
    body.append(
        &Label::builder()
            .label("<b>Brush internals</b>")
            .use_markup(true)
            .xalign(0.0)
            .build(),
    );
    body.append(
        &Label::builder()
            .label(
                "These knobs change the shape of each brush style globally. Editing the \
                 calligraphy section affects every stroke (existing + future) drawn with \
                 BrushStyle::Calligraphy, regardless of which tool routed to it.",
            )
            .wrap(true)
            .xalign(0.0)
            .build(),
    );

    add_brush_param_sections(&body, &state);

    dialog.present();
}

fn add_brush_param_sections(body: &GtkBox, state: &SharedState) {
    use journal_canvas::vello_renderer::{
        BrushParams, CalligraphyParams, PaintbrushParams, PenParams, PencilParams, SprayParams,
    };

    fn row_label(label: &str) -> Label {
        Label::builder().label(label).xalign(1.0).build()
    }

    fn spin_for(min: f64, max: f64, step: f64, digits: u32, val: f64) -> SpinButton {
        let s = SpinButton::with_range(min, max, step);
        s.set_digits(digits);
        s.set_value(val);
        s.set_hexpand(true);
        s
    }

    let read_pen = || -> PenParams { state.borrow().brush_params.pen };
    let read_pencil = || -> PencilParams { state.borrow().brush_params.pencil };
    let read_paintbrush = || -> PaintbrushParams { state.borrow().brush_params.paintbrush };
    let read_spray = || -> SprayParams { state.borrow().brush_params.spray };
    let read_calligraphy = || -> CalligraphyParams { state.borrow().brush_params.calligraphy };

    fn make_section(title: &str) -> (GtkBox, gtk4::Grid) {
        let row = GtkBox::builder().orientation(Orientation::Vertical).spacing(6).build();
        row.append(
            &Label::builder()
                .label(&format!("<b>{}</b>", title))
                .use_markup(true)
                .xalign(0.0)
                .build(),
        );
        let grid = gtk4::Grid::builder().row_spacing(4).column_spacing(10).build();
        row.append(&grid);
        (row, grid)
    }

    // Pen / Highlighter (same params apply to both since draw_smooth handles both).
    {
        let (row, grid) = make_section("Pen / Highlighter (draw_smooth)");
        let p = read_pen();
        grid.attach(&row_label("Width floor"), 0, 0, 1, 1);
        let floor = spin_for(0.0, 1.5, 0.05, 2, p.width_floor);
        grid.attach(&floor, 1, 0, 1, 1);
        grid.attach(&row_label("Pressure amplitude"), 0, 1, 1, 1);
        let amp = spin_for(0.0, 1.5, 0.05, 2, p.width_pressure_amplitude);
        grid.attach(&amp, 1, 1, 1, 1);

        let reset = Button::with_label("Reset");
        {
            let floor = floor.clone();
            let amp = amp.clone();
            reset.connect_clicked(move |_| {
                let d = PenParams::default();
                floor.set_value(d.width_floor);
                amp.set_value(d.width_pressure_amplitude);
            });
        }
        row.append(&reset);
        row.append(&Separator::new(Orientation::Horizontal));

        let apply: Rc<dyn Fn()> = {
            let state = state.clone();
            let floor = floor.clone();
            let amp = amp.clone();
            Rc::new(move || {
                let prev = state.borrow().brush_params.pen;
                state.borrow_mut().brush_params.pen = PenParams {
                    width_floor: floor.value(),
                    width_pressure_amplitude: amp.value(),
                    ..prev
                };
            })
        };
        {
            let a = apply.clone();
            floor.connect_value_changed(move |_| a());
        }
        {
            let a = apply.clone();
            amp.connect_value_changed(move |_| a());
        }
        body.append(&row);
    }

    // Pencil
    {
        let (row, grid) = make_section("Pencil");
        let p = read_pencil();
        grid.attach(&row_label("Core min (mm)"), 0, 0, 1, 1);
        let cmin = spin_for(0.05, 3.0, 0.05, 2, p.core_clamp_min);
        grid.attach(&cmin, 1, 0, 1, 1);
        grid.attach(&row_label("Core max (mm)"), 0, 1, 1, 1);
        let cmax = spin_for(0.05, 5.0, 0.05, 2, p.core_clamp_max);
        grid.attach(&cmax, 1, 1, 1, 1);
        grid.attach(&row_label("Tilt threshold"), 0, 2, 1, 1);
        let thr = spin_for(0.0, 1.0, 0.02, 2, p.tilt_threshold);
        grid.attach(&thr, 1, 2, 1, 1);
        grid.attach(&row_label("Tilt band width ×"), 0, 3, 1, 1);
        let tband = spin_for(0.0, 30.0, 0.5, 1, p.tilt_band_mult);
        grid.attach(&tband, 1, 3, 1, 1);
        grid.attach(&row_label("Tilt alpha scale"), 0, 4, 1, 1);
        let talpha = spin_for(0.0, 1.0, 0.02, 2, p.tilt_alpha_scale);
        grid.attach(&talpha, 1, 4, 1, 1);

        let reset = Button::with_label("Reset");
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
        row.append(&reset);
        row.append(&Separator::new(Orientation::Horizontal));

        let apply: Rc<dyn Fn()> = {
            let state = state.clone();
            let (cmin, cmax, thr, tband, talpha) =
                (cmin.clone(), cmax.clone(), thr.clone(), tband.clone(), talpha.clone());
            Rc::new(move || {
                let prev = state.borrow().brush_params.pencil;
                state.borrow_mut().brush_params.pencil = PencilParams {
                    core_clamp_min: cmin.value(),
                    core_clamp_max: cmax.value(),
                    tilt_threshold: thr.value(),
                    tilt_band_mult: tband.value(),
                    tilt_alpha_scale: talpha.value(),
                    ..prev
                };
            })
        };
        for s in [&cmin, &cmax, &thr, &tband, &talpha] {
            let a = apply.clone();
            s.connect_value_changed(move |_| a());
        }
        body.append(&row);
    }

    // Paintbrush
    {
        let (row, grid) = make_section("Paintbrush");
        let p = read_paintbrush();
        grid.attach(&row_label("Halo width ×"), 0, 0, 1, 1);
        let hw = spin_for(1.0, 5.0, 0.05, 2, p.halo_width_mult);
        grid.attach(&hw, 1, 0, 1, 1);
        grid.attach(&row_label("Outer halo ×"), 0, 1, 1, 1);
        let oh = spin_for(0.5, 4.0, 0.05, 2, p.outer_halo_mult);
        grid.attach(&oh, 1, 1, 1, 1);
        grid.attach(&row_label("Mid halo ×"), 0, 2, 1, 1);
        let mh = spin_for(0.2, 3.0, 0.05, 2, p.mid_halo_mult);
        grid.attach(&mh, 1, 2, 1, 1);
        grid.attach(&row_label("Outer alpha"), 0, 3, 1, 1);
        let oa = spin_for(0.0, 1.0, 0.01, 2, p.outer_alpha);
        grid.attach(&oa, 1, 3, 1, 1);
        grid.attach(&row_label("Mid alpha"), 0, 4, 1, 1);
        let ma = spin_for(0.0, 1.0, 0.01, 2, p.mid_alpha);
        grid.attach(&ma, 1, 4, 1, 1);
        grid.attach(&row_label("Core alpha"), 0, 5, 1, 1);
        let ca = spin_for(0.0, 1.0, 0.01, 2, p.core_alpha);
        grid.attach(&ca, 1, 5, 1, 1);

        let reset = Button::with_label("Reset");
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
        row.append(&reset);
        row.append(&Separator::new(Orientation::Horizontal));

        let apply: Rc<dyn Fn()> = {
            let state = state.clone();
            let (hw, oh, mh, oa, ma, ca) =
                (hw.clone(), oh.clone(), mh.clone(), oa.clone(), ma.clone(), ca.clone());
            Rc::new(move || {
                let prev = state.borrow().brush_params.paintbrush;
                state.borrow_mut().brush_params.paintbrush = PaintbrushParams {
                    halo_width_mult: hw.value(),
                    outer_halo_mult: oh.value(),
                    mid_halo_mult: mh.value(),
                    outer_alpha: oa.value(),
                    mid_alpha: ma.value(),
                    core_alpha: ca.value(),
                    ..prev
                };
            })
        };
        for s in [&hw, &oh, &mh, &oa, &ma, &ca] {
            let a = apply.clone();
            s.connect_value_changed(move |_| a());
        }
        body.append(&row);
    }

    // Spray
    {
        let (row, grid) = make_section("Spray Can");
        let p = read_spray();
        grid.attach(&row_label("Dots per point"), 0, 0, 1, 1);
        let dpp = spin_for(1.0, 200.0, 1.0, 0, p.dots_per_point as f64);
        grid.attach(&dpp, 1, 0, 1, 1);
        grid.attach(&row_label("Dot radius factor"), 0, 1, 1, 1);
        let drf = spin_for(0.01, 1.0, 0.01, 2, p.dot_radius_factor);
        grid.attach(&drf, 1, 1, 1, 1);
        grid.attach(&row_label("Min dot radius"), 0, 2, 1, 1);
        let mdr = spin_for(0.05, 4.0, 0.05, 2, p.min_dot_radius);
        grid.attach(&mdr, 1, 2, 1, 1);

        let reset = Button::with_label("Reset");
        {
            let (dpp, drf, mdr) = (dpp.clone(), drf.clone(), mdr.clone());
            reset.connect_clicked(move |_| {
                let d = SprayParams::default();
                dpp.set_value(d.dots_per_point as f64);
                drf.set_value(d.dot_radius_factor);
                mdr.set_value(d.min_dot_radius);
            });
        }
        row.append(&reset);
        row.append(&Separator::new(Orientation::Horizontal));

        let apply: Rc<dyn Fn()> = {
            let state = state.clone();
            let (dpp, drf, mdr) = (dpp.clone(), drf.clone(), mdr.clone());
            Rc::new(move || {
                let prev = state.borrow().brush_params.spray;
                state.borrow_mut().brush_params.spray = SprayParams {
                    dots_per_point: dpp.value() as u32,
                    dot_radius_factor: drf.value(),
                    min_dot_radius: mdr.value(),
                    ..prev
                };
            })
        };
        for s in [&dpp, &drf, &mdr] {
            let a = apply.clone();
            s.connect_value_changed(move |_| a());
        }
        body.append(&row);
    }

    // Calligraphy
    {
        let (row, grid) = make_section("Calligraphy");
        let p = read_calligraphy();
        grid.attach(&row_label("Nib angle (°)"), 0, 0, 1, 1);
        let nib = spin_for(-90.0, 90.0, 1.0, 0, p.nib_angle_deg);
        grid.attach(&nib, 1, 0, 1, 1);
        grid.attach(&row_label("Min ratio"), 0, 1, 1, 1);
        let mr = spin_for(0.0, 1.0, 0.02, 2, p.min_ratio);
        grid.attach(&mr, 1, 1, 1, 1);
        grid.attach(&row_label("Resample step ×"), 0, 2, 1, 1);
        let rs = spin_for(0.05, 2.0, 0.05, 2, p.resample_step_mult);
        grid.attach(&rs, 1, 2, 1, 1);
        let smooth_chk = CheckButton::with_label("Smooth outline (quad-through-midpoints)");
        smooth_chk.set_active(p.smooth_outline);
        grid.attach(&smooth_chk, 0, 3, 2, 1);

        let reset = Button::with_label("Reset");
        {
            let (nib, mr, rs, smooth_chk) = (nib.clone(), mr.clone(), rs.clone(), smooth_chk.clone());
            reset.connect_clicked(move |_| {
                let d = CalligraphyParams::default();
                nib.set_value(d.nib_angle_deg);
                mr.set_value(d.min_ratio);
                rs.set_value(d.resample_step_mult);
                smooth_chk.set_active(d.smooth_outline);
            });
        }
        row.append(&reset);
        row.append(&Separator::new(Orientation::Horizontal));

        let apply: Rc<dyn Fn()> = {
            let state = state.clone();
            let (nib, mr, rs, smooth_chk) = (nib.clone(), mr.clone(), rs.clone(), smooth_chk.clone());
            Rc::new(move || {
                let prev = state.borrow().brush_params.calligraphy;
                state.borrow_mut().brush_params.calligraphy = CalligraphyParams {
                    nib_angle_deg: nib.value(),
                    min_ratio: mr.value(),
                    resample_step_mult: rs.value(),
                    smooth_outline: smooth_chk.is_active(),
                    ..prev
                };
            })
        };
        for s in [&nib, &mr, &rs] {
            let a = apply.clone();
            s.connect_value_changed(move |_| a());
        }
        {
            let a = apply.clone();
            smooth_chk.connect_toggled(move |_| a());
        }
        body.append(&row);
    }

    let _ = BrushParams::default(); // avoid unused warning if helpers above prune
}
