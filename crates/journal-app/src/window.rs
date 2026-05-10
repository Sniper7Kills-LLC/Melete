use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box as GtkBox, Button, DrawingArea, Grid, HeaderBar, Label,
    MenuButton, Orientation, Overlay, Popover, Separator, Stack, StackTransitionType,
};
use journal_core::{NotebookId, NotebookTemplate, PageTemplate};
// NotebookStore methods reached via dyn JournalBackend.

use crate::canvas_widget;
use crate::input;
use crate::state::SharedState;
use crate::toolbar;
use crate::views::{home, notebook as notebook_view, planner_nav};

const HOME_NAME: &str = "home";
const NOTEBOOK_NAME: &str = "notebook";
const TEMPLATE_EDITOR_NAME: &str = "template_editor";
pub const NOTEBOOK_TEMPLATE_EDITOR_NAME: &str = "notebook_template_editor";
const TOOL_EDITOR_NAME: &str = "tool_editor";

pub struct AppWindow {
    pub root: GtkBox,
    pub canvas: DrawingArea,
    stack: Stack,
    home_container: GtkBox,
    notebook_container: GtkBox,
    template_editor_container: GtkBox,
    notebook_template_editor_container: GtkBox,
    tool_editor_container: GtkBox,
    canvas_overlay: Overlay,
    back_btn: Button,
    sidebar_toggle_btn: Button,
    notebook_settings_btn: Button,
    title_label: Label,
    state: SharedState,
    parent: ApplicationWindow,
    current_notebook: Rc<RefCell<Option<NotebookId>>>,
    current_sidebar: Rc<RefCell<Option<GtkBox>>>,
    previous_view: Rc<RefCell<Option<NotebookId>>>,
}

pub type SharedWindow = Rc<RefCell<AppWindow>>;

