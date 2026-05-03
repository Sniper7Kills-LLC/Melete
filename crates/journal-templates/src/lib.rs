//! Template definitions, built-ins, and on-disk registry (Phase 3).

pub mod builtin;
pub mod canvas_bridge;
pub mod error;
pub mod format;
pub mod registry;

pub use builtin::builtin_templates;
pub use canvas_bridge::page_template_to_background_config;
pub use error::TemplateError;
pub use format::{
    parse_template_toml, template_file_from_page_template, template_file_to_page_template,
    TemplateFile,
};
pub use registry::TemplateRegistry;

#[cfg(test)]
mod tests {
    use super::*;
    use journal_core::BackgroundType;
    use tempfile::tempdir;

    #[test]
    fn builtins_non_empty() {
        let r = TemplateRegistry::with_builtins();
        assert!(r.len() > 0);
        assert!(r.list().iter().any(|t| t.name == "Blank"));
    }

    #[test]
    fn parse_minimal_toml() {
        let src = r#"
schema_version = 1
id = "00000000-0000-0000-0000-000000000001"
name = "Dotted Grid"
description = "Light dotted grid."
size_mm = [215.9, 279.4]

[background]
type = "dots"
spacing = 5.0
"#;
        let f = parse_template_toml(src).expect("parse");
        let t = template_file_to_page_template(f);
        assert_eq!(t.name, "Dotted Grid");
        assert!(matches!(t.background, BackgroundType::Dots { spacing } if (spacing - 5.0).abs() < 1e-9));
    }

    #[test]
    fn round_trip_template() {
        for original in builtin_templates() {
            let file = template_file_from_page_template(&original);
            let toml_text = format::serialize_template_toml(&file).expect("serialize");
            let parsed = parse_template_toml(&toml_text).expect("parse");
            let back = template_file_to_page_template(parsed);
            assert_eq!(original, back, "round trip mismatch for {}", original.name);
        }
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let src = r#"
schema_version = 99
id = "00000000-0000-0000-0000-000000000001"
name = "Bad"
[background]
type = "blank"
"#;
        let err = parse_template_toml(src).unwrap_err();
        matches!(err, TemplateError::SchemaVersion(99));
    }

    #[test]
    fn load_dir_reads_toml_files() {
        let dir = tempdir().expect("tempdir");
        let toml_path = dir.path().join("hand.toml");
        std::fs::write(
            &toml_path,
            r#"
schema_version = 1
id = "11111111-1111-1111-1111-111111111111"
name = "Hand-written"
description = "Test template."
size_mm = [210.0, 297.0]

[background]
type = "grid"
spacing = 10.0
"#,
        )
        .unwrap();
        // Non-TOML file should be ignored.
        std::fs::write(dir.path().join("ignored.txt"), "nope").unwrap();

        let mut reg = TemplateRegistry::new();
        let n = reg.load_dir(dir.path()).expect("load");
        assert_eq!(n, 1);
        assert_eq!(reg.len(), 1);
        let t = reg.list()[0];
        assert_eq!(t.name, "Hand-written");
    }

    #[test]
    fn load_dir_skips_invalid_files() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("bad.toml"), "not a valid toml ===").unwrap();
        std::fs::write(
            dir.path().join("good.toml"),
            r#"
schema_version = 1
id = "22222222-2222-2222-2222-222222222222"
name = "Good"
[background]
type = "blank"
"#,
        )
        .unwrap();

        let mut reg = TemplateRegistry::new();
        let n = reg.load_dir(dir.path()).expect("load");
        assert_eq!(n, 1);
    }

    #[test]
    fn canvas_bridge_maps_variants() {
        use journal_canvas::BackgroundConfig;
        for t in builtin_templates() {
            let cfg = page_template_to_background_config(&t);
            match (&t.background, cfg) {
                (BackgroundType::Blank, BackgroundConfig::Blank) => {}
                (BackgroundType::Dots { spacing }, BackgroundConfig::Dots { spacing: s }) => {
                    assert_eq!(*spacing, s);
                }
                (BackgroundType::Lines { spacing }, BackgroundConfig::Lines { spacing: s }) => {
                    assert_eq!(*spacing, s);
                }
                (BackgroundType::Grid { spacing }, BackgroundConfig::Grid(g)) => {
                    assert_eq!(*spacing, g.base_spacing);
                }
                _ => panic!("unexpected mapping for {:?}", t.background),
            }
        }
    }
}
