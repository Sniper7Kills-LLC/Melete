//! Free functions for the page/notebook template + asset tables in
//! `index.db`. The `MultiFileSqliteBackend::TemplateStore` impl is a
//! thin wrapper over these.
//!
//! Schema (see `multi_file_backend::INDEX_TEMPLATE_SCHEMA`):
//! - `page_templates(id, name, description, category, body_toml, sha256, updated_at)`
//! - `notebook_templates(id, name, body_toml, sha256, updated_at)` —
//!   no description / category column; notebook templates carry that
//!   metadata inside their body if they need it.
//! - `page_template_assets(template_id, name, mime, sha256, bytes)`
//!   with `ON DELETE CASCADE` from `page_templates`.

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::backend::{AssetBytes, AssetMeta, TemplateRow};
use crate::error::{Result, StorageError};
use crate::util::{blob_to_uuid, uuid_to_blob};

// ── page templates ─────────────────────────────────────────────────────

pub(crate) fn list_page_templates(conn: &Connection) -> Result<Vec<TemplateRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, category, body_toml, sha256
         FROM page_templates ORDER BY name COLLATE NOCASE ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        let id: Vec<u8> = r.get(0)?;
        let name: String = r.get(1)?;
        let desc: String = r.get(2)?;
        let cat: String = r.get(3)?;
        let body: String = r.get(4)?;
        let sha: String = r.get(5)?;
        Ok((id, name, desc, cat, body, sha))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (id, name, description, category, body_toml, sha256) = r?;
        out.push(TemplateRow {
            id: blob_to_uuid(&id)?,
            name,
            description,
            category,
            body_toml,
            sha256,
        });
    }
    Ok(out)
}

pub(crate) fn get_page_template(conn: &Connection, id: Uuid) -> Result<TemplateRow> {
    let row = conn
        .query_row(
            "SELECT name, description, category, body_toml, sha256
             FROM page_templates WHERE id = ?1",
            params![uuid_to_blob(id)],
            |r| {
                let name: String = r.get(0)?;
                let desc: String = r.get(1)?;
                let cat: String = r.get(2)?;
                let body: String = r.get(3)?;
                let sha: String = r.get(4)?;
                Ok((name, desc, cat, body, sha))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => StorageError::NotFound,
            other => StorageError::Sqlite(other),
        })?;
    Ok(TemplateRow {
        id,
        name: row.0,
        description: row.1,
        category: row.2,
        body_toml: row.3,
        sha256: row.4,
    })
}

/// Upsert a `page_templates` row plus replace the full asset set.
/// Wrapped in a transaction so partial writes can't leave dangling
/// `asset:<name>` references in the body.
pub(crate) fn put_page_template(
    conn: &mut Connection,
    row: &TemplateRow,
    assets: &[AssetBytes],
) -> Result<()> {
    let tx = conn.transaction()?;
    put_page_template_in(&tx, row, assets)?;
    tx.commit()?;
    Ok(())
}

/// Upsert variant that runs on an existing connection / transaction
/// without opening one of its own. Used by the catalog migration,
/// which wraps the whole walk in a single outer transaction.
pub(crate) fn put_page_template_in(
    conn: &Connection,
    row: &TemplateRow,
    assets: &[AssetBytes],
) -> Result<()> {
    let updated_at = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO page_templates (id, name, description, category, body_toml, sha256, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             description = excluded.description,
             category = excluded.category,
             body_toml = excluded.body_toml,
             sha256 = excluded.sha256,
             updated_at = excluded.updated_at",
        params![
            uuid_to_blob(row.id),
            row.name,
            row.description,
            row.category,
            row.body_toml,
            row.sha256,
            updated_at
        ],
    )?;
    // Replace the asset set wholesale — `put` is the canonical "this
    // is the template's content now" call. Callers that only want to
    // touch one asset should round-trip through `get_*_asset` first.
    conn.execute(
        "DELETE FROM page_template_assets WHERE template_id = ?1",
        params![uuid_to_blob(row.id)],
    )?;
    {
        let mut stmt = conn.prepare(
            "INSERT INTO page_template_assets (template_id, name, mime, sha256, bytes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for a in assets {
            stmt.execute(params![
                uuid_to_blob(row.id),
                a.name,
                a.mime,
                a.sha256,
                a.bytes
            ])?;
        }
    }
    Ok(())
}