pub fn build(parent: &ApplicationWindow, state: SharedState) -> SharedWindow {
    let header = HeaderBar::new();
    let title_label = Label::new(Some("Journal"));
    title_label.add_css_class("wordmark");
    header.set_title_widget(Some(&title_label));

    let back_btn = Button::from_icon_name("go-previous-symbolic");
    back_btn.set_tooltip_text(Some("Back to notebooks"));
    back_btn.set_visible(false);
    header.pack_start(&back_btn);

    let sidebar_toggle_btn = Button::from_icon_name("sidebar-show-symbolic");
    sidebar_toggle_btn.set_tooltip_text(Some("Toggle sidebar"));
    sidebar_toggle_btn.set_visible(false);
    header.pack_start(&sidebar_toggle_btn);

    let notebook_settings_btn = Button::from_icon_name("document-properties-symbolic");
    notebook_settings_btn.set_tooltip_text(Some("Notebook settings"));
    notebook_settings_btn.set_visible(false);
    header.pack_end(&notebook_settings_btn);

    let app_settings_btn = Button::from_icon_name("preferences-system-symbolic");
    app_settings_btn.set_tooltip_text(Some("App settings"));
    header.pack_end(&app_settings_btn);
    {
        let parent = parent.clone();
        let state = state.clone();
        app_settings_btn.connect_clicked(move |_| {
            crate::settings_dialogs::open_app_settings(&parent, state.clone(), Box::new(|| {}));
        });
    }

    // Library / public catalog — discoverable in the header so the
    // "browse + fork community templates" path doesn't hide in a
    // hamburger menu.
    #[cfg(feature = "remote")]
    {
        let library_btn = Button::from_icon_name("folder-download-symbolic");
        library_btn.set_tooltip_text(Some("Library — browse public templates"));
        header.pack_end(&library_btn);
        let parent_for_lib = parent.clone();
        let state_for_lib = state.clone();
        library_btn.connect_clicked(move |_| {
            crate::remote_browser::open_browser(&parent_for_lib, state_for_lib.clone());
        });
    }

    // Account chip — shows the signed-in email (or "Sign in" prompt
    // when anonymous). Anonymous click opens the sign-in modal; signed-in
    // click opens an account popover (email / "Account settings…" /
    // "Sign out"). Opening App Settings was confusing — clicking an
    // account control should land on account actions, not the global
    // preferences window.
    #[cfg(feature = "remote")]
    {
        let account_btn = Button::new();
        account_btn.add_css_class("flat");
        refresh_account_chip(&account_btn);
        header.pack_end(&account_btn);
        let parent_for_acct = parent.clone();
        let state_for_acct = state.clone();
        let account_btn_for_click = account_btn.clone();
        account_btn.connect_clicked(move |btn| {
            if !crate::sign_in_modal::is_signed_in() {
                let chip = account_btn_for_click.clone();
                crate::sign_in_modal::open(
                    &parent_for_acct,
                    Box::new(move |_signed_in| {
                        refresh_account_chip(&chip);
                    }),
                );
                return;
            }
            open_account_popover(
                btn,
                &parent_for_acct,
                state_for_acct.clone(),
                account_btn_for_click.clone(),
            );
        });
    }

    // Create current_notebook early so build_menu_button can share it.
    let current_notebook: Rc<RefCell<Option<NotebookId>>> = Rc::new(RefCell::new(None));

    // Menu entries are built before `win` exists, so they forward through
    // these closure cells populated after `win` is built. Each closure
    // takes an optional seed (None = blank slate, Some(x) = focused on x).
    let tools_open: Rc<RefCell<Option<Rc<dyn Fn(Option<journal_core::Brush>)>>>> =
        Rc::new(RefCell::new(None));
    let templates_open: Rc<RefCell<Option<Rc<dyn Fn(Option<PageTemplate>)>>>> =
        Rc::new(RefCell::new(None));

    let menu_btn = build_menu_button(
        parent,
        state.clone(),
        current_notebook.clone(),
        tools_open.clone(),
        templates_open.clone(),
    );
    header.pack_end(&menu_btn);

    let cheatsheet_btn = build_cheatsheet_button();
    header.pack_end(&cheatsheet_btn);

    let canvas = canvas_widget::build_canvas(state.clone());
    crate::fetcher::install_poller(state.clone(), canvas.clone());
    let bar = toolbar::build_toolbar(state.clone(), tools_open.clone());
    let canvas_overlay = Overlay::new();

    // Vello path: GLArea is the canvas surface — Vello renders bg, widgets,
    // strokes, and overlays. DrawingArea isn't added at all so the GTK
    // theme can't paint a default opaque background over the GL content.
    // Cairo path (vello off): the existing DrawingArea handles everything.
    #[cfg(feature = "vello")]
    {
        if let Some(gl_area) = crate::vello_glarea::build(state.clone()) {
            input::attach_stylus(&gl_area, state.clone());
            input::attach_mouse(&gl_area, state.clone());
            input::attach_hover(&gl_area, state.clone());
            input::attach_pan_zoom(&gl_area, state.clone());
            canvas_overlay.set_child(Some(&gl_area));
        } else {
            input::attach_stylus(&canvas, state.clone());
            input::attach_mouse(&canvas, state.clone());
            input::attach_hover(&canvas, state.clone());
            input::attach_pan_zoom(&canvas, state.clone());
            canvas_overlay.set_child(Some(&canvas));
        }
    }
    #[cfg(not(feature = "vello"))]
    {
        input::attach_stylus(&canvas, state.clone());
        input::attach_mouse(&canvas, state.clone());
        input::attach_hover(&canvas, state.clone());
        input::attach_pan_zoom(&canvas, state.clone());
        canvas_overlay.set_child(Some(&canvas));
    }

    // Right-side dock slot for the Tool Options panel. Sits as a
    // right-aligned overlay child; visible only when the user toggles
    // "Dock to right side" in the panel header.
    let tool_dock_slot = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .width_request(360)
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::Fill)
        .vexpand(true)
        .build();
    tool_dock_slot.add_css_class("background");
    tool_dock_slot.set_visible(false);
    canvas_overlay.add_overlay(&tool_dock_slot);

    canvas_overlay.add_overlay(&bar);

    // Zoom indicator + fit-page button — bottom-right corner of the canvas.
    let zoom_corner = build_zoom_corner(state.clone(), canvas.clone());
    canvas_overlay.add_overlay(&zoom_corner);

    let home_container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();
    let notebook_container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();
    let template_editor_container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();
    let notebook_template_editor_container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();
    let tool_editor_container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let stack = Stack::new();
    stack.set_transition_type(StackTransitionType::SlideLeftRight);
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.add_named(&home_container, Some(HOME_NAME));
    stack.add_named(&notebook_container, Some(NOTEBOOK_NAME));
    stack.add_named(&template_editor_container, Some(TEMPLATE_EDITOR_NAME));
    stack.add_named(
        &notebook_template_editor_container,
        Some(NOTEBOOK_TEMPLATE_EDITOR_NAME),
    );
    stack.add_named(&tool_editor_container, Some(TOOL_EDITOR_NAME));

    parent.set_titlebar(Some(&header));

    let root = GtkBox::builder().orientation(Orientation::Vertical).build();
    root.append(&stack);

    let win = Rc::new(RefCell::new(AppWindow {
        root,
        canvas: canvas.clone(),
        stack,
        home_container,
        notebook_container,
        template_editor_container,
        notebook_template_editor_container,
        tool_editor_container,
        canvas_overlay,
        back_btn: back_btn.clone(),
        sidebar_toggle_btn: sidebar_toggle_btn.clone(),
        notebook_settings_btn: notebook_settings_btn.clone(),
        title_label,
        state: state.clone(),
        parent: parent.clone(),
        current_notebook,
        current_sidebar: Rc::new(RefCell::new(None)),
        previous_view: Rc::new(RefCell::new(None)),
    }));

    {
        let win = win.clone();
        back_btn.connect_clicked(move |_| show_home(&win));
    }

    // Wire menu closures now that `win` exists.
    {
        let win = win.clone();
        *tools_open.borrow_mut() = Some(Rc::new(move |seed: Option<journal_core::Brush>| {
            show_tool_editor(&win, seed);
        }));
    }
    {
        let win = win.clone();
        *templates_open.borrow_mut() = Some(Rc::new(move |seed: Option<PageTemplate>| {
            show_template_editor(&win, seed);
        }));
    }

    // Register the catalog browser's editor-openers so its "Edit"
    // (fork-then-open) action can switch the GTK stack to the
    // appropriate full-screen editor.
    #[cfg(feature = "remote")]
    {
        let win_for_page = win.clone();
        let win_for_nb = win.clone();
        let win_for_brush = win.clone();
        crate::remote_browser::set_editor_openers(crate::remote_browser::EditorOpeners {
            page: Rc::new(move |t: PageTemplate| show_template_editor(&win_for_page, Some(t))),
            notebook: Rc::new(move |t: NotebookTemplate| {
                show_notebook_template_editor(&win_for_nb, Some(t))
            }),
            brush: Rc::new(move |b: journal_core::Brush| show_tool_editor(&win_for_brush, Some(b))),
        });
    }

    {
        let win = win.clone();
        sidebar_toggle_btn.connect_clicked(move |_| {
            let sidebar = win.borrow().current_sidebar.borrow().clone();
            if let Some(sb) = sidebar {
                sb.set_visible(!sb.is_visible());
            }
        });
    }

    {
        let win = win.clone();
        notebook_settings_btn.connect_clicked(move |_| {
            let (parent, state, nb_id) = {
                let w = win.borrow();
                let nb = *w.current_notebook.borrow();
                (w.parent.clone(), w.state.clone(), nb)
            };
            if let Some(nid) = nb_id {
                crate::settings_dialogs::open_notebook_settings(
                    &parent,
                    state,
                    nid,
                    Box::new(|| {}),
                );
            }
        });
    }

    build_home_into(&win);
    show_home(&win);

    // Developer-mode Tool Options panel — either a floating window or
    // docked into the right side of the canvas. Only built when the
    // user has opted in via app settings or `JOURNAL_DEV=1`. Shown
    // only when the notebook canvas view is active; on Home /
    // template-editor views it's hidden so the user isn't staring at
    // tool tuning while looking at notebook cards.
    let tool_panel: Rc<RefCell<Option<Rc<crate::tool_options_popup::ToolOptionsPanel>>>> =
        Rc::new(RefCell::new(None));
    if crate::config::developer_mode_enabled(&crate::config::load()) {
        let panel = crate::tool_options_popup::build_tool_options_panel(
            parent,
            state.clone(),
            tool_dock_slot.clone(),
            tools_open.clone(),
        );
        *tool_panel.borrow_mut() = Some(panel);
    }
    {
        let stack = win.borrow().stack.clone();
        let tool_panel_w = tool_panel.clone();
        let update_visibility = move || {
            let on_notebook = stack
                .visible_child_name()
                .map(|n| n.as_str() == NOTEBOOK_NAME)
                .unwrap_or(false);
            if let Some(panel) = tool_panel_w.borrow().as_ref() {
                if on_notebook {
                    panel.show();
                } else {
                    panel.hide();
                }
            }
        };
        update_visibility();
        let stack = win.borrow().stack.clone();
        let update_clone = update_visibility.clone();
        stack.connect_visible_child_notify(move |_| update_clone());
    }

    win
}

