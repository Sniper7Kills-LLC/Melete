//! Free functions for the `brushes` table in `index.db`.
//!
//! Mirrors the per-store module layout used by `notebook_store`,
//! `section_store` etc. — keeps `MultiFileSqliteBackend` thin and lets
//! a future in-memory test impl share the same primitives.

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::backend::BrushRow;
use crate::error::{Result, StorageError};
use crate::util::{blob_to_uuid, uuid_to_blob};

pub(crate) fn list_brushes(conn: &Connection) -> Result<Vec<BrushRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, body_toml, sha256, updated_at_sort
         FROM brushes ORDER BY name COLLATE NOCASE ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        let id: Vec<u8> = r.get(0)?;
        let name: String = r.get(1)?;
        let body_toml: String = r.get(2)?;
        let sha256: String = r.get(3)?;
        let updated_at_sort: String = r.get(4)?;
        Ok((id, name, body_toml, sha256, updated_at_sort))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (id, name, body_toml, sha256, updated_at_sort) = r?;
        out.push(BrushRow {
            id: blob_to_uuid(&id)?,
            name,
            body_toml,
            sha256,
            updated_at_sort,
        });
    }
    Ok(out)
}

pub(crate) fn get_brush(conn: &Connection, id: Uuid) -> Result<BrushRow> {
    let row = conn
        .query_row(
            "SELECT name, body_toml, sha256, updated_at_sort FROM brushes WHERE id = ?1",
            params![uuid_to_blob(id)],
            |r| {
                let name: String = r.get(0)?;
                let body: String = r.get(1)?;
                let sha: String = r.get(2)?;
                let uas: String = r.get(3)?;
                Ok((name, body, sha, uas))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => StorageError::NotFound,
            other => StorageError::Sqlite(other),
        })?;
    Ok(BrushRow {
        id,
        name: row.0,
        body_toml: row.1,
        sha256: row.2,
        updated_at_sort: row.3,
    })
}

pub(crate) fn put_brush(conn: &Connection, row: &BrushRow) -> Result<()> {
    let updated_at = chrono::Utc::now().to_rfc3339();
    // If the caller didn't pre-stamp the row, default to "now" so
    // sort consumers can rely on the column always being a parseable
    // RFC3339 string.
    let updated_at_sort = if row.updated_at_sort.is_empty() {
        updated_at.clone()
    } else {
        row.updated_at_sort.clone()
    };
    conn.execute(
        "INSERT INTO brushes (id, name, body_toml, sha256, updated_at, updated_at_sort)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             body_toml = excluded.body_toml,
             sha256 = excluded.sha256,
             updated_at = excluded.updated_at,
             updated_at_sort = excluded.updated_at_sort",
        params![
            uuid_to_blob(row.id),
            row.name,
            row.body_toml,
            row.sha256,
            updated_at,
            updated_at_sort,
        ],
    )?;
    Ok(())
}

pub(crate) fn delete_brush(conn: &Connection, id: Uuid) -> Result<()> {
    let n = conn.execute(
        "DELETE FROM brushes WHERE id = ?1",
        params![uuid_to_blob(id)],
    )?;
    if n == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
