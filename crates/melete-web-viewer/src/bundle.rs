//! `NotebookBundle` envelope (Rust mirror).
//!
//! The exact shape the SPA POC is feeding via `loadNotebook` lives in
//! `web/public/sample-notebook.json` and is mirrored on the TS side by
//! `web/src/types/index.ts::NotebookBundle`. This module is the Rust
//! mirror — `serde_json::from_str` can deserialize the JSON straight
//! into these types and the viewer pulls the data it needs (pages,
//! per-page strokes, page templates) out of the result.
//!
//! Schema notes (the TS bundle is **not** byte-identical to what
//! `melete-core`'s built-in `Serialize` impls emit). Differences:
//!   - `PageTemplate` ids in the envelope are bare UUID strings, not
//!     the tuple-newtype `{ "0": uuid }` form serde emits by default.
//!     `JsonTemplateId` accepts either.
//!
//! `BackgroundType` and `NotebookKind` are now internally-tagged in
//! `melete-core` directly (matches the SPA's TS shape), so their
//! shims here were deleted.
//!
//! Anything the viewer doesn't care about (planner addresses, widget
//! overrides, cached fetch payloads) lands in `serde_json::Value` so
//! unknown future-extensions don't fail deserialization.

#![allow(dead_code)]

use std::collections::HashMap;

