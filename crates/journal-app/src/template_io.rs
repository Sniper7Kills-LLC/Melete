//! Page-template / notebook-template ↔ `journal_storage::TemplateRow`
//! helpers. Since Phase 6.3, both registries are populated from
//! the local SQLite backend rather than `~/.local/share/journal/{templates,notebook_templates}/*.toml`.
//! This module is the lowering / lifting layer in front of those
//! `JournalBackend::{list,put,delete}_*_template` calls.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Context, Result};
use chrono::Utc;
use journal_core::{NotebookTemplate, PageTemplate};
use journal_storage::{AssetBytes, JournalBackend, TemplateRow};
use journal_templates::{
    parse_template_toml, serialize_template_toml, template_file_from_page_template,
    template_file_to_page_template, NotebookTemplateRegistry, TemplateRegistry,
};
use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn page_template_to_row(t: &PageTemplate) -> Result<TemplateRow> {
    let file = template_file_from_page_template(t);
    let body_toml = serialize_template_toml(&file)
        .map_err(|e| anyhow::anyhow!("serialize page template: {}", e))?;
    let sha = sha256_hex(body_toml.as_bytes());
    Ok(TemplateRow {
        id: t.id.0,
        name: t.name.clone(),
        description: t.description.clone(),
        category: t.category.clone(),
        body_toml,
        sha256: sha,
        updated_at_sort: now_rfc3339(),
    })
}

pub fn page_template_from_row(row: &TemplateRow) -> Result<PageTemplate> {
    let parsed = parse_template_toml(&row.body_toml)
        .with_context(|| format!("parse page template {}", row.id))?;
    Ok(template_file_to_page_template(parsed))
}

pub fn notebook_template_to_row(t: &NotebookTemplate) -> Result<TemplateRow> {
    let body_toml =
        toml::to_string(t).with_context(|| format!("serialize notebook template {}", t.id.0))?;
    let sha = sha256_hex(body_toml.as_bytes());
    Ok(TemplateRow {
        id: t.id.0,
        name: t.name.clone(),
        description: String::new(),
        category: String::new(),
        body_toml,
        sha256: sha,
        updated_at_sort: now_rfc3339(),
    })
}

pub fn notebook_template_from_row(row: &TemplateRow) -> Result<NotebookTemplate> {
    toml::from_str::<NotebookTemplate>(&row.body_toml)
        .with_context(|| format!("parse notebook template {}", row.id))
}

/// Pull every page + notebook template out of the backend and
/// populate the in-memory registries. Built-in entries seeded by
/// `with_builtins()` survive (the registry is a `HashMap` so backend
/// rows with the same id overwrite a builtin; otherwise both coexist).
/// Run once after the backend opens.
pub fn hydrate_registries_from_backend(
    backend: &Rc<RefCell<dyn JournalBackend>>,
    page_reg: &Rc<RefCell<TemplateRegistry>>,
    nb_reg: &Rc<RefCell<NotebookTemplateRegistry>>,
) {
    let mut b = backend.borrow_mut();
    match b.list_page_templates() {
        Ok(rows) => {
            let mut reg = page_reg.borrow_mut();
            let mut n = 0usize;
            for row in &rows {
                match page_template_from_row(row) {
                    Ok(t) => {
                        reg.insert(t);
                        n += 1;
                    }
                    Err(e) => tracing::warn!("hydrate page template {}: {:#}", row.id, e),
                }
            }
            tracing::info!("hydrated {} page templates from backend", n);
        }
        Err(e) => tracing::warn!("list_page_templates: {:#}", e),
    }
    match b.list_notebook_templates() {
        Ok(rows) => {
            let mut reg = nb_reg.borrow_mut();
            let mut n = 0usize;
            for row in &rows {
                match notebook_template_from_row(row) {
                    Ok(t) => {
                        reg.insert(t);
                        n += 1;
                    }
                    Err(e) => tracing::warn!("hydrate notebook template {}: {:#}", row.id, e),
                }
            }
            tracing::info!("hydrated {} notebook templates from backend", n);
        }
        Err(e) => tracing::warn!("list_notebook_templates: {:#}", e),
    }
}

/// Persist a notebook template to backend, then re-insert the new
/// Persist a notebook template via the backend and refresh the
/// in-memory registry. Returns `Err` on serialize / backend failure
/// so editor save handlers can keep the editor open instead of
/// auto-closing on failed saves (#22).
pub fn put_notebook_template(
    backend: &Rc<RefCell<dyn JournalBackend>>,
    nb_reg: &Rc<RefCell<NotebookTemplateRegistry>>,
    t: &NotebookTemplate,
) -> Result<()> {
    let row = notebook_template_to_row(t)
        .map_err(|e| anyhow::anyhow!("serialize notebook template {}: {:#}", t.id.0, e))?;
    backend
        .borrow_mut()
        .put_notebook_template(&row)
        .map_err(|e| anyhow::anyhow!("put_notebook_template {}: {:#}", t.id.0, e))?;
    nb_reg.borrow_mut().insert(t.clone());
    Ok(())
}

pub fn delete_notebook_template(
    backend: &Rc<RefCell<dyn JournalBackend>>,
    nb_reg: &Rc<RefCell<NotebookTemplateRegistry>>,
    id: uuid::Uuid,
) {
    if let Err(e) = backend.borrow_mut().delete_notebook_template(id) {
        tracing::warn!("delete_notebook_template {}: {:#}", id, e);
    }
    nb_reg
        .borrow_mut()
        .remove(journal_core::TemplateId(id));
}

/// Persist a page template (plus optional inline asset bytes) to the
/// backend, then upsert into the in-memory registry. Used by the
/// import / template-editor save paths.
pub fn put_page_template(
    backend: &Rc<RefCell<dyn JournalBackend>>,
    page_reg: &Rc<RefCell<TemplateRegistry>>,
    t: &PageTemplate,
    assets: &[AssetBytes],
) -> Result<()> {
    let row = page_template_to_row(t)?;
    backend
        .borrow_mut()
        .put_page_template(&row, assets)
        .map_err(|e| anyhow::anyhow!("put_page_template: {:#}", e))?;
    page_reg.borrow_mut().insert(t.clone());
    Ok(())
}

pub fn delete_page_template(
    backend: &Rc<RefCell<dyn JournalBackend>>,
    page_reg: &Rc<RefCell<TemplateRegistry>>,
    id: uuid::Uuid,
) {
    if let Err(e) = backend.borrow_mut().delete_page_template(id) {
        tracing::warn!("delete_page_template {}: {:#}", id, e);
    }
    page_reg
        .borrow_mut()
        .remove(journal_core::TemplateId(id));
}

/// Build an `AssetBytes` from raw bytes + filename hint, computing
/// sha256. `name` becomes the `asset:<name>` URI fragment templates
/// reference.
pub fn asset_bytes_from_file(name: String, mime: &str, bytes: Vec<u8>) -> AssetBytes {
    let sha = sha256_hex(&bytes);
    AssetBytes {
        name,
        mime: mime.to_string(),
        sha256: sha,
        bytes,
    }
}