/// Build and show a popover anchored to the account chip with the
/// signed-in user's email and the two account actions: open Preferences
/// to the Account group, or sign out. Built fresh each click so the
/// email label stays in sync after sign-out + sign-in.
#[cfg(feature = "remote")]
fn open_account_popover(
    anchor: &Button,
    parent: &ApplicationWindow,
    state: SharedState,
    chip: Button,
) {
    let popover = Popover::new();
    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let email_lbl = Label::builder()
        .label(
            crate::sign_in_modal::current_email()
                .map(|e| format!("Signed in as {}", e))
                .unwrap_or_else(|| "Signed in".to_string()),
        )
        .halign(Align::Start)
        .build();
    email_lbl.add_css_class("dim-label");
    vbox.append(&email_lbl);
    vbox.append(&Separator::new(Orientation::Horizontal));

    let settings_btn = Button::with_label("Account settings…");
    settings_btn.add_css_class("flat");
    {
        let popover_clone = popover.clone();
        let parent = parent.clone();
        settings_btn.connect_clicked(move |_| {
            popover_clone.popdown();
            crate::settings_dialogs::open_app_settings(&parent, state.clone(), Box::new(|| {}));
        });
    }
    vbox.append(&settings_btn);

    let signout_btn = Button::with_label("Sign out");
    signout_btn.add_css_class("flat");
    signout_btn.add_css_class("destructive-action");
    {
        let popover_clone = popover.clone();
        let chip = chip.clone();
        signout_btn.connect_clicked(move |_| {
            popover_clone.popdown();
            use journal_storage::remote_template_store::store::RemoteTemplateStore;
            match RemoteTemplateStore::connect().and_then(|mut s| s.sign_out()) {
                Ok(()) => refresh_account_chip(&chip),
                Err(e) => tracing::warn!("sign_out: {e}"),
            }
        });
    }
    vbox.append(&signout_btn);

    popover.set_child(Some(&vbox));
    popover.set_parent(anchor);
    // Drop the parent link on close so the popover widget is released
    // (otherwise GTK holds a reference until the chip is destroyed).
    popover.connect_closed(|p| p.unparent());
    popover.popup();
}

/// Render the account chip in the header to reflect current sign-in
/// state. Called on app start and after each sign-in / sign-out.
#[cfg(feature = "remote")]
fn refresh_account_chip(btn: &Button) {
    if let Some(email) = crate::sign_in_modal::current_email() {
        // Show the local-part of the email; the @domain rarely changes
        // and chews header width on a Framework 12 screen.
        let label = email.split('@').next().unwrap_or(email.as_str()).to_string();
        btn.set_label(&format!("● {}", label));
        btn.set_tooltip_text(Some(&format!("Signed in as {} — click to manage", email)));
    } else {
        btn.set_label("Sign in");
        btn.set_tooltip_text(Some("Sign in to publish + browse public catalog"));
    }
}

fn build_menu_button(
    parent: &ApplicationWindow,
    state: SharedState,
    current_notebook: Rc<RefCell<Option<NotebookId>>>,
    tools_open: Rc<RefCell<Option<Rc<dyn Fn(Option<journal_core::Brush>)>>>>,
    templates_open: Rc<RefCell<Option<Rc<dyn Fn(Option<PageTemplate>)>>>>,
) -> MenuButton {
    let popover = Popover::new();
    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    // ── File (notebook-scoped exports) ───────────────────────────────
    // Hidden entirely when no notebook is open so the home screen menu
    // isn't cluttered with greyed-out exporters.
    let export_btn = Button::with_label("Export page as PDF…");
    {
        let state = state.clone();
        let parent = parent.clone();
        let popover_clone = popover.clone();
        export_btn.connect_clicked(move |_| {
            popover_clone.popdown();
            do_pdf_export(&parent, state.clone());
        });
    }
    vbox.append(&export_btn);

    let export_nb_btn = Button::with_label("Export notebook as PDF…");
    {
        let state = state.clone();
        let parent = parent.clone();
        let popover_clone = popover.clone();
        let current_notebook = current_notebook.clone();
        export_nb_btn.connect_clicked(move |_| {
            popover_clone.popdown();
            let Some(nb_id) = *current_notebook.borrow() else {
                return;
            };
            do_notebook_pdf_export(&parent, state.clone(), nb_id);
        });
    }
    vbox.append(&export_nb_btn);

    let file_separator = Separator::new(Orientation::Horizontal);
    vbox.append(&file_separator);

    // Toggle visibility of the File group when notebook-open state changes.
    {
        let current_notebook = current_notebook.clone();
        let export_btn = export_btn.clone();
        let export_nb_btn = export_nb_btn.clone();
        let sep = file_separator.clone();
        gtk4::glib::timeout_add_local(
            std::time::Duration::from_millis(250),
            move || {
                let has_nb = current_notebook.borrow().is_some();
                export_btn.set_visible(has_nb);
                export_nb_btn.set_visible(has_nb);
                sep.set_visible(has_nb);
                gtk4::glib::ControlFlow::Continue
            },
        );
    }

    // ── Library (manage local artifacts) ─────────────────────────────
    let templates_btn = Button::with_label("Manage templates…");
    {
        let parent = parent.clone();
        let state = state.clone();
        let popover_clone = popover.clone();
        let templates_open = templates_open.clone();
        templates_btn.connect_clicked(move |_| {
            popover_clone.popdown();
            let opener: crate::template_manager::OpenEditorFn = {
                let templates_open = templates_open.clone();
                Rc::new(move |seed: Option<PageTemplate>| {
                    if let Some(f) = templates_open.borrow().as_ref().cloned() {
                        f(seed);
                    }
                })
            };
            crate::template_manager::open(&parent, state.clone(), opener);
        });
    }
    vbox.append(&templates_btn);

    let tool_editor_btn = Button::with_label("Manage brushes/tools…");
    {
        let popover_clone = popover.clone();
        let tools_open = tools_open.clone();
        tool_editor_btn.connect_clicked(move |_| {
            popover_clone.popdown();
            if let Some(f) = tools_open.borrow().as_ref().cloned() {
                f(None);
            }
        });
    }
    vbox.append(&tool_editor_btn);

    vbox.append(&Separator::new(Orientation::Horizontal));

    // ── App ──────────────────────────────────────────────────────────
    let prefs_btn = Button::with_label("Preferences…");
    {
        let parent = parent.clone();
        let state = state.clone();
        let popover_clone = popover.clone();
        prefs_btn.connect_clicked(move |_| {
            popover_clone.popdown();
            crate::settings_dialogs::open_app_settings(&parent, state.clone(), Box::new(|| {}));
        });
    }
    vbox.append(&prefs_btn);

    popover.set_child(Some(&vbox));

    MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .popover(&popover)
        .tooltip_text("Menu")
        .build()
}

