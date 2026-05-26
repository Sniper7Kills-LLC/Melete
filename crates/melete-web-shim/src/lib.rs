//! `journal-web-shim` — pure-data WASM bindings for the page-template
//! designer's TOML round-trip.
//!
//! This crate is the in-browser counterpart to the
//! `melete_templates::format::{parse_template_toml, serialize_template_toml}`
//! pair. It accepts / emits the same `PageTemplate` JSON surface the
//! desktop's serde uses, so the SPA's `Shim` interface (see
//! `web/src/wasm/index.ts`) can shape designer state without re-implementing
//! the schema in TypeScript.
//!
//! Surface (extern via `wasm-bindgen`):
//!   - `parse_template_toml(toml: &str) -> Result<JsValue, JsValue>`
//!     Parses a TOML string and returns a `JsValue` matching the TS
//!     `PageTemplate` shape (`{ id, name, description, background,
//!     size_mm, tiling, default_viewport, widgets, category }`).
//!   - `serialize_template_toml(value: JsValue) -> Result<String, JsValue>`
//!     Inverse: takes the same JS object shape and emits TOML the
//!     desktop parser will accept byte-for-byte.
//!
//! `serde-wasm-bindgen` does the JS↔Rust conversion via serde, so any
//! `Serialize` / `Deserialize` implementors carry through. The
//! `Serializer::new().serialize_maps_as_objects(true)` config flips the
//! default Map-as-`Map` behaviour so `HashMap`-shaped fields land as
//! plain JS objects (closer to what the TS side expects).

#![allow(clippy::needless_pass_by_value)]

use melete_core::brush::Brush;
use melete_core::template::NotebookTemplate;
use melete_core::PageTemplate;
use melete_templates::format::{
    parse_template_toml as parse_toml, serialize_template_toml as serialize_toml,
    template_file_from_page_template, template_file_to_page_template,
};
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::prelude::*;

/// Wrap any error type as a JS-side `Error` so callers see the message
/// in the console / `.catch` handler instead of an opaque `RuntimeError`.
fn js_err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from(js_sys::Error::new(&e.to_string()))
}

/// Install the panic hook lazily (idempotent — `console_error_panic_hook::set_once`).
/// The web/build-wasm.sh script can opt in by enabling the
/// `panic_hook` feature; off by default so the wasm artefact stays
/// small for production.
#[cfg(feature = "panic_hook")]
fn install_panic_hook() {
    console_error_panic_hook::set_once();
}

#[cfg(not(feature = "panic_hook"))]
fn install_panic_hook() {}

/// Parse a TOML page-template into a JS object.
///
/// Returns the same shape the desktop's `serde_json::to_value(&PageTemplate)`
/// would emit — discriminator-tagged enums (`background.kind`,
/// `widgets[].kind.kind`), `size_mm` as a 2-tuple array, and so on. The
/// SPA's `web/src/types/index.ts` carries TS mirrors that line up with
/// this exact wire format.
#[wasm_bindgen]
pub fn parse_template_toml(toml: &str) -> Result<JsValue, JsValue> {
    install_panic_hook();
    let file = parse_toml(toml).map_err(js_err)?;
    let template = template_file_to_page_template(file);
    // serde-wasm-bindgen's default emits Maps for `HashMap` and tuples
    // as JS arrays. Both match what the TS side expects, but `serialize_maps_as_objects`
    // catches future widget metadata fields that might use a HashMap so
    // designers see plain `{}`-shaped values instead of `Map(…)`.
    let serializer = Serializer::new().serialize_maps_as_objects(true);
    template.serialize(&serializer).map_err(js_err)
}

/// Serialize a JS-side `PageTemplate` object back to TOML.
///
/// The input must satisfy the same JSON shape `parse_template_toml`
/// emits — the SPA's designer store keeps templates in exactly that
/// shape, so a round-trip through this fn produces the canonical TOML
/// string the desktop reads. `Result::Err` carries any deserialization
/// or TOML-encoding error verbatim.
#[wasm_bindgen]
pub fn serialize_template_toml(value: JsValue) -> Result<String, JsValue> {
    install_panic_hook();
    let template: PageTemplate = serde_wasm_bindgen::from_value(value).map_err(js_err)?;
    let file = template_file_from_page_template(&template);
    serialize_toml(&file).map_err(js_err)
}

// `serde::Serialize` is brought into scope for `template.serialize(...)` above.
use serde::Serialize;

// ---------------------------------------------------------------------
// Brush round-trip — backs the SPA's `/tooler` route.
// ---------------------------------------------------------------------

/// TOML wire format for a `Brush`. Wraps the `Brush` under a top-level
/// `[brush]` table so the file is self-describing (matches the
/// page-template wire format's `[template]` table). Adds a small
/// `[meta]` header carrying a schema version so older readers can
/// reject future formats explicitly.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BrushFile {
    #[serde(default)]
    meta: BrushFileMeta,
    brush: Brush,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BrushFileMeta {
    /// Bumped when the on-disk schema changes incompatibly.
    schema_version: u32,
}

impl Default for BrushFileMeta {
    fn default() -> Self {
        Self { schema_version: 1 }
    }
}

/// Parse a TOML brush document into a JS object.
///
/// Accepts either the wrapped `[brush]` form produced by
/// `serialize_brush_toml` or a bare `Brush` table at the root, so the
/// SPA can paste in either shape.
#[wasm_bindgen]
pub fn parse_brush_toml(toml: &str) -> Result<JsValue, JsValue> {
    install_panic_hook();
    // Try the wrapped form first; fall back to a bare Brush.
    let brush: Brush = match toml::from_str::<BrushFile>(toml) {
        Ok(file) => file.brush,
        Err(_) => toml::from_str::<Brush>(toml).map_err(js_err)?,
    };
    let serializer = Serializer::new().serialize_maps_as_objects(true);
    brush.serialize(&serializer).map_err(js_err)
}

