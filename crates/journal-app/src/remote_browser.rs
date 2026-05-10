//! "Browse public templates" window. Lists public page templates,
//! notebook templates, and brushes from the Amplify catalog and lets
//! the user fork an entry into their local registry.
//!
//! Gated on the `remote` feature so non-cloud builds drop the menu
//! entry entirely.

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, Label, ListBox, ListBoxRow, Notebook, Orientation,
    ScrolledWindow, SelectionMode, Window,
};

use journal_storage::remote_template_store::store::{
    RemoteError, RemoteTemplateOps, RemoteTemplateStore, RemoteTemplateSummary,
};

use crate::state::SharedState;

pub fn open_browser(parent: &ApplicationWindow, state: SharedState) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Browse public catalog")
        .default_width(720)
        .default_height(720)
        .build();

    let outer = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(0)
        .build();

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();
    let title = Label::builder()
        .label("Public templates + brushes")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    title.add_css_class("title-3");
    let close_btn = Button::with_label("Close");
    {
        let win = win.clone();
        close_btn.connect_clicked(move |_| win.close());
    }
    header.append(&title);
    header.append(&close_btn);
    outer.append(&header);

    // Tabs: Page Templates / Notebook Templates / Brushes.
    let notebook = Notebook::builder().vexpand(true).hexpand(true).build();
    notebook.append_page(
        &build_tab(state.clone(), Kind::PageTemplate),
        Some(&Label::new(Some("Page templates"))),
    );
    notebook.append_page(
        &build_tab(state.clone(), Kind::NotebookTemplate),
        Some(&Label::new(Some("Notebook templates"))),
    );
    notebook.append_page(
        &build_tab(state.clone(), Kind::Brush),
        Some(&Label::new(Some("Brushes"))),
    );
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

fn build_tab(state: SharedState, kind: Kind) -> GtkBox {
    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(12)
        .margin_end(12)
        .build();

    let status = Label::builder()
        .label("Loading…")
        .halign(gtk4::Align::Start)
        .build();
    status.add_css_class("dim-label");
    body.append(&status);

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

    // Synchronous fetch on the GTK main thread — sub-second over a
    // healthy connection. If this becomes laggy we move to a worker
    // thread + glib::idle_add.
    let result = fetch(kind);
    populate(&list, &status, kind, result, state);
    body
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

    let fork_btn = Button::with_label("Fork");
    fork_btn.add_css_class("suggested-action");
    fork_btn.set_valign(gtk4::Align::Center);
    {
        let id = row.id;
        let state = state.clone();
        fork_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            b.set_label("Forking…");
            match fork_into_local(kind, id, state.clone()) {
                Ok(()) => b.set_label("Forked ✓"),
                Err(e) => {
                    tracing::warn!("fork failed: {e}");
                    b.set_label("Fork failed");
                    b.set_sensitive(true);
                }
            }
        });
    }
    outer.append(&fork_btn);

    outer
}

fn fork_into_local(kind: Kind, id: uuid::Uuid, state: SharedState) -> Result<(), RemoteError> {
    let mut s = RemoteTemplateStore::connect()?;
    match kind {
        Kind::PageTemplate => {
            let row = s.fork_page_template(id)?;
            let s_state = state.borrow();
            let template = crate::template_io::page_template_from_row(&row).map_err(|e| {
                RemoteError::Malformed(format!("parse forked page template: {e:#}"))
            })?;
            crate::template_io::put_page_template(
                &s_state.backend,
                &s_state.templates,
                &template,
                &[],
            )
            .map_err(|e| RemoteError::Malformed(format!("save forked page template: {e:#}")))?;
        }
        Kind::NotebookTemplate => {
            let row = s.fork_notebook_template(id)?;
            let s_state = state.borrow();
            let template = crate::template_io::notebook_template_from_row(&row).map_err(|e| {
                RemoteError::Malformed(format!("parse forked notebook template: {e:#}"))
            })?;
            crate::template_io::put_notebook_template(
                &s_state.backend,
                &s_state.notebook_templates,
                &template,
            );
        }
        Kind::Brush => {
            let row = s.fork_brush(id)?;
            let s_state = state.borrow();
            put_brush_into_local(&s_state, &row).map_err(|e| {
                RemoteError::Malformed(format!("save forked brush: {e:#}"))
            })?;
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

use journal_storage::remote_template_store::store::Visibility;

/// Publish a local page template (and its asset bytes) to the public
/// catalog. Returns once both the asset upload and the
/// publishPageTemplate mutation succeed. Fire-and-log on the caller
/// side; no UI feedback beyond the button label.
pub fn publish_local_page_template(
    template: &journal_core::PageTemplate,
    state: SharedState,
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

    store.publish_page_template(&row, &assets, Visibility::Public)?;
    Ok(())
}

/// Publish a local notebook template. No assets to upload (notebook
/// templates have no binary attachments).
#[allow(dead_code)]
pub fn publish_local_notebook_template(
    template: &journal_core::NotebookTemplate,
    state: SharedState,
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
    store.publish_notebook_template(&row, Visibility::Public)?;
    Ok(())
}

/// Publish a local brush. No assets to upload (brushes have no
/// binary attachments).
#[allow(dead_code)]
pub fn publish_local_brush(
    brush: &journal_core::Brush,
    state: SharedState,
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
    store.publish_brush(&row, Visibility::Public)?;
    Ok(())
}