fn do_pdf_export(parent: &ApplicationWindow, state: SharedState) {
    let dialog = gtk4::FileDialog::builder()
        .title("Export page as PDF")
        .modal(true)
        .initial_name("page.pdf")
        .build();

    let filter = gtk4::FileFilter::new();
    filter.set_name(Some("PDF files"));
    filter.add_pattern("*.pdf");
    filter.add_mime_type("application/pdf");
    let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));

    let parent_clone = parent.clone();
    dialog.save(Some(parent), gtk4::gio::Cancellable::NONE, move |result| {
        let file = match result {
            Ok(f) => f,
            Err(_) => return,
        };
        let path = match file.path() {
            Some(p) => p,
            None => {
                tracing::warn!("PDF export: no path from file dialog");
                return;
            }
        };
        let path = if path.extension().map(|e| e != "pdf").unwrap_or(true) {
            path.with_extension("pdf")
        } else {
            path
        };
        if let Err(e) = crate::pdf_export::export_page_to_pdf(&state, &path) {
            tracing::error!("PDF export failed: {:#}", e);
            let dialog = gtk4::AlertDialog::builder()
                .message("Export failed")
                .detail(format!("{:#}", e).as_str())
                .build();
            dialog.show(Some(&parent_clone));
        } else {
            tracing::info!("PDF exported to {:?}", path);
        }
    });
}

fn do_notebook_pdf_export(
    parent: &ApplicationWindow,
    state: SharedState,
    notebook_id: journal_core::NotebookId,
) {
    let dialog = gtk4::FileDialog::builder()
        .title("Export notebook as PDF")
        .modal(true)
        .initial_name("notebook.pdf")
        .build();

    let filter = gtk4::FileFilter::new();
    filter.set_name(Some("PDF files"));
    filter.add_pattern("*.pdf");
    filter.add_mime_type("application/pdf");
    let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));

    let parent_clone = parent.clone();
    dialog.save(Some(parent), gtk4::gio::Cancellable::NONE, move |result| {
        let file = match result {
            Ok(f) => f,
            Err(_) => return,
        };
        let path = match file.path() {
            Some(p) => p,
            None => {
                tracing::warn!("notebook PDF export: no path from file dialog");
                return;
            }
        };
        let path = if path.extension().map(|e| e != "pdf").unwrap_or(true) {
            path.with_extension("pdf")
        } else {
            path
        };
        if let Err(e) = crate::pdf_export::export_notebook_to_pdf(&state, notebook_id, &path) {
            tracing::error!("notebook PDF export failed: {:#}", e);
            let dialog = gtk4::AlertDialog::builder()
                .message("Export failed")
                .detail(format!("{:#}", e).as_str())
                .build();
            dialog.show(Some(&parent_clone));
        } else {
            tracing::info!("notebook PDF exported to {:?}", path);
        }
    });
}

pub fn show_home(win: &SharedWindow) {
    let w = win.borrow();
    w.stack.set_visible_child_name(HOME_NAME);
    w.back_btn.set_visible(false);
    w.sidebar_toggle_btn.set_visible(false);
    w.notebook_settings_btn.set_visible(false);
    *w.current_notebook.borrow_mut() = None;
    *w.current_sidebar.borrow_mut() = None;
    w.title_label.set_text("Journal");
}

