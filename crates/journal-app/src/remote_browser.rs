//! "Browse public templates" window. Lists public page templates,
//! notebook templates, and brushes from the Amplify catalog and lets
//! the user fork an entry into their local registry.
//!
//! Gated on the `remote` feature so non-cloud builds drop the menu
//! entry entirely.

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, DropDown, Entry, Label, ListBox, ListBoxRow,
    Notebook, Orientation, ScrolledWindow, SelectionMode, StringList, Window,
};

use journal_storage::remote_template_store::store::{
    RemoteError, RemoteTemplateOps, RemoteTemplateStore, RemoteTemplateSummary,
};

use crate::state::SharedState;

// ── Editor-opener wiring ────────────────────────────────────────────
//
// The catalog browser needs to hand a forked template / brush off to
// the appropriate full-screen editor. The openers themselves live in
// `window.rs` (they hold a `SharedWindow` to switch the GTK stack).
// To avoid threading the openers through every call site, `window.rs`
// stashes them in this thread-local once the window is built; the
// browser then reads them on demand.

pub type OpenPageTemplateEditorFn = std::rc::Rc<dyn Fn(journal_core::PageTemplate)>;
pub type OpenNotebookTemplateEditorFn = std::rc::Rc<dyn Fn(journal_core::NotebookTemplate)>;
pub type OpenBrushEditorFn = std::rc::Rc<dyn Fn(journal_core::Brush)>;

#[derive(Clone)]
pub struct EditorOpeners {
    pub page: OpenPageTemplateEditorFn,
    pub notebook: OpenNotebookTemplateEditorFn,
    pub brush: OpenBrushEditorFn,
}

thread_local! {
    static EDITOR_OPENERS: std::cell::RefCell<Option<EditorOpeners>> =
        const { std::cell::RefCell::new(None) };
}

pub fn set_editor_openers(o: EditorOpeners) {
    EDITOR_OPENERS.with(|c| *c.borrow_mut() = Some(o));
}

fn with_openers<R>(f: impl FnOnce(Option<&EditorOpeners>) -> R) -> R {
    EDITOR_OPENERS.with(|c| f(c.borrow().as_ref()))
}

pub fn open_browser(parent: &ApplicationWindow, state: SharedState) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Browse public catalog")
        .default_width(760)
        .default_height(720)
        .build();

    let outer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(0)
        .build();

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(10)
        .margin_bottom(6)
        .margin_start(14)
        .margin_end(14)
        .build();
    let title_col = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .build();
    let title = Label::builder()
        .label("Public catalog")
        .halign(gtk4::Align::Start)
        .build();
    title.add_css_class("title-2");
    let subtitle = Label::builder()
        .label("Templates and brushes shared by the community. Fork to copy into your library.")
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    subtitle.add_css_class("dim-label");
    title_col.append(&title);
    title_col.append(&subtitle);
    header.append(&title_col);

    let close_btn = Button::with_label("Close");
    close_btn.set_valign(gtk4::Align::Center);
    {
        let win = win.clone();
        close_btn.connect_clicked(move |_| win.close());
    }
    header.append(&close_btn);
    outer.append(&header);

    // Tabs: Page Templates / Notebook Templates / Brushes.
    // Each tab manages its own refresh + state so reloads only re-fetch
    // the active model.
    let notebook = Notebook::builder().vexpand(true).hexpand(true).build();
    let (page_body, page_label, _page_refresh) = build_tab(state.clone(), Kind::PageTemplate);
    let (nb_body, nb_label, _nb_refresh) = build_tab(state.clone(), Kind::NotebookTemplate);
    let (brush_body, brush_label, _brush_refresh) = build_tab(state.clone(), Kind::Brush);
    notebook.append_page(&page_body, Some(&page_label));
    notebook.append_page(&nb_body, Some(&nb_label));
    notebook.append_page(&brush_body, Some(&brush_label));
    outer.append(&notebook);

    win.set_child(Some(&outer));
    win.present();
}

#[derive(Clone, Copy)]
enum Kind {
    PageTemplate,
    NotebookTemplate,
    Brush,
}

