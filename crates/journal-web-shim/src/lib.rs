//! `journal-web-shim` — pure-data WASM bindings for the page-template
//! designer's TOML round-trip.
//!
//! This crate is the in-browser counterpart to the
//! `journal_templates::format::{parse_template_toml, serialize_template_toml}`
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

use journal_core::PageTemplate;
use journal_templates::format::{
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

#[cfg(test)]
mod tests {
    // The crate's test suite lives outside wasm — `cargo test` (host
    // target) exercises the underlying `journal-templates` round-trip
    // directly. The wasm-bindgen-exposed entry points only serialize
    // through serde-wasm-bindgen, which has its own integration tests
    // upstream. Adding a duplicate harness here would require a
    // wasm-bindgen-test runner; not worth the wiring for a 60-line
    // adapter crate.
}