pub fn show_notebook(win: &SharedWindow, notebook_id: NotebookId) {
    {
        let mut cfg = crate::config::load();
        cfg.recent_notebook_ids.retain(|id| *id != notebook_id.0);
        cfg.recent_notebook_ids.insert(0, notebook_id.0);
        cfg.recent_notebook_ids.truncate(5);
        if let Err(e) = crate::config::save(&cfg) {
            tracing::warn!("failed to save recent notebooks: {}", e);
        }
    }

    let overlay = win.borrow().canvas_overlay.clone();
    if overlay.parent().is_some() {
        overlay.unparent();
    }

    let container = win.borrow().notebook_container.clone();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let parent = win.borrow().parent.clone();
    let state = win.borrow().state.clone();
    let canvas = win.borrow().canvas.clone();

    {
        let mut s = state.borrow_mut();
        s.strokes.clear();
        s.current_stroke = None;
        s.current_page_id = None;
        s.current_page_date = None;
        s.current_template = None;
        s.background = crate::state::default_background();
        s.page_rect = crate::state::default_page_rect();
        // Drop any callback installed by the previous notebook's planner
        // nav strip; the new notebook will install its own.
        s.planner_nav_sync_date = None;
    }
    win.borrow().canvas.queue_draw();

    let planner_template = planner_nav::resolve_planner_template(&state, notebook_id);
    if let Some(ref template) = planner_template {
        let today = chrono::Local::now().date_naive();
        let _ = planner_nav::goto_date(&state, &canvas, notebook_id, template, today);
    }

    let view =
        notebook_view::build_notebook_view(&parent, state.clone(), notebook_id, canvas, overlay);
    container.append(&view.root);

    let title = match state
        .borrow()
        .backend
        .borrow_mut()
        .get_notebook(notebook_id)
    {
        Ok(nb) => nb.name,
        Err(_) => "Notebook".to_string(),
    };
    win.borrow().title_label.set_text(&title);
    win.borrow().back_btn.set_visible(true);
    win.borrow().sidebar_toggle_btn.set_visible(true);
    win.borrow().notebook_settings_btn.set_visible(true);
    *win.borrow().current_notebook.borrow_mut() = Some(notebook_id);
    *win.borrow().current_sidebar.borrow_mut() = Some(view.sidebar_root);
    win.borrow().stack.set_visible_child_name(NOTEBOOK_NAME);

    // Pull any cloud-side strokes the local DB doesn't have yet, on a
    // worker thread so the UI stays responsive while AppSync paginates
    // through up to a few thousand stroke rows. The poller below
    // applies the result back on the main thread.
    #[cfg(feature = "remote")]
    spawn_pull_with_progress(&parent, win.clone(), state.clone(), notebook_id);

    // Auto-enable Live Sync per `AppConfig::autosync_default`. On
    // first-time open of a notebook (not yet in the cloud), kick a
    // silent full sync so existing strokes mirror up. Subsequent
    // opens just flip the live flag.
    //
    // The visibility lookup is a single sync HTTP GET (~100 ms) and
    // runs on the main thread. The actual push is non-blocking via
    // `spawn_sync`. Acceptable freeze for a one-time per-notebook
    // open.
    #[cfg(feature = "remote")]
    {
        let cfg = crate::config::load();
        if cfg.autosync_default && crate::sign_in_modal::is_signed_in() {
            crate::notebook_sync::mark_enabled_external(notebook_id);
            tracing::info!("notebook_sync: autosync_default ON for {:?}", notebook_id);
            use crate::notebook_sync::NotebookVisibility;
            match crate::notebook_sync::fetch_remote_visibility(notebook_id) {
                Ok(Some(v)) => {
                    tracing::debug!(
                        "notebook_sync: autosync — already pushed ({:?}), skipping initial sync",
                        v
                    );
                }
                Ok(None) => {
                    match crate::notebook_sync::spawn_sync(
                        &state,
                        notebook_id,
                        NotebookVisibility::Private,
                        false,
                    ) {
                        Ok(_) => tracing::info!(
                            "notebook_sync: autosync initial sync started for {:?}",
                            notebook_id
                        ),
                        Err(e) => tracing::warn!(
                            "notebook_sync: autosync initial sync failed for {:?}: {:#}",
                            notebook_id,
                            e
                        ),
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "notebook_sync: autosync visibility lookup failed for {:?}: {:#}",
                        notebook_id,
                        e
                    );
                }
            }
        }
    }
}

fn build_home_into(win: &SharedWindow) {
    let container = win.borrow().home_container.clone();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let parent = win.borrow().parent.clone();
    let state = win.borrow().state.clone();
    let backend = state.borrow().backend.clone();
    let win_for_open = win.clone();
    let on_open: Rc<dyn Fn(NotebookId)> = Rc::new(move |id| {
        show_notebook(&win_for_open, id);
    });
    let win_for_editor = win.clone();
    let on_open_template_editor: Rc<dyn Fn(Option<PageTemplate>)> = Rc::new(move |edit| {
        show_template_editor(&win_for_editor, edit);
    });

    // Register the stack-page notebook template editor opener with the
    // template manager so the edit button routes to the full-screen editor.
    let win_for_nb_editor = win.clone();
    let on_open_nb_editor: crate::template_manager::OpenNbEditorFn =
        Rc::new(move |edit: Option<NotebookTemplate>| {
            show_notebook_template_editor(&win_for_nb_editor, edit);
        });
    crate::template_manager::set_nb_editor_opener(on_open_nb_editor);

    let home = home::build_home(&parent, state, backend, on_open, on_open_template_editor);
    container.append(&home);
}

/// Switch the stack to the full-screen template editor for `edit` (Some => edit
/// existing template, None => new template). When the editor closes (save or
/// cancel) we navigate back to the previous view (notebook canvas or home).
pub fn show_template_editor(win: &SharedWindow, edit: Option<PageTemplate>) {
    let container = win.borrow().template_editor_container.clone();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    // Remember where we came from so Back/Save can return to it.
    let return_notebook = *win.borrow().current_notebook.borrow();
    *win.borrow().previous_view.borrow_mut() = return_notebook;

    let state = win.borrow().state.clone();
    let parent = win.borrow().parent.clone();

    let win_for_done = win.clone();
    let on_done: Rc<dyn Fn()> = Rc::new(move || {
        let prev = *win_for_done.borrow().previous_view.borrow();
        match prev {
            Some(nb_id) => show_notebook(&win_for_done, nb_id),
            None => show_home(&win_for_done),
        }
    });

    let view = crate::template_creator::build_editor_view(&parent, state, edit, on_done.clone());
    container.append(&view);

    let w = win.borrow();
    w.title_label.set_text("Template Editor");
    w.back_btn.set_visible(false);
    w.sidebar_toggle_btn.set_visible(false);
    w.notebook_settings_btn.set_visible(false);
    w.stack.set_visible_child_name(TEMPLATE_EDITOR_NAME);
}

/// Switch the stack to the full-screen Tool Editor.
/// `seed_brush` — `Some(b)` opens the editor on a specific brush;
/// `None` opens blank-slate (a default Pen composition).
pub fn show_tool_editor(win: &SharedWindow, seed_brush: Option<journal_core::Brush>) {
    let container = win.borrow().tool_editor_container.clone();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    // Remember where we came from so Done/Cancel can return.
    let return_notebook = *win.borrow().current_notebook.borrow();
    *win.borrow().previous_view.borrow_mut() = return_notebook;

    let state = win.borrow().state.clone();
    let parent = win.borrow().parent.clone();

    let win_for_done = win.clone();
    let on_done: Rc<dyn Fn()> = Rc::new(move || {
        let prev = *win_for_done.borrow().previous_view.borrow();
        match prev {
            Some(nb_id) => show_notebook(&win_for_done, nb_id),
            None => show_home(&win_for_done),
        }
    });

    let view = crate::tool_editor::build_editor_view(&parent, state, seed_brush, on_done);
    container.append(&view);

    let w = win.borrow();
    w.title_label.set_text("Tool Editor");
    w.back_btn.set_visible(false);
    w.sidebar_toggle_btn.set_visible(false);
    w.notebook_settings_btn.set_visible(false);
    w.stack.set_visible_child_name(TOOL_EDITOR_NAME);
}

