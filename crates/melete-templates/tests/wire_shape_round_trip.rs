//! JSON wire-shape parity gate (#13) — for every seed `PageTemplate`
//! TOML, detour through a `serde_json::Value` and back. This is the
//! same IR shape `serde-wasm-bindgen` emits to JavaScript in the web
//! shim, so any drift here would silently break the SPA designer's
//! TOML round-trip even when the desktop's byte-equal test keeps
//! passing.
//!
//! Companion to `seed_round_trip.rs`: that test gates desktop-side
//! TOML stability; this one gates JSON-IR stability for `melete-web-shim`.
//!
//! Failures here mean a serde derive on `PageTemplate` (or one of its
//! transitive enums) was reshaped — usually a tagged enum changed
//! discriminator names. Verify the SPA designer still loads templates
//! before bumping the goldens.

use std::path::PathBuf;

use melete_templates::{
    parse_template_toml, serialize_template_toml, template_file_from_page_template,
    template_file_to_page_template,
};

fn seed_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("tools");
    p.push("seed-data");
    p
}

#[test]
fn page_template_seeds_survive_json_ir_round_trip() {
    let dir = seed_root().join("page_templates");
    let mut count = 0usize;
    for entry in std::fs::read_dir(&dir).expect("read seed-data/page_templates") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let original = std::fs::read_to_string(&path).expect("read toml");

        // Desktop parse path.
        let file = parse_template_toml(&original).expect("parse seed toml");
        let template = template_file_to_page_template(file);

        // Detour through serde_json::Value — same IR shape the web
        // shim emits to JS via serde-wasm-bindgen.
        let value = serde_json::to_value(&template).expect("template -> json value");
        let template_from_json: melete_core::PageTemplate =
            serde_json::from_value(value).expect("json value -> template");

        // Re-serialize via the same path the desktop reader uses and
        // assert byte-equality with the on-disk seed.
        let file_back = template_file_from_page_template(&template_from_json);
        let reserialized = serialize_template_toml(&file_back).expect("serialize");
        assert_eq!(
            original,
            reserialized,
            "JSON IR round-trip drifted for {}",
            path.display()
        );
        count += 1;
    }
    assert!(count >= 1, "expected at least one page-template seed");
}

#[test]
fn page_template_json_ir_is_stable_under_self_round_trip() {
    let dir = seed_root().join("page_templates");
    for entry in std::fs::read_dir(&dir).expect("read seed-data/page_templates") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let original = std::fs::read_to_string(&path).expect("read toml");
        let file = parse_template_toml(&original).expect("parse");
        let template = template_file_to_page_template(file);
        let v1 = serde_json::to_value(&template).expect("v1");
        let template2: melete_core::PageTemplate =
            serde_json::from_value(v1.clone()).expect("v1 -> template");
        let v2 = serde_json::to_value(&template2).expect("v2");
        assert_eq!(v1, v2, "JSON IR not idempotent for {}", path.display());
    }
}