use melete_core::{
    BackgroundType, NotebookKind, PageTemplate, Stroke, TemplateId, TemplateWidget, TilingMode,
    Viewport,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct NotebookBundle {
    /// Schema version (1 today). The deserializer accepts any positive
    /// integer; the viewer warns and continues if a newer version is
    /// supplied with unrecognized fields, courtesy of `serde(default)`
    /// on the optional fields below.
    pub schema_version: u32,
    pub notebook: NotebookHeader,
    #[serde(default)]
    pub sections: Vec<SectionStub>,
    pub pages: Vec<PageStub>,
    pub page_templates: Vec<JsonPageTemplate>,
    /// Page id (UUID string) → list of strokes on that page.
    pub strokes_by_page: HashMap<String, Vec<Stroke>>,
    #[serde(default)]
    pub asset_refs: HashMap<String, String>,
}

impl NotebookBundle {
    /// Materialize the envelope's `JsonPageTemplate`s into the core
    /// `PageTemplate` form the renderer consumes. Cheap clone — each
    /// template is a few hundred bytes.
    pub fn page_templates_resolved(&self) -> Vec<PageTemplate> {
        self.page_templates
            .iter()
            .cloned()
            .map(PageTemplate::from)
            .collect()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotebookHeader {
    /// Either `{"0": "uuid"}` or a bare UUID string — accept both.
    pub id: JsonUuid,
    pub name: String,
    /// `NotebookKind` itself is internally-tagged in `melete-core` —
    /// JSON wire shape matches the SPA's TS `NotebookKind`, no shim
    /// needed. Optional + `default` because the viewer doesn't
    /// actually inspect the variant; a missing field deserialises to
    /// `None` and the viewer treats it as a plain notebook.
    #[serde(default)]
    pub kind: Option<NotebookKind>,
    #[serde(default)]
    pub assigned_templates: Vec<JsonUuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SectionStub {
    pub id: JsonUuid,
    pub notebook_id: JsonUuid,
    pub name: String,
    pub position: i64,
    #[serde(default)]
    pub allowed_templates: Option<Vec<JsonUuid>>,
    #[serde(default)]
    pub parent_section_id: Option<JsonUuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageStub {
    pub id: JsonUuid,
    /// Optional — pages can be blank.
    #[serde(default)]
    pub template_id: Option<JsonUuid>,
    pub section_id: JsonUuid,
    pub position: i64,
    pub name: String,
    /// Catch-all for fields the viewer doesn't inspect.
    #[serde(default)]
    pub planner_address: serde_json::Value,
    #[serde(default)]
    pub created_at: serde_json::Value,
    #[serde(default)]
    pub modified_at: serde_json::Value,
    #[serde(default)]
    pub widget_overrides: serde_json::Value,
    #[serde(default)]
    pub widget_data: serde_json::Value,
    #[serde(default)]
    pub flagged: bool,
}

/// `PageTemplate` as the web envelope serializes it. Holds the
/// internally-tagged `BackgroundType` (which now matches
/// `melete_core::BackgroundType`'s JSON shape after the
/// `#[serde(tag = "kind")]` unification — see the type-level docs on
/// `BackgroundType`) plus a few small id-shape conversions.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonPageTemplate {
    pub id: JsonUuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub background: BackgroundType,
    /// `(width, height)` in mm.
    pub size_mm: (f64, f64),
    #[serde(default)]
    pub tiling: JsonTilingMode,
    #[serde(default)]
    pub default_viewport: Option<Viewport>,
    #[serde(default)]
    pub widgets: Vec<TemplateWidget>,
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub enum JsonTilingMode {
    #[default]
    None,
    Repeat,
}

impl From<JsonTilingMode> for TilingMode {
    fn from(v: JsonTilingMode) -> Self {
        match v {
            JsonTilingMode::None => TilingMode::None,
            JsonTilingMode::Repeat => TilingMode::Repeat,
        }
    }
}

impl From<JsonPageTemplate> for PageTemplate {
    fn from(t: JsonPageTemplate) -> Self {
        PageTemplate {
            id: TemplateId(t.id.0),
            name: t.name,
            description: t.description,
            background: t.background,
            size_mm: t.size_mm,
            tiling: t.tiling.into(),
            default_viewport: t.default_viewport,
            widgets: t.widgets,
            category: t.category,
        }
    }
}

/// UUID newtype that accepts either:
///   - A bare UUID string: `"deadbeef-..."`
///   - A serde-default tuple-newtype object: `{"0": "deadbeef-..."}`
///
/// `journal_core`'s `NotebookId(pub Uuid)` / `TemplateId(pub Uuid)` /
/// etc all serialize as the bare string by default (transparent
/// newtype), so accepting both forms keeps us forward-compatible with
/// any future encoder that explicitly wraps.
#[derive(Debug, Clone, Copy)]
pub struct JsonUuid(pub Uuid);

impl<'de> Deserialize<'de> for JsonUuid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Use `Value` as the universal landing point — bare-string and
        // tuple-newtype both make sense as `Value`, and the dispatch
        // below covers each.
        let v = serde_json::Value::deserialize(deserializer)?;
        let uuid = match v {
            serde_json::Value::String(s) => {
                Uuid::parse_str(&s).map_err(|e| serde::de::Error::custom(format!("uuid: {e}")))?
            }
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(s)) = map.get("0") {
                    Uuid::parse_str(s)
                        .map_err(|e| serde::de::Error::custom(format!("uuid: {e}")))?
                } else {
                    return Err(serde::de::Error::custom(
                        "expected uuid string or {0: uuid}",
                    ));
                }
            }
            other => {
                return Err(serde::de::Error::custom(format!(
                    "expected uuid, got {other:?}"
                )));
            }
        };
        Ok(JsonUuid(uuid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `web/public/sample-notebook.json` is the canonical envelope the
    /// SPA's `Viewer` page fetches. The bundle deserializer must accept
    /// its exact shape so the WASM viewer can ingest the same bytes
    /// that the TS-only mock viewer used to handle.
    const SAMPLE_NOTEBOOK_JSON: &str = include_str!("../../../web/public/sample-notebook.json");

    #[test]
    fn parses_sample_notebook() {
        let bundle: NotebookBundle =
            serde_json::from_str(SAMPLE_NOTEBOOK_JSON).expect("parse sample-notebook.json");
        assert_eq!(bundle.schema_version, 1);
        assert_eq!(bundle.pages.len(), 2);
        assert_eq!(bundle.page_templates.len(), 1);
        // Two strokes on the first page.
        let first_page_id = bundle.pages[0].id.0.to_string();
        let strokes = bundle
            .strokes_by_page
            .get(&first_page_id)
            .expect("strokes for first page");
        assert_eq!(strokes.len(), 2);
    }

    #[test]
    fn page_templates_resolve_to_core_type() {
        let bundle: NotebookBundle =
            serde_json::from_str(SAMPLE_NOTEBOOK_JSON).expect("parse sample-notebook.json");
        let resolved = bundle.page_templates_resolved();
        assert_eq!(resolved.len(), 1);
        let t = &resolved[0];
        assert_eq!(t.name, "Daily Layout");
        assert!(matches!(t.background, BackgroundType::Blank));
        assert_eq!(t.widgets.len(), 3);
    }
}