/// Switch the stack to the full-screen notebook template editor.
/// `edit` — `Some(t)` edits an existing template, `None` creates a new one.
pub fn show_notebook_template_editor(win: &SharedWindow, edit: Option<NotebookTemplate>) {
    let container = win.borrow().notebook_template_editor_container.clone();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    // Remember where we came from.
    let return_notebook = *win.borrow().current_notebook.borrow();
    *win.borrow().previous_view.borrow_mut() = return_notebook;

    let state = win.borrow().state.clone();
    let parent = win.borrow().parent.clone();

    let win_for_done = win.clone();
    let on_done: Rc<dyn Fn()> = Rc::new(move || {
        let prev = *win_for_done.borrow().previous_view.borrow();
        match prev {
            Some(nb_id) => show_notebook(&win_for_done, nb_id),
            None => show_home(&win_for_done),
        }
    });

    let win_for_chip = win.clone();
    let on_open_chip: Rc<dyn Fn(PageTemplate)> = Rc::new(move |t| {
        show_template_editor(&win_for_chip, Some(t));
    });

    let view = crate::notebook_template_creator::build_editor_view(
        &parent,
        state,
        edit,
        on_done,
        Some(on_open_chip),
    );
    container.append(&view);

    let w = win.borrow();
    w.title_label.set_text("Notebook Template Editor");
    w.back_btn.set_visible(false);
    w.sidebar_toggle_btn.set_visible(false);
    w.notebook_settings_btn.set_visible(false);
    w.stack
        .set_visible_child_name(NOTEBOOK_TEMPLATE_EDITOR_NAME);
}

/// Compute the zoom value at which the page would exactly fit the viewport
/// (matches `state::fit_viewport_to_page`'s `* 0.9` margin). Returns `None`
/// when we can't compute it yet (zero-sized canvas or page).
fn natural_fit_zoom_inner(s: &crate::state::CanvasState) -> Option<f64> {
    let (sw, sh) = s.transform.screen_size();
    let pr = s.page_rect;
    if sw <= 0.0 || sh <= 0.0 || pr.width <= 0.0 || pr.height <= 0.0 {
        return None;
    }
    Some((sw / pr.width).min(sh / pr.height) * 0.9)
}

/// Build the floating zoom indicator + "fit page" button cluster that sits
/// in the bottom-right corner of the canvas overlay.
///
/// The displayed percentage is relative to the page's *natural* fit zoom
/// (i.e. the zoom that makes the page fill the viewport). 100% therefore
/// means "page fits the screen" — what users expect — instead of the raw
/// internal `transform.zoom()` value, which is in canvas-units-per-screen-px
/// and produces visually misleading numbers.
fn build_zoom_corner(state: SharedState, canvas: DrawingArea) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::End)
        .margin_end(16)
        .margin_bottom(16)
        .build();

    // Reset-grid button — re-anchors the grid spacing to the current zoom
    // so the on-screen grid pitch matches what the user sees right now.
    // Hidden unless the page actually has a grid-style background.
    let reset_grid_btn = Button::from_icon_name("view-grid-symbolic");
    reset_grid_btn.add_css_class("zoom-badge");
    reset_grid_btn.add_css_class("osd");
    reset_grid_btn.set_tooltip_text(Some(
        "Reset grid to current zoom — re-anchors the template grid \
         so it stays at this on-screen size",
    ));
    {
        let state = state.clone();
        let canvas = canvas.clone();
        reset_grid_btn.connect_clicked(move |_| {
            {
                let mut s = state.borrow_mut();
                let z = s.transform.zoom().max(1e-6);
                s.bg_scale = (1.0 / z).clamp(1e-4, 1e4);
            }
            canvas.queue_draw();
        });
    }
    row.append(&reset_grid_btn);

    // Fit-to-page button — explicit, discoverable.
    let fit_btn = Button::from_icon_name("zoom-fit-best-symbolic");
    fit_btn.add_css_class("zoom-badge");
    fit_btn.add_css_class("osd");
    fit_btn.set_tooltip_text(Some("Fit page (Ctrl+0)"));
    {
        let state = state.clone();
        let canvas = canvas.clone();
        fit_btn.connect_clicked(move |_| {
            let page_rect = state.borrow().page_rect;
            {
                let mut s = state.borrow_mut();
                crate::state::fit_viewport_to_page_pub(&mut s.transform, page_rect);
                s.bg_scale = 1.0;
            }
            canvas.queue_draw();
        });
    }
    row.append(&fit_btn);

    // Zoom-percentage label — also clickable as a redundant fit shortcut.
    let badge = Button::with_label("100%");
    badge.add_css_class("zoom-badge");
    badge.add_css_class("osd");
    badge.set_tooltip_text(Some("Zoom — click to fit page"));
    {
        let state = state.clone();
        let canvas = canvas.clone();
        badge.connect_clicked(move |_| {
            let page_rect = state.borrow().page_rect;
            {
                let mut s = state.borrow_mut();
                crate::state::fit_viewport_to_page_pub(&mut s.transform, page_rect);
                s.bg_scale = 1.0;
            }
            canvas.queue_draw();
        });
    }
    row.append(&badge);

    // Tick: update the label whenever the zoom (relative to fit-zoom)
    // changes. Cheap — int compare first, only set_label on change.
    {
        let state = state.clone();
        let badge_for_tick = badge.clone();
        let reset_btn = reset_grid_btn.clone();
        let last: std::rc::Rc<std::cell::Cell<i32>> = std::rc::Rc::new(std::cell::Cell::new(-1));
        badge.add_tick_callback(move |_, _| {
            let (pct, has_grid) = {
                let s = state.borrow();
                let pct = match natural_fit_zoom_inner(&s) {
                    Some(fit) if fit > 1e-9 => ((s.transform.zoom() / fit) * 100.0).round() as i32,
                    _ => 100,
                };
                (pct, background_is_grid(&s.background))
            };
            if last.get() != pct {
                last.set(pct);
                badge_for_tick.set_label(&format!("{}%", pct));
                // Brief amber pulse on every zoom change so the value
                // pop reads as "I just updated" — see CSS .zoom-badge.pulse.
                badge_for_tick.add_css_class("pulse");
                let badge_for_clear = badge_for_tick.clone();
                gtk4::glib::timeout_add_local_once(
                    std::time::Duration::from_millis(160),
                    move || {
                        badge_for_clear.remove_css_class("pulse");
                    },
                );
            }
            reset_btn.set_visible(has_grid);
            gtk4::glib::ControlFlow::Continue
        });
    }

    row
}