impl Kind {
    fn tab_base_label(self) -> &'static str {
        match self {
            Kind::PageTemplate => "Page templates",
            Kind::NotebookTemplate => "Notebook templates",
            Kind::Brush => "Brushes",
        }
    }
}

/// Build a tab. Returns (body, tab_label, refresh_fn).
/// The tab label updates with the current row count; the refresh fn
/// is exposed in case the caller wants to invoke it externally
/// (currently only the in-tab Refresh button uses it).
fn build_tab(
    state: SharedState,
    kind: Kind,
) -> (GtkBox, Label, std::rc::Rc<dyn Fn()>) {
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(10)
        .margin_bottom(8)
        .margin_start(14)
        .margin_end(14)
        .build();

    // Top row — count status + Refresh.
    let toolbar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    let status = Label::builder()
        .label("Loading…")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    status.add_css_class("dim-label");
    let refresh_btn = Button::from_icon_name("view-refresh-symbolic");
    refresh_btn.set_tooltip_text(Some("Refresh"));
    refresh_btn.add_css_class("flat");
    toolbar.append(&status);
    toolbar.append(&refresh_btn);
    body.append(&toolbar);

    // Filter row — search + (page templates only) category dropdown.
    let filter_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .build();
    let search = Entry::builder()
        .placeholder_text("Search by name or description")
        .hexpand(true)
        .build();
    filter_row.append(&search);
    let category_dropdown = DropDown::from_strings(&["All categories"]);
    category_dropdown.set_visible(matches!(kind, Kind::PageTemplate));
    filter_row.append(&category_dropdown);
    body.append(&filter_row);

    // Result list.
    let scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .build();
    let list = ListBox::builder()
        .selection_mode(SelectionMode::None)
        .build();
    list.add_css_class("boxed-list");
    scroll.set_child(Some(&list));
    body.append(&scroll);

    let tab_label = Label::new(Some(kind.tab_base_label()));

    // Cached row set so search / category re-filter without re-fetching.
    let cache: std::rc::Rc<std::cell::RefCell<Vec<RemoteTemplateSummary>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let last_error: std::rc::Rc<std::cell::RefCell<Option<RemoteError>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));

    let apply_filters: std::rc::Rc<dyn Fn()> = {
        let list = list.clone();
        let status = status.clone();
        let cache = cache.clone();
        let last_error = last_error.clone();
        let search = search.clone();
        let category_dropdown = category_dropdown.clone();
        let tab_label = tab_label.clone();
        let state = state.clone();
        std::rc::Rc::new(move || {
            // Surface a load error first.
            if let Some(err) = last_error.borrow().as_ref() {
                let result: Result<Vec<RemoteTemplateSummary>, RemoteError> = match err {
                    RemoteError::NotSignedIn => Err(RemoteError::NotSignedIn),
                    other => Err(RemoteError::Malformed(format!("{other}"))),
                };
                populate(&list, &status, kind, result, state.clone());
                return;
            }
            let q = search.text().to_string().to_lowercase();
            let cat_idx = category_dropdown.selected();
            let cats = current_categories(&cache.borrow());
            let cat_filter = if cat_idx == 0 {
                None
            } else {
                cats.get(cat_idx as usize - 1).cloned()
            };
            let filtered: Vec<RemoteTemplateSummary> = cache
                .borrow()
                .iter()
                .filter(|r| {
                    if !q.is_empty() {
                        let in_name = r.name.to_lowercase().contains(&q);
                        let in_desc = r.description.to_lowercase().contains(&q);
                        if !in_name && !in_desc {
                            return false;
                        }
                    }
                    if let Some(c) = &cat_filter {
                        if r.category.as_deref() != Some(c.as_str()) {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect();
            tab_label.set_label(&match cache.borrow().len() {
                0 => kind.tab_base_label().to_string(),
                n => format!("{} ({})", kind.tab_base_label(), n),
            });
            let shown = filtered.len();
            let total = cache.borrow().len();
            let label = if shown == total {
                format!("{} entries", total)
            } else {
                format!("{} of {} entries", shown, total)
            };
            status.set_label(&label);
            populate(&list, &status, kind, Ok(filtered), state.clone());
            status.set_label(&label);
        })
    };

    let refresh: std::rc::Rc<dyn Fn()> = {
        let cache = cache.clone();
        let last_error = last_error.clone();
        let category_dropdown = category_dropdown.clone();
        let status = status.clone();
        let apply_filters = apply_filters.clone();
        std::rc::Rc::new(move || {
            status.set_label("Loading…");
            let result = fetch(kind);
            match result {
                Ok(rows) => {
                    *cache.borrow_mut() = rows;
                    *last_error.borrow_mut() = None;
                    // Refresh the dropdown options against the new
                    // category set, preserving the user's selection
                    // when possible.
                    if matches!(kind, Kind::PageTemplate) {
                        let cats = current_categories(&cache.borrow());
                        let prev_label = if category_dropdown.selected() == 0 {
                            "All categories".to_string()
                        } else {
                            cats.get(category_dropdown.selected() as usize - 1)
                                .cloned()
                                .unwrap_or_else(|| "All categories".to_string())
                        };
                        let mut options = vec!["All categories".to_string()];
                        options.extend(cats.iter().cloned());
                        let strs: Vec<&str> = options.iter().map(|s| s.as_str()).collect();
                        category_dropdown.set_model(Some(&StringList::new(&strs)));
                        let new_idx = options
                            .iter()
                            .position(|c| c == &prev_label)
                            .unwrap_or(0) as u32;
                        category_dropdown.set_selected(new_idx);
                    }
                }
                Err(e) => {
                    *cache.borrow_mut() = Vec::new();
                    *last_error.borrow_mut() = Some(e);
                }
            }
            apply_filters();
        })
    };

    {
        let refresh = refresh.clone();
        refresh_btn.connect_clicked(move |_| refresh());
    }
    {
        let apply = apply_filters.clone();
        search.connect_changed(move |_| apply());
    }
    {
        let apply = apply_filters.clone();
        category_dropdown.connect_selected_notify(move |_| apply());
    }

    refresh();
    (body, tab_label, refresh)
}

/// Distinct sorted list of non-empty `category` values across `rows`.
fn current_categories(rows: &[RemoteTemplateSummary]) -> Vec<String> {
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for r in rows {
        if let Some(c) = r.category.as_ref() {
            if !c.is_empty() {
                set.insert(c.clone());
            }
        }
    }
    set.into_iter().collect()
}

fn fetch(kind: Kind) -> Result<Vec<RemoteTemplateSummary>, RemoteError> {
    let mut s = RemoteTemplateStore::connect()?;
    if !s.is_signed_in() {
        return Err(RemoteError::NotSignedIn);
    }
    match kind {
        Kind::PageTemplate => s.list_public_page_templates(),
        Kind::NotebookTemplate => s.list_public_notebook_templates(),
        Kind::Brush => s.list_public_brushes(),
    }
}

fn populate(
    list: &ListBox,
    status: &Label,
    kind: Kind,
    result: Result<Vec<RemoteTemplateSummary>, RemoteError>,
    state: SharedState,
) {
    while let Some(c) = list.first_child() {
        list.remove(&c);
    }
    match result {
        Err(RemoteError::NotSignedIn) => {
            status.set_label(
                "Sign in via App Settings → Account before browsing the public catalog.",
            );
        }
        Err(e) => {
            status.set_label(&format!("Error: {e}"));
        }
        Ok(rows) if rows.is_empty() => {
            status.set_label("No public entries yet.");
        }
        Ok(rows) => {
            status.set_label(&format!("{} entries", rows.len()));
            for row in rows {
                let lbr = ListBoxRow::new();
                lbr.set_child(Some(&render_row(&row, kind, state.clone())));
                list.append(&lbr);
            }
        }
    }
}

fn render_row(row: &RemoteTemplateSummary, kind: Kind, state: SharedState) -> GtkBox {
    let outer = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(8)
        .margin_end(8)
        .build();

    let text_col = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();
    let name_lbl = Label::builder()
        .label(if row.name.is_empty() {
            "(unnamed)"
        } else {
            row.name.as_str()
        })
        .halign(gtk4::Align::Start)
        .build();
    name_lbl.add_css_class("title-4");
    text_col.append(&name_lbl);

    if !row.description.is_empty() {
        let desc = Label::builder()
            .label(&row.description)
            .halign(gtk4::Align::Start)
            .wrap(true)
            .build();
        desc.add_css_class("dim-label");
        text_col.append(&desc);
    }

    let meta = format!(
        "{}forks {} · views {}{}",
        row.owner
            .as_ref()
            .map(|o| format!("by {} · ", o))
            .unwrap_or_default(),
        row.fork_count,
        row.view_count,
        row.category
            .as_ref()
            .map(|c| format!(" · {}", c))
            .unwrap_or_default(),
    );
    let meta_lbl = Label::builder()
        .label(&meta)
        .halign(gtk4::Align::Start)
        .build();
    meta_lbl.add_css_class("dim-label");
    text_col.append(&meta_lbl);

    outer.append(&text_col);

    // Two actions per row:
    //   * Download — read-only copy of the upstream entry into the
    //     local registry (preserves the original id; user gets a
    //     usable copy but can't republish since they don't own it).
    //   * Edit — server-side fork (fresh id, owned by caller),
    //     saved locally, then opened in the appropriate full-screen
    //     editor for tweaking before publish.
    let download_btn = Button::with_label("Download");
    download_btn.set_valign(gtk4::Align::Center);
    {
        let id = row.id;
        let state = state.clone();
        download_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            b.set_label("Downloading…");
            match download_into_local(kind, id, state.clone()) {
                Ok(()) => b.set_label("Downloaded ✓"),
                Err(e) => {
                    tracing::warn!("download failed: {e}");
                    b.set_label("Download failed");
                    b.set_sensitive(true);
                }
            }
        });
    }
    outer.append(&download_btn);

    let edit_btn = Button::with_label("Edit");
    edit_btn.add_css_class("suggested-action");
    edit_btn.set_valign(gtk4::Align::Center);
    edit_btn.set_tooltip_text(Some("Fork into your library and open in the editor"));
    {
        let id = row.id;
        let state = state.clone();
        edit_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            b.set_label("Forking…");
            match edit_into_local(kind, id, state.clone()) {
                Ok(()) => b.set_label("Opening editor…"),
                Err(e) => {
                    tracing::warn!("edit (fork+open) failed: {e}");
                    b.set_label("Failed");
                    b.set_sensitive(true);
                }
            }
        });
    }
    outer.append(&edit_btn);

    outer
}

