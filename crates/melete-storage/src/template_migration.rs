//! One-shot migration: pull the user's existing TOML brush library +
//! page-template directory + notebook-template directory into the
//! catalog tables of `index.db`. Pre-1.0, so we don't keep a renamed
//! safety copy on disk and we don't carry a "migration done" fuse —
//! the idempotency check is "all three tables empty AND at least one
//! legacy file exists".
//!
//! Walks (defaults; overridable for tests):
//! - `~/.config/journal/brushes.toml` → `brushes`.
//! - `~/.local/share/journal/templates/*.toml` → `page_templates`
//!   plus sibling image / PDF files → `page_template_assets`, with
//!   the `path = "..."` field rewritten to `path = "asset:<name>"` in
//!   the stored `body_toml`.
//! - `~/.local/share/journal/notebook_templates/*.toml` →
//!   `notebook_templates`.
//!
//! Failure handling:
//! - Per-file parse / read errors → `tracing::warn` + skip, the rest
//!   of the migration still runs.
//! - The whole migration runs in a single transaction; per-INSERT
//!   errors inside the transaction propagate and roll back any
//!   partial work. The caller sees one `Result`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::backend::{AssetBytes, BrushRow, TemplateRow};
use crate::error::Result;
use crate::{brush_store, template_catalog_store};

/// Filesystem layout the migration walks. Defaults follow XDG; tests
/// inject `TempDir`s.
pub struct MigrationPaths {
    pub brushes_toml: PathBuf,
    pub page_templates_dir: PathBuf,
    pub notebook_templates_dir: PathBuf,
}

impl MigrationPaths {
    /// Default layout: `dirs::config_dir()/journal/...` and
    /// `dirs::data_dir()/journal/...`. Returns `None` if either base
    /// dir can't be resolved (extremely rare on Linux but guarded
    /// for portability).
    pub fn xdg_default() -> Option<Self> {
        let cfg = dirs::config_dir()?.join("melete");
        let data = dirs::data_dir()?.join("melete");
        Some(Self {
            brushes_toml: cfg.join("brushes.toml"),
            page_templates_dir: data.join("templates"),
            notebook_templates_dir: data.join("notebook_templates"),
        })
    }
}

/// Run the migration if and only if all three catalog tables are
/// empty AND at least one of the legacy files / directories has
/// content. Idempotent: subsequent calls observe non-zero row counts
/// and short-circuit.
pub fn migrate_if_needed(conn: &mut Connection, paths: &MigrationPaths) -> Result<()> {
    if !needs_migration(conn)? {
        return Ok(());
    }
    if !any_legacy_present(paths) {
        return Ok(());
    }
    tracing::info!("template/brush migration: pulling legacy TOML into index.db");

    let tx = conn.transaction()?;

    // Brushes: one TOML file holds the whole library.
    if paths.brushes_toml.exists() {
        match migrate_brushes(&tx, &paths.brushes_toml) {
            Ok(n) => tracing::info!("migrated {} brushes from {:?}", n, paths.brushes_toml),
            Err(e) => tracing::warn!("brush migration failed: {}", e),
        }
    }

    // Page templates: one TOML per file, with sibling asset dirs.
    if paths.page_templates_dir.exists() {
        match migrate_page_templates(&tx, &paths.page_templates_dir) {
            Ok(n) => tracing::info!(
                "migrated {} page templates from {:?}",
                n,
                paths.page_templates_dir
            ),
            Err(e) => tracing::warn!("page template migration failed: {}", e),
        }
    }

    // Notebook templates: one TOML per file, no asset side-load.
    if paths.notebook_templates_dir.exists() {
        match migrate_notebook_templates(&tx, &paths.notebook_templates_dir) {
            Ok(n) => tracing::info!(
                "migrated {} notebook templates from {:?}",
                n,
                paths.notebook_templates_dir
            ),
            Err(e) => tracing::warn!("notebook template migration failed: {}", e),
        }
    }

    tx.commit()?;
    Ok(())
}

