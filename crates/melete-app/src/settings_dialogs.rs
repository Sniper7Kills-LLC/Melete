use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, CheckButton, ColorDialog, ColorDialogButton,
    DrawingArea, Entry, Label, Orientation, ScrolledWindow, Separator, SpinButton, Window,
};
use libadwaita as adw;
use melete_core::{NotebookId, PageTemplate, SectionId, TemplateId};
// {Notebook,Section}Store methods reached via dyn NotebookBackend.

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
    v.sort_by_key(|a| a.name.to_lowercase());
    v
}

pub fn open_notebook_settings(
    parent: &ApplicationWindow,
    state: SharedState,
    notebook_id: NotebookId,
    on_saved: Box<dyn Fn()>,
) {
    let win = modal(parent, "Notebook settings");
    win.set_default_height(620);
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

    let nb = match state
        .borrow()
        .backend
        .borrow_mut()
        .get_notebook(notebook_id)
    {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("failed to load notebook for settings: {}", e);
            return;
        }
    };
    let assigned: std::collections::HashSet<TemplateId> =
        nb.assigned_templates.iter().copied().collect();

    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let list = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .build();
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

    // ── Cloud sync — notebook-scoped because the controls all act on
    // *this* notebook's remote row. Hidden when the `remote` feature is
    // off; rows greyed out when not signed in.
    #[cfg(feature = "remote")]
    add_notebook_cloud_sync_section(&body, parent, &state, notebook_id);

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

#[cfg(feature = "remote")]
fn add_notebook_cloud_sync_section(
    body: &GtkBox,
    parent: &ApplicationWindow,
    state: &SharedState,
    notebook_id: NotebookId,
) {
    body.append(&Separator::new(Orientation::Horizontal));

    let title = Label::builder()
        .label("Cloud sync")
        .halign(gtk4::Align::Start)
        .build();
    title.add_css_class("title-3");
    body.append(&title);

    let hint = Label::builder()
        .label(
            "Push this notebook to the cloud, change who can see it, or stream every new \
             stroke as you draw.",
        )
        .wrap(true)
        .halign(gtk4::Align::Start)
        .build();
    hint.add_css_class("dim-label");
    body.append(&hint);

    let push_btn = Button::with_label("Sync to cloud now");
    {
        let parent = parent.clone();
        let state = state.clone();
        push_btn.connect_clicked(move |_| {
            crate::window::sync_with_smart_visibility(
                &parent,
                state.clone(),
                notebook_id,
                /* live_after = */ false,
            );
        });
    }
    body.append(&push_btn);

    let visibility_btn = Button::with_label("Visibility…");
    {
        let parent = parent.clone();
        visibility_btn.connect_clicked(move |_| {
            let parent_for_dialog = parent.clone();
            crate::window::ask_visibility_then(
                &parent,
                "Notebook visibility",
                "Pick who can read this notebook on the web. Private = signed-in owner only. \
                 Unlisted = anyone with the link. Public = browsable.",
                move |chosen: crate::notebook_sync::NotebookVisibility| {
                    match crate::notebook_sync::set_remote_visibility(notebook_id, chosen) {
                        Ok(()) => {
                            tracing::info!(
                                "notebook_sync: visibility set to {:?} for {:?}",
                                chosen,
                                notebook_id
                            );
                            let dialog = gtk4::AlertDialog::builder()
                                .message("Visibility updated")
                                .detail(format!("Set to {:?}.", chosen))
                                .build();
                            dialog.show(Some(&parent_for_dialog));
                        }
                        Err(e) => {
                            tracing::error!(
                                "notebook_sync: set_visibility failed for {:?}: {:#}",
                                notebook_id,
                                e
                            );
                            let dialog = gtk4::AlertDialog::builder()
                                .message("Visibility change failed")
                                .detail(format!("{:#}", e))
                                .build();
                            dialog.show(Some(&parent_for_dialog));
                        }
                    }
                },
            );
        });
    }
    body.append(&visibility_btn);

    let live_check = CheckButton::with_label("Live sync (push every stroke)");
    live_check.set_active(crate::notebook_sync::is_enabled(notebook_id));
    {
        let parent = parent.clone();
        let state = state.clone();
        live_check.connect_toggled(move |btn| {
            if btn.is_active() {
                crate::window::sync_with_smart_visibility(
                    &parent,
                    state.clone(),
                    notebook_id,
                    /* live_after = */ true,
                );
            } else {
                tracing::info!("notebook_sync: live sync OFF for {:?}", notebook_id);
                crate::notebook_sync::disable(notebook_id);
            }
        });
    }
    body.append(&live_check);

    // Sign-in-required gating: the three sync controls are useless
    // without a logged-in session. Re-evaluate while the modal is open
    // so a sign-in completed in another window flips them on.
    let signed = crate::sign_in_modal::is_signed_in();
    push_btn.set_sensitive(signed);
    visibility_btn.set_sensitive(signed);
    live_check.set_sensitive(signed);
    {
        let push_btn = push_btn.clone();
        let visibility_btn = visibility_btn.clone();
        let live_check = live_check.clone();
        gtk4::glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let on = crate::sign_in_modal::is_signed_in();
            push_btn.set_sensitive(on);
            visibility_btn.set_sensitive(on);
            live_check.set_sensitive(on);
            let actual = crate::notebook_sync::is_enabled(notebook_id);
            if live_check.is_active() != actual {
                live_check.set_active(actual);
            }
            gtk4::glib::ControlFlow::Continue
        });
    }
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

    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let list = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .build();
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