/// Read-only download — fetches the upstream entry as-is and inserts
/// into the local registry under the original id. The user gets a
/// usable copy but isn't the owner and can't republish over it.
fn download_into_local(
    kind: Kind,
    id: uuid::Uuid,
    state: SharedState,
) -> Result<(), RemoteError> {
    let mut s = RemoteTemplateStore::connect()?;
    match kind {
        Kind::PageTemplate => {
            let (row, _assets) = s.get_page_template(id)?;
            let s_state = state.borrow();
            let template = crate::template_io::page_template_from_row(&row).map_err(|e| {
                RemoteError::Malformed(format!("parse downloaded page template: {e:#}"))
            })?;
            crate::template_io::put_page_template(
                &s_state.backend,
                &s_state.templates,
                &template,
                &[],
            )
            .map_err(|e| {
                RemoteError::Malformed(format!("save downloaded page template: {e:#}"))
            })?;
        }
        Kind::NotebookTemplate => {
            let row = s.get_notebook_template(id)?;
            let s_state = state.borrow();
            let template =
                crate::template_io::notebook_template_from_row(&row).map_err(|e| {
                    RemoteError::Malformed(format!(
                        "parse downloaded notebook template: {e:#}"
                    ))
                })?;
            let _ = crate::template_io::put_notebook_template(
                &s_state.backend,
                &s_state.notebook_templates,
                &template,
            );
        }
        Kind::Brush => {
            let row = s.get_brush(id)?;
            let s_state = state.borrow();
            put_brush_into_local(&s_state, &row).map_err(|e| {
                RemoteError::Malformed(format!("save downloaded brush: {e:#}"))
            })?;
        }
    }
    Ok(())
}