pub(crate) fn delete_page_template(conn: &Connection, id: Uuid) -> Result<()> {
    let n = conn.execute(
        "DELETE FROM page_templates WHERE id = ?1",
        params![uuid_to_blob(id)],
    )?;
    if n == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

pub(crate) fn list_page_template_assets(
    conn: &Connection,
    template_id: Uuid,
) -> Result<Vec<AssetMeta>> {
    let mut stmt = conn.prepare(
        "SELECT name, mime, sha256, length(bytes) FROM page_template_assets
         WHERE template_id = ?1 ORDER BY name ASC",
    )?;
    let rows = stmt.query_map(params![uuid_to_blob(template_id)], |r| {
        let name: String = r.get(0)?;
        let mime: String = r.get(1)?;
        let sha: String = r.get(2)?;
        let size: i64 = r.get(3)?;
        Ok((name, mime, sha, size))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (name, mime, sha256, size) = r?;
        out.push(AssetMeta {
            name,
            mime,
            sha256,
            size: size.max(0) as u64,
        });
    }
    Ok(out)
}

pub(crate) fn get_page_template_asset(
    conn: &Connection,
    template_id: Uuid,
    name: &str,
) -> Result<Option<AssetBytes>> {
    let row: Option<(String, String, Vec<u8>)> = conn
        .query_row(
            "SELECT mime, sha256, bytes FROM page_template_assets
             WHERE template_id = ?1 AND name = ?2",
            params![uuid_to_blob(template_id), name],
            |r| {
                let mime: String = r.get(0)?;
                let sha: String = r.get(1)?;
                let bytes: Vec<u8> = r.get(2)?;
                Ok((mime, sha, bytes))
            },
        )
        .optional()?;
    Ok(row.map(|(mime, sha256, bytes)| AssetBytes {
        name: name.to_string(),
        mime,
        sha256,
        bytes,
    }))
}

// ── notebook templates ─────────────────────────────────────────────────

pub(crate) fn list_notebook_templates(conn: &Connection) -> Result<Vec<TemplateRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, body_toml, sha256
         FROM notebook_templates ORDER BY name COLLATE NOCASE ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        let id: Vec<u8> = r.get(0)?;
        let name: String = r.get(1)?;
        let body: String = r.get(2)?;
        let sha: String = r.get(3)?;
        Ok((id, name, body, sha))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (id, name, body_toml, sha256) = r?;
        out.push(TemplateRow {
            id: blob_to_uuid(&id)?,
            name,
            description: String::new(),
            category: String::new(),
            body_toml,
            sha256,
        });
    }
    Ok(out)
}

pub(crate) fn get_notebook_template(conn: &Connection, id: Uuid) -> Result<TemplateRow> {
    let row = conn
        .query_row(
            "SELECT name, body_toml, sha256 FROM notebook_templates WHERE id = ?1",
            params![uuid_to_blob(id)],
            |r| {
                let name: String = r.get(0)?;
                let body: String = r.get(1)?;
                let sha: String = r.get(2)?;
                Ok((name, body, sha))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => StorageError::NotFound,
            other => StorageError::Sqlite(other),
        })?;
    Ok(TemplateRow {
        id,
        name: row.0,
        description: String::new(),
        category: String::new(),
        body_toml: row.1,
        sha256: row.2,
    })
}

pub(crate) fn put_notebook_template(conn: &Connection, row: &TemplateRow) -> Result<()> {
    let updated_at = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO notebook_templates (id, name, body_toml, sha256, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             body_toml = excluded.body_toml,
             sha256 = excluded.sha256,
             updated_at = excluded.updated_at",
        params![
            uuid_to_blob(row.id),
            row.name,
            row.body_toml,
            row.sha256,
            updated_at
        ],
    )?;
    Ok(())
}

pub(crate) fn delete_notebook_template(conn: &Connection, id: Uuid) -> Result<()> {
    let n = conn.execute(
        "DELETE FROM notebook_templates WHERE id = ?1",
        params![uuid_to_blob(id)],
    )?;
    if n == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