pub fn open_app_settings(parent: &ApplicationWindow, state: SharedState, on_saved: Box<dyn Fn()>) {
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
            new_cfg.stylus_top_action = inputs.stylus_top_action;
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
    let drawing_group = adw::PreferencesGroup::builder().title("Drawing").build();
    let presets_row = adw::ActionRow::builder()
        .title("Pen presets")
        .subtitle("Saved width / opacity / color combos for the toolbar")
        .activatable(true)
        .build();
    let presets_arrow = gtk4::Image::from_icon_name("go-next-symbolic");
    presets_arrow.add_css_class("dim-label");
    presets_row.add_suffix(&presets_arrow);
    drawing_group.add(&presets_row);
    {
        let parent = parent.clone();
        let state = state.clone();
        presets_row.connect_activated(move |_| {
            open_pen_presets_settings(&parent, state.clone());
        });
    }

    // ── Stylus ───────────────────────────────────────────────────────
    let stylus_group = adw::PreferencesGroup::builder()
        .title("Stylus")
        .description("Stylus barrel-button bindings.")
        .build();
    let top_action_labels = ["Cycle drawing tools", "Cycle color slots"];
    let top_action_model = gtk4::StringList::new(&top_action_labels);
    let active_top_idx = match cfg.stylus_top_action {
        crate::config::StylusTopAction::ToolCycle => 0,
        crate::config::StylusTopAction::ColorCycle => 1,
    };
    let top_action_row = adw::ComboRow::builder()
        .title("Top barrel button")
        .subtitle("Action when the upper stylus button is clicked.")
        .model(&top_action_model)
        .selected(active_top_idx)
        .build();
    stylus_group.add(&top_action_row);

    // ── Developer ────────────────────────────────────────────────────
    let dev_group = adw::PreferencesGroup::builder().title("Developer").build();
    let dev_row = adw::SwitchRow::builder()
        .title("Developer mode")
        .subtitle(
            "Enables the per-tool Tool Settings button in the menu and a floating Tool Options \
             panel that follows the currently-selected tool.",
        )
        .active(cfg.developer_mode)
        .build();
    dev_group.add(&dev_row);

    // App-only groups. Account-specific settings (Account, Cloud sync)
    // moved to `open_account_settings`.
    page.add(&display_group);
    page.add(&drawing_group);
    page.add(&stylus_group);
    page.add(&empty_group);
    page.add(&dev_group);

    prefs.add(&page);

    // Wire change signals to auto-save (adw convention — no Save/Cancel).
    let snapshot_inputs = {
        let path_state = path_state.clone();
        let text_row = text_row.clone();
        let dev_row = dev_row.clone();
        let top_action_row = top_action_row.clone();
        move || PersistInputs {
            image_path: path_state.borrow().clone(),
            text: text_row.text().to_string(),
            developer_mode: dev_row.is_active(),
            stylus_top_action: match top_action_row.selected() {
                1 => crate::config::StylusTopAction::ColorCycle,
                _ => crate::config::StylusTopAction::ToolCycle,
            },
        }
    };

    {
        let parent = parent.clone();
        let path_state = path_state.clone();
        let image_row = image_row.clone();
        let persist = persist.clone();
        let snapshot_inputs = snapshot_inputs.clone();
        pick_btn.connect_clicked(move |_| {
            let dialog = FileDialog::builder()
                .title("Pick placeholder image")
                .build();
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
    {
        let persist = persist.clone();
        let snapshot_inputs = snapshot_inputs.clone();
        top_action_row.connect_selected_notify(move |_| {
            persist(&snapshot_inputs());
        });
    }
    prefs.present();
}

struct PersistInputs {
    image_path: Option<std::path::PathBuf>,
    text: String,
    developer_mode: bool,
    stylus_top_action: crate::config::StylusTopAction,
}

/// Account settings window — separate from app preferences so account
/// state (sign-in, sync workers, autosync) lives next to identity, not
/// alongside font / stylus / empty-state preferences.
#[cfg(feature = "remote")]
pub fn open_account_settings(parent: &ApplicationWindow, state: SharedState) {
    use adw::prelude::*;
    let _ = state; // reserved for future plan/usage hooks

    let cfg = crate::config::load();
    let prefs = adw::PreferencesWindow::builder()
        .title("Account settings")
        .transient_for(parent)
        .modal(true)
        .default_width(560)
        .default_height(560)
        .build();

    let page = adw::PreferencesPage::new();

    // ── Account ─────────────────────────────────────────────────────
    let account_group = adw::PreferencesGroup::builder()
        .title("Account")
        .description(
            "Sign in to publish your templates / brushes to the public catalog \
             and fork others into your library.",
        )
        .build();
    crate::account_settings::populate_account_group(parent, &account_group);

    // ── Cloud sync ──────────────────────────────────────────────────
    let sync_group = adw::PreferencesGroup::builder()
        .title("Cloud sync")
        .description(
            "Notebook sync to AWS. Worker count controls how many \
             stroke uploads run in parallel — higher = faster eraser \
             fan-out, more concurrent connections.",
        )
        .build();

    let workers_row = adw::SpinRow::with_range(1.0, 16.0, 1.0);
    workers_row.set_title("Sync worker threads");
    workers_row.set_subtitle("Default 4. Takes effect after relaunch.");
    workers_row.set_value(cfg.sync_worker_count as f64);
    sync_group.add(&workers_row);

    let autosync_row = adw::SwitchRow::builder()
        .title("Auto-enable Live Sync on notebook open")
        .subtitle("When signed in, every notebook opens with live sync already on.")
        .active(cfg.autosync_default)
        .build();
    sync_group.add(&autosync_row);

    // ── Subscription ────────────────────────────────────────────────
    let subscription_group = adw::PreferencesGroup::builder()
        .title("Subscription")
        .description("Tier + caps for cloud sync. Manage billing in Stripe.")
        .build();
    crate::subscription_settings::populate_subscription_group(parent, &subscription_group);

    page.add(&account_group);
    page.add(&subscription_group);
    page.add(&sync_group);
    prefs.add(&page);

    let persist_sync = {
        let workers_row = workers_row.clone();
        let autosync_row = autosync_row.clone();
        move || {
            let mut new_cfg = crate::config::load();
            new_cfg.sync_worker_count = (workers_row.value() as usize).clamp(1, 16);
            new_cfg.autosync_default = autosync_row.is_active();
            if let Err(e) = crate::config::save(&new_cfg) {
                tracing::error!("save config: {e}");
            }
        }
    };
    {
        let persist = persist_sync.clone();
        workers_row.connect_changed(move |_| persist());
    }
    {
        let persist = persist_sync.clone();
        autosync_row.connect_active_notify(move |_| persist());
    }

    prefs.present();
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
pub fn open_pen_presets_settings(parent: &ApplicationWindow, state: SharedState) {
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