/// Server-side fork → save into local registry → hand off to the
/// matching full-screen editor (looked up via the `EDITOR_OPENERS`
/// thread-local, populated by `window.rs` once the main window is
/// built). Editor-open is best-effort: if openers aren't registered
/// the user still gets a forked copy in their library.
fn edit_into_local(kind: Kind, id: uuid::Uuid, state: SharedState) -> Result<(), RemoteError> {
    let mut s = RemoteTemplateStore::connect()?;
    match kind {
        Kind::PageTemplate => {
            let row = s.fork_page_template(id)?;
            let template = crate::template_io::page_template_from_row(&row).map_err(|e| {
                RemoteError::Malformed(format!("parse forked page template: {e:#}"))
            })?;
            {
                let s_state = state.borrow();
                crate::template_io::put_page_template(
                    &s_state.backend,
                    &s_state.templates,
                    &template,
                    &[],
                )
                .map_err(|e| {
                    RemoteError::Malformed(format!("save forked page template: {e:#}"))
                })?;
            }
            with_openers(|openers| {
                if let Some(o) = openers {
                    (o.page)(template.clone());
                } else {
                    tracing::warn!("page template editor opener not registered");
                }
            });
        }
        Kind::NotebookTemplate => {
            let row = s.fork_notebook_template(id)?;
            let template = crate::template_io::notebook_template_from_row(&row).map_err(|e| {
                RemoteError::Malformed(format!("parse forked notebook template: {e:#}"))
            })?;
            {
                let s_state = state.borrow();
                let _ = crate::template_io::put_notebook_template(
                    &s_state.backend,
                    &s_state.notebook_templates,
                    &template,
                );
            }
            with_openers(|openers| {
                if let Some(o) = openers {
                    (o.notebook)(template.clone());
                } else {
                    tracing::warn!("notebook template editor opener not registered");
                }
            });
        }
        Kind::Brush => {
            let row = s.fork_brush(id)?;
            {
                let s_state = state.borrow();
                put_brush_into_local(&s_state, &row).map_err(|e| {
                    RemoteError::Malformed(format!("save forked brush: {e:#}"))
                })?;
            }
            // Reconstruct a Brush from BrushRow.body_toml so the
            // editor opens on the cloned shape.
            let brush: journal_core::Brush = match toml::from_str(&row.body_toml) {
                Ok(b) => b,
                Err(e) => {
                    return Err(RemoteError::Malformed(format!(
                        "parse forked brush body: {e}"
                    )));
                }
            };
            with_openers(|openers| {
                if let Some(o) = openers {
                    (o.brush)(brush.clone());
                } else {
                    tracing::warn!("brush editor opener not registered");
                }
            });
        }
    }
    Ok(())
}