fn background_is_grid(bg: &journal_canvas::BackgroundConfig) -> bool {
    use journal_canvas::BackgroundConfig as B;
    matches!(
        bg,
        B::Grid(_) | B::Lines { .. } | B::Dots { .. } | B::Isometric { .. } | B::Hexagonal { .. }
    )
}

fn build_cheatsheet_button() -> MenuButton {
    let popover = Popover::new();
    let grid = Grid::builder().row_spacing(2).column_spacing(12).build();
    grid.add_css_class("cheatsheet-grid");

    let title = Label::builder()
        .label("Keyboard shortcuts")
        .halign(Align::Start)
        .build();
    title.add_css_class("title-4");
    grid.attach(&title, 0, 0, 2, 1);

    let entries: &[(&str, &str)] = &[
        ("B", "Pen"),
        ("H", "Highlighter"),
        ("E", "Eraser (cycle)"),
        ("V", "Selection"),
        ("Ctrl+Z", "Undo"),
        ("Ctrl+Shift+Z", "Redo"),
        ("Ctrl+C", "Copy selection"),
        ("Ctrl+V", "Paste"),
        ("Ctrl+0", "Fit page"),
        ("Ctrl++", "Zoom in"),
        ("Ctrl+-", "Zoom out"),
        ("Ctrl+S", "Save (template editor)"),
        ("Esc", "Clear selection"),
        ("Delete", "Delete selection"),
        ("F11", "Fullscreen"),
    ];
    for (i, (key, action)) in entries.iter().enumerate() {
        let row = (i + 1) as i32;
        let key_lbl = Label::builder().label(*key).halign(Align::End).build();
        key_lbl.add_css_class("kbd");
        let act_lbl = Label::builder().label(*action).halign(Align::Start).build();
        grid.attach(&key_lbl, 0, row, 1, 1);
        grid.attach(&act_lbl, 1, row, 1, 1);
    }
    popover.set_child(Some(&grid));

    MenuButton::builder()
        .icon_name("dialog-question-symbolic")
        .popover(&popover)
        .tooltip_text("Keyboard shortcuts")
        .build()
}


#[cfg(feature = "remote")]
pub(crate) fn ask_visibility_then(
    parent: &ApplicationWindow,
    title: &str,
    detail: &str,
    on_pick: impl Fn(crate::notebook_sync::NotebookVisibility) + 'static,
) {
    use crate::notebook_sync::NotebookVisibility;
    // GTK4's AlertDialog supports a button list with a default. Order
    // matters: index 0 is the "first" / default action. Putting Private
    // there matches the requirement that visibility default to PRIVATE.
    // GTK4 also supports cancel/default index hints to handle the
    // Escape key and the Enter key respectively.
    let dialog = gtk4::AlertDialog::builder()
        .message(title)
        .detail(detail)
        .buttons(["Private", "Unlisted", "Public", "Cancel"])
        .default_button(0)
        .cancel_button(3)
        .modal(true)
        .build();
    let parent = parent.clone();
    dialog.choose(
        Some(&parent),
        gtk4::gio::Cancellable::NONE,
        move |res| {
            let idx = match res {
                Ok(i) => i,
                Err(_) => return,
            };
            let visibility = match idx {
                0 => NotebookVisibility::Private,
                1 => NotebookVisibility::Unlisted,
                2 => NotebookVisibility::Public,
                _ => return,
            };
            on_pick(visibility);
        },
    );
}

/// Push the notebook to the cloud, prompting for visibility only on
/// the first push. Subsequent pushes reuse the remote row's existing
/// visibility silently. `live_after` flips the live-sync flag on after
/// the upload completes — used by the "Live sync" toggle.
#[cfg(feature = "remote")]
pub(crate) fn sync_with_smart_visibility(
    parent: &ApplicationWindow,
    state: SharedState,
    nb_id: NotebookId,
    live_after: bool,
) {
    use crate::notebook_sync::NotebookVisibility;
    let parent_owned = parent.clone();
    match crate::notebook_sync::fetch_remote_visibility(nb_id) {
        Ok(Some(existing)) => {
            // Already pushed — reuse the existing visibility silently.
            run_sync(&parent_owned, state, nb_id, existing, live_after);
        }
        Ok(None) => {
            // Never pushed — prompt for the initial visibility.
            let parent_for_prompt = parent_owned.clone();
            let parent_for_run = parent_owned.clone();
            ask_visibility_then(
                &parent_for_prompt,
                "First sync — pick visibility",
                "This notebook hasn't been pushed before. Pick who can see it on the web. You can change this later from the notebook settings.",
                move |chosen: NotebookVisibility| {
                    run_sync(&parent_for_run, state.clone(), nb_id, chosen, live_after);
                },
            );
        }
        Err(e) => {
            tracing::error!("notebook_sync: fetch_remote_visibility failed for {:?}: {:#}", nb_id, e);
            let dialog = gtk4::AlertDialog::builder()
                .message("Cloud lookup failed")
                .detail(format!("{:#}", e))
                .build();
            dialog.show(Some(&parent_owned));
        }
    }
}

