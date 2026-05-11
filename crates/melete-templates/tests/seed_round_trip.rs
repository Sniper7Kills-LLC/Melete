//! Byte-equal round-trip preservation gate for `tools/seed-data/`.
//!
//! Each TOML in `tools/seed-data/page_templates/` is parsed and
//! re-serialized; the result must match the on-disk bytes exactly.
//! Each TOML in `tools/seed-data/notebook_templates/` is parsed as a
//! `NotebookTemplate` and re-serialized via `toml::to_string_pretty`;
//! the result must also match the on-disk bytes exactly.
//!
//! If this test fails after a `serialize_template_toml` change, the
//! seed bytes need to be re-extracted (run the ignored
//! `extract_seed_data` test) and re-checked into git.

use std::path::PathBuf;

use melete_core::NotebookTemplate;
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
fn page_template_seeds_round_trip_byte_equal() {
    let dir = seed_root().join("page_templates");
    let mut count = 0usize;
    for entry in std::fs::read_dir(&dir).expect("read seed-data/page_templates") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let original = std::fs::read_to_string(&path).expect("read toml");
        let parsed = parse_template_toml(&original).expect("parse seed");
        let page = template_file_to_page_template(parsed);
        let file = template_file_from_page_template(&page);
        let reserialized = serialize_template_toml(&file).expect("serialize");
        assert_eq!(
            original,
            reserialized,
            "byte-equal round-trip failed for {}",
            path.display()
        );
        count += 1;
    }
    assert_eq!(count, 16, "expected 16 page-template seeds, found {count}");
}

#[test]
fn notebook_template_seeds_round_trip_byte_equal() {
    let dir = seed_root().join("notebook_templates");
    let mut count = 0usize;
    for entry in std::fs::read_dir(&dir).expect("read seed-data/notebook_templates") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let original = std::fs::read_to_string(&path).expect("read toml");
        let parsed: NotebookTemplate = toml::from_str(&original).expect("parse notebook seed");
        let reserialized = toml::to_string_pretty(&parsed).expect("serialize notebook");
        assert_eq!(
            original,
            reserialized,
            "byte-equal round-trip failed for {}",
            path.display()
        );
        count += 1;
    }
    assert_eq!(count, 1, "expected 1 notebook-template seed, found {count}");
}
