//! Persistent user-defined brush library.
//!
//! Stored in the SQLite catalog (`index.db::brushes`) via the
//! `BrushStore` trait. Each row carries one self-contained `Brush`
//! value (id + name + layers) serialized as TOML in `body_toml`.
//!
//! Built-in brushes live in `melete_canvas::built_in_brushes` and are
//! NOT stored here — the editor merges built-ins + user library at
//! display time. Saving a forked built-in writes a fresh UUID so the
//! original stays addressable.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use melete_canvas::built_in_brushes as bi;
use melete_core::Brush;
use melete_storage::{BrushRow, NotebookBackend};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Wire shape for one brush body: a `[[brushes]]`-keyed wrapper used
/// historically by `~/.config/journal/brushes.toml`. We continue to
/// emit / parse the same shape inside `BrushRow.body_toml` so the
/// catalog migration's existing parser keeps working.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrushLibraryFile {
    #[serde(default)]
    pub brushes: Vec<Brush>,
}

/// Load the user's brush library from the backend's brush catalog.
/// Backend or per-row decode errors are logged + skipped — a broken
/// row should not refuse to launch the app.
pub fn load(backend: &Rc<RefCell<dyn NotebookBackend>>) -> Vec<Brush> {
    let rows = match backend.borrow_mut().list_brushes() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("brush catalog list failed: {}", e);
            return Vec::new();
        }
    };
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        match decode_brush_row(&row) {
            Some(b) => out.push(b),
            None => tracing::warn!("brush row {} could not be decoded", row.id),
        }
    }
    out
}

fn decode_brush_row(row: &BrushRow) -> Option<Brush> {
    // Migration writes single-table TOML containing a `[[brushes]]`
    // entry, so the body is wrapped. New `save()`s also use the same
    // shape for round-trip stability.
    if let Ok(file) = toml::from_str::<BrushLibraryFile>(&row.body_toml) {
        if let Some(b) = file.brushes.into_iter().next() {
            return Some(b);
        }
    }
    // Fallback: body is a bare brush table.
    toml::from_str::<Brush>(&row.body_toml).ok()
}

fn encode_brush_body(brush: &Brush) -> Result<String, std::io::Error> {
    let file = BrushLibraryFile {
        brushes: vec![brush.clone()],
    };
    toml::to_string(&file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// All built-in compositions, freshly constructed with their default
/// per-style tuning. Used by `resolve_id` to look up an assignment.
/// (Built-ins aren't stored anywhere persistent because they're
/// derived from `ToolStyleParams::default()`.)
pub fn built_ins() -> Vec<Brush> {
    vec![
        bi::pen(0.6, 0.4),
        bi::pencil(0.4, 0.9, 0.12, 8.0, 0.22),
        bi::highlighter(0.6, 0.4),
        bi::paintbrush(1.6, 1.4, 0.95, 0.07, 0.20, 0.95),
        bi::spray(36, 0.06, 0.35),
        bi::calligraphy(45.0, 0.18, 0.5, true),
    ]
}

/// Look up a brush by id across built-ins + the user library. Used
/// to materialise per-tool brush assignments stored as just the id
/// in `config.toml::tool_brush_assignments`.
pub fn resolve_id(id: Uuid, user_library: &[Brush]) -> Option<Brush> {
    built_ins()
        .into_iter()
        .find(|b| b.id == id)
        .or_else(|| user_library.iter().find(|b| b.id == id).cloned())
}

/// Persist the brush library to the catalog. The given slice is the
/// new authoritative state — any catalog row whose id isn't in
/// `brushes` is deleted so deletions performed in memory propagate.
pub fn save(backend: &Rc<RefCell<dyn NotebookBackend>>, brushes: &[Brush]) -> std::io::Result<()> {
    let mut be = backend.borrow_mut();

    let existing: HashSet<Uuid> = match be.list_brushes() {
        Ok(rows) => rows.into_iter().map(|r| r.id).collect(),
        Err(e) => {
            tracing::warn!("brush catalog list during save: {}", e);
            HashSet::new()
        }
    };
    let kept: HashSet<Uuid> = brushes.iter().map(|b| b.id).collect();

    for brush in brushes {
        let body_toml = encode_brush_body(brush)?;
        let sha256 = sha256_hex(body_toml.as_bytes());
        let row = BrushRow {
            id: brush.id,
            name: brush.name.clone(),
            body_toml,
            sha256,
            updated_at_sort: String::new(), // backend stamps on insert
        };
        if let Err(e) = be.put_brush(&row) {
            return Err(std::io::Error::other(format!(
                "put_brush {}: {}",
                brush.id, e
            )));
        }
    }

    for id in existing.difference(&kept) {
        if let Err(e) = be.delete_brush(*id) {
            tracing::warn!("delete_brush {} during save: {}", id, e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use melete_core::{BrushLayer, ColorMod, Geometry, TipShape, WidthMode};
    use uuid::Uuid;

    fn sample_brush() -> Brush {
        Brush {
            id: Uuid::new_v4(),
            name: "Sample".into(),
            layers: vec![BrushLayer {
                enabled: true,
                geometry: Geometry::Smooth {
                    resample_step_mm: 1.0,
                },
                width: WidthMode::Pressure {
                    floor: 0.6,
                    amp: 0.4,
                },
                tip: TipShape::Round,
                tip_scale: 1.0,
                color: ColorMod::default(),
                blend: melete_core::BlendMode::Normal,
            }],
            cursor: melete_core::CursorShape::Auto,
            default_color: None,
        }
    }

    #[test]
    fn library_round_trip_via_toml() {
        let brushes = vec![sample_brush(), sample_brush()];
        let file = BrushLibraryFile {
            brushes: brushes.clone(),
        };
        let text = toml::to_string(&file).expect("encode");
        let decoded: BrushLibraryFile = toml::from_str(&text).expect("decode");
        assert_eq!(decoded.brushes, brushes);
    }

    #[test]
    fn empty_file_decodes_to_empty_vec() {
        let decoded: BrushLibraryFile = toml::from_str("").expect("empty decode");
        assert!(decoded.brushes.is_empty());
    }
}