/// Serialize a JS-side `Brush` object to TOML.
///
/// Output is the wrapped `[meta] / [brush]` form so a downstream parser
/// can reject older / newer schema versions explicitly.
#[wasm_bindgen]
pub fn serialize_brush_toml(value: JsValue) -> Result<String, JsValue> {
    install_panic_hook();
    let brush: Brush = serde_wasm_bindgen::from_value(value).map_err(js_err)?;
    let file = BrushFile {
        meta: BrushFileMeta::default(),
        brush,
    };
    toml::to_string_pretty(&file).map_err(js_err)
}

// ---------------------------------------------------------------------
// NotebookTemplate round-trip — backs the SPA's `/templeter` route.
// ---------------------------------------------------------------------

/// TOML wire format for a `NotebookTemplate`. Mirrors the
/// page-template + brush wrappers: small `[meta]` header carrying a
/// schema version, then the notebook template under a top-level
/// `[notebook_template]` table. Desktop reads the same shape via the
/// `app::template_io` path which currently does a bare
/// `toml::from_str::<NotebookTemplate>` — we accept either form here
/// so legacy bare files keep working.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct NotebookTemplateFile {
    #[serde(default)]
    meta: NotebookTemplateFileMeta,
    notebook_template: NotebookTemplate,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct NotebookTemplateFileMeta {
    schema_version: u32,
}

impl Default for NotebookTemplateFileMeta {
    fn default() -> Self {
        Self { schema_version: 1 }
    }
}

/// Parse a TOML notebook-template document into a JS object.
///
/// Accepts either the wrapped `[notebook_template]` form produced by
/// `serialize_notebook_template_toml` or a bare `NotebookTemplate`
/// table at the root.
#[wasm_bindgen]
pub fn parse_notebook_template_toml(toml: &str) -> Result<JsValue, JsValue> {
    install_panic_hook();
    let nt: NotebookTemplate = match toml::from_str::<NotebookTemplateFile>(toml) {
        Ok(file) => file.notebook_template,
        Err(_) => toml::from_str::<NotebookTemplate>(toml).map_err(js_err)?,
    };
    let serializer = Serializer::new().serialize_maps_as_objects(true);
    nt.serialize(&serializer).map_err(js_err)
}

/// Serialize a JS-side `NotebookTemplate` object to TOML.
#[wasm_bindgen]
pub fn serialize_notebook_template_toml(value: JsValue) -> Result<String, JsValue> {
    install_panic_hook();
    let nt: NotebookTemplate = serde_wasm_bindgen::from_value(value).map_err(js_err)?;
    let file = NotebookTemplateFile {
        meta: NotebookTemplateFileMeta::default(),
        notebook_template: nt,
    };
    toml::to_string_pretty(&file).map_err(js_err)
}

#[cfg(test)]
mod tests {
    // The crate's test suite lives outside wasm — `cargo test` (host
    // target) exercises the underlying `journal-templates` round-trip
    // directly. The wasm-bindgen-exposed entry points only serialize
    // through serde-wasm-bindgen, which has its own integration tests
    // upstream. Adding a duplicate harness here would require a
    // wasm-bindgen-test runner; not worth the wiring for a 60-line
    // adapter crate.

    use super::{BrushFile, BrushFileMeta};
    use melete_core::brush::{Brush, Geometry, TipShape, WidthMode};

    /// Round-trip a one-layer Brush through the BrushFile TOML wrapper
    /// to catch any serde derives going missing on the underlying enum
    /// types — we deserialize from a tagged-enum TOML form.
    #[test]
    fn brush_file_toml_roundtrip() {
        // Build a Brush JSON literal that exercises every internally
        // tagged enum (Geometry, WidthMode, TipShape, CursorShape) plus
        // ColorMod / BlendMode plain shapes — proves the schema.
        let json = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000000",
            "name": "test-brush",
            "layers": [{
                "enabled": true,
                "geometry": { "type": "smooth", "resample_step_mm": 0.5 },
                "width": { "type": "pressure", "floor": 0.2, "amp": 1.0 },
                "tip": { "type": "round" },
                "tip_scale": 1.0,
                "color": { "alpha_mult": 1.0, "hue_shift_deg": 0.0 },
                "blend": "Normal"
            }],
            "cursor": { "type": "auto" },
            "default_color": [60, 60, 80, 255]
        });
        let brush: Brush = serde_json::from_value(json).expect("brush from json");
        let file = BrushFile {
            meta: BrushFileMeta::default(),
            brush,
        };
        let s = toml::to_string_pretty(&file).expect("serialize");
        let parsed: BrushFile = toml::from_str(&s).expect("deserialize wrapped");
        assert_eq!(parsed.brush.name, "test-brush");
        assert_eq!(parsed.meta.schema_version, 1);
        assert_eq!(parsed.brush.layers.len(), 1);
        match parsed.brush.layers[0].geometry {
            Geometry::Smooth { resample_step_mm } => {
                assert!((resample_step_mm - 0.5).abs() < 1e-9);
            }
            _ => panic!("wrong geometry"),
        }
        match parsed.brush.layers[0].width {
            WidthMode::Pressure { floor, amp } => {
                assert!((floor - 0.2).abs() < 1e-9);
                assert!((amp - 1.0).abs() < 1e-9);
            }
            _ => panic!("wrong width"),
        }
        match parsed.brush.layers[0].tip {
            TipShape::Round => {}
            _ => panic!("wrong tip"),
        }
    }
}
