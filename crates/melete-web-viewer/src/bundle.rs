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
//! `journal-core`'s built-in `Serialize` impls emit). Differences:
//!   - `BackgroundType` and `NotebookKind` use internal `kind`-tagging
//!     in the JSON envelope, not serde's default external tagging.
//!     `JsonBackgroundType` / `JsonNotebookKind` below match the wire
//!     format and convert into the core types.
//!   - `PageTemplate` ids in the envelope are bare UUID strings, not
//!     the tuple-newtype `{ "0": uuid }` form serde emits by default.
//!     `JsonTemplateId` accepts either.
//!
//! Anything the viewer doesn't care about (planner addresses, widget
//! overrides, cached fetch payloads) lands in `serde_json::Value` so
//! unknown future-extensions don't fail deserialization.

#![allow(dead_code)]

use std::collections::HashMap;

use melete_core::{BackgroundType, NotebookKind, PageTemplate, Stroke, TemplateId, TilingMode, TemplateWidget, Viewport};
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
    /// Web envelope wraps the kind in an `{ "kind": "Standard" }`-style
    /// internal-tagged object, vs serde's external `"Standard"` /
    /// `{"Planner": {…}}` form. Capture the JSON verbatim so the viewer
    /// doesn't have to roundtrip through `NotebookKind` at all (it
    /// doesn't care about the variant).
    #[serde(default)]
    pub kind: serde_json::Value,
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

/// `PageTemplate` as the web envelope serializes it. Internally-tagged
/// `BackgroundType` is the main divergence from `melete_core::PageTemplate`'s
/// default serde output; we round-trip through this struct on parse.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonPageTemplate {
    pub id: JsonUuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub background: JsonBackgroundType,
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

/// Internally-tagged `BackgroundType` — the form the SPA's TS types
/// emit (`{ "kind": "Grid", "spacing": 5 }`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub enum JsonBackgroundType {
    Blank,
    Dots { spacing: f64 },
    Lines { spacing: f64 },
    Grid { spacing: f64 },
    Isometric { spacing: f64 },
    Hexagonal { spacing: f64 },
    Image { path: String },
    Pdf { path: String, page: u32 },
}

impl From<JsonBackgroundType> for BackgroundType {
    fn from(v: JsonBackgroundType) -> Self {
        match v {
            JsonBackgroundType::Blank => BackgroundType::Blank,
            JsonBackgroundType::Dots { spacing } => BackgroundType::Dots { spacing },
            JsonBackgroundType::Lines { spacing } => BackgroundType::Lines { spacing },
            JsonBackgroundType::Grid { spacing } => BackgroundType::Grid { spacing },
            JsonBackgroundType::Isometric { spacing } => BackgroundType::Isometric { spacing },
            JsonBackgroundType::Hexagonal { spacing } => BackgroundType::Hexagonal { spacing },
            JsonBackgroundType::Image { path } => BackgroundType::Image { path },
            JsonBackgroundType::Pdf { path, page } => BackgroundType::Pdf { path, page },
        }
    }
}

impl From<JsonPageTemplate> for PageTemplate {
    fn from(t: JsonPageTemplate) -> Self {
        PageTemplate {
            id: TemplateId(t.id.0),
            name: t.name,
            description: t.description,
            background: t.background.into(),
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
            serde_json::Value::String(s) => Uuid::parse_str(&s)
                .map_err(|e| serde::de::Error::custom(format!("uuid: {e}")))?,
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(s)) = map.get("0") {
                    Uuid::parse_str(s)
                        .map_err(|e| serde::de::Error::custom(format!("uuid: {e}")))?
                } else {
                    return Err(serde::de::Error::custom("expected uuid string or {0: uuid}"));
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

/// Helper for the `NotebookKind` enum — viewer doesn't introspect kind
/// today, but if a future caller wants it, parse the JSON's `kind`
/// field via this. Internal-tagged, mirror of `web/src/types/index.ts`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub enum JsonNotebookKind {
    Standard,
    Planner {
        template_id: JsonUuid,
        creation_date: String,
    },
}

impl From<JsonNotebookKind> for NotebookKind {
    fn from(k: JsonNotebookKind) -> Self {
        match k {
            JsonNotebookKind::Standard => NotebookKind::Standard,
            JsonNotebookKind::Planner {
                template_id,
                creation_date,
            } => NotebookKind::Planner {
                template_id: TemplateId(template_id.0),
                creation_date: chrono::NaiveDate::parse_from_str(&creation_date, "%Y-%m-%d")
                    .unwrap_or_else(|_| chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `web/public/sample-notebook.json` is the canonical envelope the
    /// SPA's `Viewer` page fetches. The bundle deserializer must accept
    /// its exact shape so the WASM viewer can ingest the same bytes
    /// that the TS-only mock viewer used to handle.
    const SAMPLE_NOTEBOOK_JSON: &str = include_str!(
        "../../../web/public/sample-notebook.json"
    );

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