/// Open a progress window and spin up a background sync. The window
/// is non-modal-but-transient — the user can't close it via Escape
/// (no decorations) and can't draw on the canvas while it's up
/// because we set `set_modal(true)`. Polls the worker's
/// `Arc<Mutex<SyncJobState>>` every 150 ms and updates the bar.
#[cfg(feature = "remote")]
fn run_sync(
    parent: &ApplicationWindow,
    state: SharedState,
    nb_id: NotebookId,
    visibility: crate::notebook_sync::NotebookVisibility,
    live_after: bool,
) {
    let job = match crate::notebook_sync::spawn_sync(&state, nb_id, visibility, live_after) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("notebook_sync: spawn_sync failed for {:?}: {:#}", nb_id, e);
            let dialog = gtk4::AlertDialog::builder()
                .message("Sync failed to start")
                .detail(format!("{:#}", e))
                .build();
            dialog.show(Some(parent));
            return;
        }
    };

    // Build the progress window. No close button — we drive the close
    // ourselves once the worker reports `finished`.
    let win = gtk4::Window::builder()
        .transient_for(parent)
        .modal(true)
        .deletable(false)
        .resizable(false)
        .default_width(420)
        .title("Syncing notebook")
        .build();
    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(20)
        .margin_end(20)
        .build();
    let phase_label = Label::builder()
        .halign(Align::Start)
        .label("Connecting…")
        .build();
    let counter_label = Label::builder()
        .halign(Align::Start)
        .label("")
        .build();
    counter_label.add_css_class("dim-label");
    let bar = gtk4::ProgressBar::new();
    bar.set_show_text(false);
    vbox.append(&phase_label);
    vbox.append(&bar);
    vbox.append(&counter_label);
    win.set_child(Some(&vbox));
    win.present();

    let win_clone = win.clone();
    let parent_for_done = parent.clone();
    let job_state_for_tick = job.state.clone();
    let state_for_purge = state.clone();

    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
        let snapshot = job_state_for_tick.lock().unwrap().clone();
        if !snapshot.phase.is_empty() {
            phase_label.set_text(&snapshot.phase);
        }
        if snapshot.total > 0 {
            let frac = (snapshot.done as f64) / (snapshot.total as f64);
            bar.set_fraction(frac.clamp(0.0, 1.0));
            counter_label.set_text(&format!("{} / {}", snapshot.done, snapshot.total));
        } else {
            bar.pulse();
            counter_label.set_text("");
        }
        if let Some(result) = snapshot.finished {
            win_clone.close();
            match result {
                Ok(report) => {
                    tracing::info!(
                        "notebook_sync: synced {:?} ({:?}, live_after={}): sections={} pages={} strokes={} purged={}",
                        nb_id,
                        visibility,
                        live_after,
                        report.sections_upserted,
                        report.pages_upserted,
                        report.strokes_upserted,
                        report.strokes_purged.len(),
                    );
                    // Don't hard-purge — the soft-deleted local rows
                    // are the tombstone that prevents
                    // apply_pulled_strokes from re-merging the same
                    // ids on a future pull. See comment in
                    // sync_notebook_now for the full rationale.
                    let _ = state_for_purge.borrow();
                    if !live_after {
                        let dialog = gtk4::AlertDialog::builder()
                            .message("Notebook synced")
                            .detail(format!(
                                "Visibility: {:?}\nSections: {}\nPages: {}\nStrokes: {}\nLocal tombstones cleared: {}",
                                visibility,
                                report.sections_upserted,
                                report.pages_upserted,
                                report.strokes_upserted,
                                report.strokes_purged.len(),
                            ))
                            .build();
                        dialog.show(Some(&parent_for_done));
                    }
                }
                Err(e) => {
                    tracing::error!("notebook_sync: sync failed for {:?}: {}", nb_id, e);
                    if live_after {
                        // Roll back the optimistic enable from spawn_sync
                        // so the toggle reflects reality.
                        crate::notebook_sync::disable(nb_id);
                    }
                    let dialog = gtk4::AlertDialog::builder()
                        .message(if live_after {
                            "Live sync failed to start"
                        } else {
                            "Sync failed"
                        })
                        .detail(e)
                        .build();
                    dialog.show(Some(&parent_for_done));
                }
            }
            return gtk4::glib::ControlFlow::Break;
        }
        gtk4::glib::ControlFlow::Continue
    });
}

/// Background pull-on-open. Pops a small modal "Checking cloud…" window
/// while the worker fetches; once the worker finishes, applies the
/// pulled strokes to local SQLite on the main thread, then refreshes
/// the current page so newly-merged strokes hit the canvas.
#[cfg(feature = "remote")]
fn spawn_pull_with_progress(
    parent: &ApplicationWindow,
    win: SharedWindow,
    state: SharedState,
    notebook_id: NotebookId,
) {
    let job = crate::notebook_sync::spawn_pull(notebook_id);

    let progress_win = gtk4::Window::builder()
        .transient_for(parent)
        .modal(true)
        .deletable(false)
        .resizable(false)
        .default_width(360)
        .title("Checking cloud")
        .build();
    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(20)
        .margin_end(20)
        .build();
    let phase = Label::builder()
        .halign(Align::Start)
        .label("Checking cloud…")
        .build();
    let bar = gtk4::ProgressBar::new();
    bar.set_show_text(false);
    vbox.append(&phase);
    vbox.append(&bar);
    progress_win.set_child(Some(&vbox));
    progress_win.present();

    let job_state = job.state.clone();
    let win_clone = win.clone();
    let pw = progress_win.clone();
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
        // Snapshot fields we need without holding the lock through any
        // potentially-blocking work.
        let (current_phase, finished) = {
            let g = job_state.lock().unwrap();
            (g.phase.clone(), g.finished.is_some())
        };
        if !current_phase.is_empty() {
            phase.set_text(&current_phase);
        }
        bar.pulse();
        if !finished {
            return gtk4::glib::ControlFlow::Continue;
        }
        // Take ownership of the result so we can drop the lock before
        // the (long-ish) backend write.
        let outcome = job_state.lock().unwrap().finished.take();
        pw.close();
        match outcome {
            Some(Ok(strokes)) => {
                if strokes.is_empty() {
                    tracing::debug!(
                        "notebook_sync: no cloud strokes for {:?}",
                        notebook_id
                    );
                    return gtk4::glib::ControlFlow::Break;
                }
                let report = crate::notebook_sync::apply_pulled_strokes(
                    &state,
                    notebook_id,
                    strokes,
                );
                tracing::info!(
                    "notebook_sync: pulled {} stroke(s) from cloud for {:?} (skipped {} duplicates)",
                    report.strokes_inserted,
                    notebook_id,
                    report.strokes_skipped_duplicate
                );
                if report.strokes_inserted > 0 {
                    if let Some(pid) = state.borrow().current_page_id {
                        crate::views::notebook::reload_current_page_strokes(&state, pid);
                    }
                    win_clone.borrow().canvas.queue_draw();
                }
            }
            Some(Err(e)) => {
                tracing::warn!(
                    "notebook_sync: pull failed for {:?}: {}",
                    notebook_id,
                    e
                );
            }
            None => {}
        }
        gtk4::glib::ControlFlow::Break
    });
}