fn put_brush_into_local(
    state: &std::cell::Ref<crate::state::CanvasState>,
    row: &journal_storage::BrushRow,
) -> anyhow::Result<()> {
    state.backend.borrow_mut().put_brush(row)?;
    Ok(())
}

// ── publish helpers (callable from template_manager / brush_library) ──

pub use journal_storage::remote_template_store::store::Visibility;

/// Modal that lets the user pick a visibility before a publish
/// completes. `on_pick` is called with the chosen visibility on
/// confirm; the modal closes itself either way. Cancel = no callback.
pub fn pick_visibility(
    parent: &ApplicationWindow,
    title_hint: &str,
    on_pick: impl Fn(Visibility) + 'static,
) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Publish")
        .default_width(380)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    let header = Label::builder()
        .label(&format!("Publish {} as:", title_hint))
        .halign(gtk4::Align::Start)
        .build();
    header.add_css_class("title-4");
    body.append(&header);

    let public_radio = gtk4::CheckButton::builder()
        .label("Public — visible to everyone in the catalog")
        .build();
    public_radio.set_active(true);
    let unlisted_radio = gtk4::CheckButton::builder()
        .label("Unlisted — only people with the link")
        .group(&public_radio)
        .build();
    let private_radio = gtk4::CheckButton::builder()
        .label("Private — uploaded but only you can see it")
        .group(&public_radio)
        .build();
    body.append(&public_radio);
    body.append(&unlisted_radio);
    body.append(&private_radio);

    let btn_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk4::Align::End)
        .build();
    let cancel_btn = Button::with_label("Cancel");
    let ok_btn = Button::with_label("Publish");
    ok_btn.add_css_class("suggested-action");
    btn_row.append(&cancel_btn);
    btn_row.append(&ok_btn);
    body.append(&btn_row);

    {
        let win = win.clone();
        cancel_btn.connect_clicked(move |_| win.close());
    }
    {
        let win = win.clone();
        let public_radio = public_radio.clone();
        let unlisted_radio = unlisted_radio.clone();
        let on_pick = std::rc::Rc::new(on_pick);
        ok_btn.connect_clicked(move |_| {
            let vis = if public_radio.is_active() {
                Visibility::Public
            } else if unlisted_radio.is_active() {
                Visibility::Unlisted
            } else {
                Visibility::Private
            };
            (on_pick)(vis);
            win.close();
        });
    }

    win.set_child(Some(&body));
    win.present();
}