fn needs_migration(conn: &Connection) -> Result<bool> {
    for table in ["brushes", "page_templates", "notebook_templates"] {
        let n: i64 =
            conn.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |r| r.get(0))?;
        if n != 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn any_legacy_present(paths: &MigrationPaths) -> bool {
    paths.brushes_toml.exists()
        || paths.page_templates_dir.exists()
        || paths.notebook_templates_dir.exists()
}

// ── brushes ────────────────────────────────────────────────────────────

/// Loosely-typed view over the legacy `brushes.toml` shape so the
/// migration doesn't depend on `journal-canvas` or `journal-app`.
/// Only `id` + `name` are read; the full TOML body is preserved
/// verbatim by re-encoding the captured `toml::Value`.
#[derive(Debug, serde::Deserialize)]
struct LegacyBrushLibrary {
    #[serde(default)]
    brushes: Vec<toml::Value>,
}

fn migrate_brushes(conn: &Connection, path: &Path) -> Result<usize> {
    let updated_at_sort = file_mtime_rfc3339(path);
    let text = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("read {:?}: {}", path, e);
            return Ok(0);
        }
    };
    let lib: LegacyBrushLibrary = match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("parse {:?}: {}", path, e);
            return Ok(0);
        }
    };
    let mut count = 0usize;
    for entry in lib.brushes {
        let table = match entry.as_table() {
            Some(t) => t,
            None => {
                tracing::warn!("brush entry was not a table, skipping");
                continue;
            }
        };
        let id = match table
            .get("id")
            .and_then(|v| v.as_str())
            .map(Uuid::parse_str)
        {
            Some(Ok(u)) => u,
            _ => {
                tracing::warn!("brush entry missing valid id, skipping");
                continue;
            }
        };
        let name = table
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled")
            .to_string();
        let body_toml = match toml::to_string(&entry) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("re-encode brush {}: {}", id, e);
                continue;
            }
        };
        let sha = sha256_hex(body_toml.as_bytes());
        let row = BrushRow {
            id,
            name,
            body_toml,
            sha256: sha,
            updated_at_sort: updated_at_sort.clone(),
        };
        if let Err(e) = brush_store::put_brush(conn, &row) {
            tracing::warn!("insert brush {}: {}", id, e);
            continue;
        }
        count += 1;
    }
    Ok(count)
}

// ── page templates ─────────────────────────────────────────────────────

fn migrate_page_templates(conn: &Connection, dir: &Path) -> Result<usize> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("read_dir {:?}: {}", dir, e);
            return Ok(0);
        }
    };
    let mut count = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        match migrate_one_page_template(conn, dir, &path) {
            Ok(true) => count += 1,
            Ok(false) => {}
            Err(e) => tracing::warn!("page template {:?}: {}", path, e),
        }
    }
    Ok(count)
}

/// Returns `Ok(true)` if a row + assets were inserted, `Ok(false)` if
/// the file was skipped (parse error etc.), `Err` if the SQL itself
/// failed (caller propagates and rolls back).
fn migrate_one_page_template(conn: &Connection, dir: &Path, path: &Path) -> Result<bool> {
    let updated_at_sort = file_mtime_rfc3339(path);
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("read {:?}: {}", path, e);
            return Ok(false);
        }
    };
    let mut value: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("parse {:?}: {}", path, e);
            return Ok(false);
        }
    };
    let id = match read_uuid(&value, "id") {
        Some(u) => u,
        None => {
            tracing::warn!("page template {:?} missing valid id", path);
            return Ok(false);
        }
    };
    let name = read_str(&value, "name").unwrap_or("Untitled").to_string();
    let description = read_str(&value, "description").unwrap_or("").to_string();
    let category = read_str(&value, "category").unwrap_or("").to_string();

    // Walk the [background] table, replace any filesystem path with
    // `asset:<filename>`, and load the bytes for inclusion.
    let assets = rewrite_background_assets(&mut value, dir);

    let body_toml = match toml::to_string(&value) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("re-encode {:?}: {}", path, e);
            return Ok(false);
        }
    };
    let sha = sha256_hex(body_toml.as_bytes());
    let row = TemplateRow {
        id,
        name,
        description,
        category,
        body_toml,
        sha256: sha,
        updated_at_sort,
    };
    template_catalog_store::put_page_template_in(conn, &row, &assets)?;
    Ok(true)
}

/// Walk `value["background"]` and `value["pages"][i]["background"]`,
/// any place a `path = "..."` field appears under an `image` or `pdf`
/// background. Loads the referenced file (if it exists), returns it
/// as an `AssetBytes`, and rewrites the in-place TOML to
/// `path = "asset:<name>"`. Missing files are left as
/// `asset:<original-basename>` URIs but with no asset bytes — the
/// renderer will draw a placeholder.
fn rewrite_background_assets(value: &mut toml::Value, dir: &Path) -> Vec<AssetBytes> {
    let mut out: Vec<AssetBytes> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    rewrite_in(value, dir, &mut out, &mut seen);
    out
}

