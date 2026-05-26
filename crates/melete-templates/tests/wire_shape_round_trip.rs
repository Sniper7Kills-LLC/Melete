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

#[test]
fn notebook_template_seeds_survive_json_ir_round_trip() {
    let dir = seed_root().join("notebook_templates");
    for entry in std::fs::read_dir(&dir).expect("read seed-data/notebook_templates") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let original = std::fs::read_to_string(&path).expect("read toml");
        let nt: melete_core::NotebookTemplate =
            toml::from_str(&original).expect("parse notebook seed");
        // JSON IR detour — the same shape `melete-web-shim` emits to
        // the SPA's `/templeter` route via `parse_notebook_template_toml`.
        let v1 = serde_json::to_value(&nt).expect("nt -> json");
        let nt_back: melete_core::NotebookTemplate =
            serde_json::from_value(v1.clone()).expect("json -> nt");
        let v2 = serde_json::to_value(&nt_back).expect("nt_back -> json");
        assert_eq!(v1, v2, "JSON IR not idempotent for {}", path.display());

        let reserialized = toml::to_string_pretty(&nt_back).expect("re-serialize");
        assert_eq!(
            original,
            reserialized,
            "JSON IR round-trip drifted for {}",
            path.display()
        );
    }
}

#[test]
fn brush_json_ir_round_trips_through_json() {
    // No checked-in brush TOML seeds yet — exercise the schema directly
    // with a multi-layer brush that touches every internally-tagged
    // enum (Geometry, WidthMode, TipShape, CursorShape) plus ColorMod /
    // BlendMode plain shapes. This is the same surface the SPA's
    // /tooler route detours through.
    use melete_core::brush::Brush;
    let json = serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000000",
        "name": "wire-shape-fixture",
        "layers": [
            {
                "enabled": true,
                "geometry": { "type": "smooth", "resample_step_mm": 0.5 },
                "width": { "type": "pressure", "floor": 0.2, "amp": 1.0 },
                "tip": { "type": "round" },
                "tip_scale": 1.0,
                "color": { "alpha_mult": 1.0, "hue_shift_deg": 0.0 },
                "blend": "Normal"
            }
        ],
        "cursor": { "type": "auto" },
        "default_color": [60, 60, 80, 255]
    });
    let brush: Brush = serde_json::from_value(json.clone()).expect("brush from json");
    let v_back = serde_json::to_value(&brush).expect("brush to json");
    let brush2: Brush = serde_json::from_value(v_back.clone()).expect("brush from v_back");
    let v2 = serde_json::to_value(&brush2).expect("brush2 to json");
    assert_eq!(v_back, v2, "brush JSON IR not idempotent");
}