/// Publish a local page template (and its asset bytes) to the
/// catalog with the chosen visibility. Returns once both the asset
/// upload and the publishPageTemplate mutation succeed. Fire-and-log
/// on the caller side; no UI feedback beyond the button label.
pub fn publish_local_page_template(
    template: &journal_core::PageTemplate,
    state: SharedState,
    visibility: Visibility,
) -> Result<(), RemoteError> {
    let mut store = RemoteTemplateStore::connect()?;
    if !store.is_signed_in() {
        return Err(RemoteError::NotSignedIn);
    }

    // Build the row + collect any inline asset bytes from the local
    // backend.
    let (row, assets) = {
        let s = state.borrow();
        let row = crate::template_io::page_template_to_row(template)
            .map_err(|e| RemoteError::Malformed(format!("serialize page template: {e:#}")))?;
        let asset_metas = s
            .backend
            .borrow_mut()
            .list_page_template_assets(template.id.0)
            .map_err(|e| RemoteError::Malformed(format!("list assets: {e:#}")))?;
        let mut bytes = Vec::with_capacity(asset_metas.len());
        for meta in asset_metas {
            match s
                .backend
                .borrow_mut()
                .get_page_template_asset(template.id.0, &meta.name)
            {
                Ok(Some(b)) => bytes.push(b),
                Ok(None) => {
                    tracing::warn!("asset {} missing in local backend, skipping", meta.name);
                }
                Err(e) => {
                    return Err(RemoteError::Malformed(format!(
                        "load asset {}: {e:#}",
                        meta.name
                    )));
                }
            }
        }
        (row, bytes)
    };

    store.publish_page_template(&row, &assets, visibility)?;
    Ok(())
}

/// Publish a local notebook template. No assets to upload (notebook
/// templates have no binary attachments).
pub fn publish_local_notebook_template(
    template: &journal_core::NotebookTemplate,
    state: SharedState,
    visibility: Visibility,
) -> Result<(), RemoteError> {
    let mut store = RemoteTemplateStore::connect()?;
    if !store.is_signed_in() {
        return Err(RemoteError::NotSignedIn);
    }
    let row = {
        let _s = state.borrow();
        crate::template_io::notebook_template_to_row(template)
            .map_err(|e| RemoteError::Malformed(format!("serialize notebook template: {e:#}")))?
    };
    store.publish_notebook_template(&row, visibility)?;
    Ok(())
}

/// Publish a local brush. No assets to upload (brushes have no
/// binary attachments).
pub fn publish_local_brush(
    brush: &journal_core::Brush,
    state: SharedState,
    visibility: Visibility,
) -> Result<(), RemoteError> {
    let mut store = RemoteTemplateStore::connect()?;
    if !store.is_signed_in() {
        return Err(RemoteError::NotSignedIn);
    }
    // Re-derive the BrushRow from the in-memory Brush. The backend
    // already stores it in the same shape.
    let body_toml = toml::to_string(brush)
        .map_err(|e| RemoteError::Malformed(format!("serialize brush: {e}")))?;
    let sha = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(body_toml.as_bytes());
        hex::encode(h.finalize())
    };
    let row = journal_storage::BrushRow {
        id: brush.id,
        name: brush.name.clone(),
        body_toml,
        sha256: sha,
        updated_at_sort: chrono::Utc::now().to_rfc3339(),
    };
    let _ = state; // currently not needed; kept for symmetry
    store.publish_brush(&row, visibility)?;
    Ok(())
}