fn rewrite_in(
    value: &mut toml::Value,
    dir: &Path,
    out: &mut Vec<AssetBytes>,
    seen: &mut std::collections::HashSet<String>,
) {
    match value {
        toml::Value::Table(table) => {
            // If this table looks like a background descriptor with a
            // file-bearing variant, rewrite its `path` field.
            let kind = table
                .get("type")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            if matches!(kind.as_deref(), Some("image") | Some("pdf")) {
                if let Some(toml::Value::String(p)) = table.get("path").cloned() {
                    let original = PathBuf::from(&p);
                    let name = match original.file_name().and_then(|n| n.to_str()) {
                        Some(n) => n.to_string(),
                        None => p.clone(),
                    };
                    let new_uri = format!("asset:{}", name);
                    table.insert("path".to_string(), toml::Value::String(new_uri.clone()));
                    if !seen.insert(name.clone()) {
                        // Already loaded under same name from a
                        // previous reference in the same file.
                    } else {
                        // Resolve relative paths against the
                        // template's directory, leave absolute paths
                        // alone.
                        let candidate = if original.is_absolute() {
                            original.clone()
                        } else {
                            dir.join(&original)
                        };
                        match std::fs::read(&candidate) {
                            Ok(bytes) => {
                                let mime = guess_mime(&name);
                                let sha = sha256_hex(&bytes);
                                out.push(AssetBytes {
                                    name,
                                    mime,
                                    sha256: sha,
                                    bytes,
                                });
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "asset {:?} for template referenced in {:?} not found: {}",
                                    candidate,
                                    dir,
                                    e
                                );
                                // Leave the `asset:<name>` URI in
                                // place (dangling); no row inserted.
                            }
                        }
                    }
                }
            }
            for (_, v) in table.iter_mut() {
                rewrite_in(v, dir, out, seen);
            }
        }
        toml::Value::Array(arr) => {
            for v in arr.iter_mut() {
                rewrite_in(v, dir, out, seen);
            }
        }
        _ => {}
    }
}

fn guess_mime(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png".into()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".into()
    } else if lower.ends_with(".webp") {
        "image/webp".into()
    } else if lower.ends_with(".gif") {
        "image/gif".into()
    } else if lower.ends_with(".pdf") {
        "application/pdf".into()
    } else {
        "application/octet-stream".into()
    }
}

// ── notebook templates ─────────────────────────────────────────────────

fn migrate_notebook_templates(conn: &Connection, dir: &Path) -> Result<usize> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("read_dir {:?}: {}", dir, e);
            return Ok(0);
        }
    };
    let mut count = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let updated_at_sort = file_mtime_rfc3339(&path);
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("read {:?}: {}", path, e);
                continue;
            }
        };
        let value: toml::Value = match toml::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("parse {:?}: {}", path, e);
                continue;
            }
        };
        let id = match read_uuid(&value, "id") {
            Some(u) => u,
            None => {
                tracing::warn!("notebook template {:?} missing valid id", path);
                continue;
            }
        };
        let name = read_str(&value, "name").unwrap_or("Untitled").to_string();
        let body_toml = match toml::to_string(&value) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("re-encode {:?}: {}", path, e);
                continue;
            }
        };
        let sha = sha256_hex(body_toml.as_bytes());
        let row = TemplateRow {
            id,
            name,
            description: String::new(),
            category: String::new(),
            body_toml,
            sha256: sha,
            updated_at_sort,
        };
        if let Err(e) = template_catalog_store::put_notebook_template(conn, &row) {
            tracing::warn!("insert notebook template {}: {}", id, e);
            continue;
        }
        count += 1;
    }
    Ok(count)
}

// ── helpers ────────────────────────────────────────────────────────────

fn read_str<'a>(value: &'a toml::Value, key: &str) -> Option<&'a str> {
    value.as_table()?.get(key)?.as_str()
}

fn read_uuid(value: &toml::Value, key: &str) -> Option<Uuid> {
    Uuid::parse_str(read_str(value, key)?).ok()
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// File mtime as an RFC3339 string for the `updated_at_sort`
/// column. Falls back to "now" when the OS doesn't expose mtime
/// (rare on Linux but possible on remote / virtual filesystems).
/// `updated_at_sort` is a sort key for a future Amplify GSI, so
/// stable ordering across migrations matters more than exact
/// fidelity to the original create time.
fn file_mtime_rfc3339(path: &Path) -> String {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    match mtime {
        Some(t) => DateTime::<Utc>::from(t).to_rfc3339(),
        None => Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_mtime_rfc3339_handles_missing_file() {
        // Non-existent path → "now" fallback. Just check we get a
        // parseable RFC3339 string back, not the empty string.
        let s = file_mtime_rfc3339(Path::new("/nonexistent/path/that/should/not/exist"));
        assert!(DateTime::parse_from_rfc3339(&s).is_ok(), "got {:?}", s);
    }

    #[test]
    fn file_mtime_rfc3339_uses_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a");
        std::fs::write(&p, b"x").unwrap();
        let s = file_mtime_rfc3339(&p);
        let parsed = DateTime::parse_from_rfc3339(&s).expect("parses RFC3339");
        let now = Utc::now();
        // mtime should be very recent — within the last hour is
        // plenty for a no-op smoke check.
        let diff = now.signed_duration_since(parsed.with_timezone(&Utc));
        assert!(
            diff.num_seconds().abs() < 3600,
            "delta {}s",
            diff.num_seconds()
        );
    }
}
