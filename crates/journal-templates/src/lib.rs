//! Template definitions, built-ins, and on-disk registry (Phase 3).

#![allow(clippy::too_many_arguments)]

pub mod builtin;
pub mod canvas_bridge;
pub mod error;
pub mod format;
pub mod notebook_template_builtin;
pub mod registry;
pub mod title_format;

pub use builtin::{
    builtin_templates, BUILTIN_BLANK_ID, BUILTIN_COLLEGE_RULED_ID, BUILTIN_CORNELL_NOTES_ID,
    BUILTIN_DAILY_PLANNER_ID, BUILTIN_DOTTED_ID, BUILTIN_ENGINEERING_GRAPH_ID,
    BUILTIN_FRANKLIN_DAILY_ID, BUILTIN_FRANKLIN_WEEKLY_ID, BUILTIN_FULLFOCUS_DAILY_ID,
    BUILTIN_GRID_ID, BUILTIN_HEX_ID, BUILTIN_ISOMETRIC_ID, BUILTIN_MILITARY_GOTWA_ID,
    BUILTIN_MILITARY_MEDEVAC_ID, BUILTIN_MILITARY_OPORD_ID, BUILTIN_MILITARY_PACE_ID,
    BUILTIN_MILITARY_PCC_PCI_ID, BUILTIN_MILITARY_RANGE_CARD_ID, BUILTIN_MILITARY_SALUTE_ID,
    BUILTIN_MILITARY_UXO_ID, BUILTIN_MONTHLY_GOALS_ID, BUILTIN_MUSIC_STAFF_ID,
    BUILTIN_QUARTERLY_REVIEW_ID, BUILTIN_RULED_ID, BUILTIN_WIDE_RULED_ID,
};
pub use notebook_template_builtin::{
    builtin_notebook_templates, builtin_yearly_planner, BUILTIN_YEARLY_PLANNER_ID,
};
pub use title_format::{render as render_title, TitleContext};

/// True if this template id matches one of the built-in template ids.
pub fn is_builtin(id: journal_core::TemplateId) -> bool {
    matches!(
        id.0,
        BUILTIN_BLANK_ID
            | BUILTIN_DOTTED_ID
            | BUILTIN_RULED_ID
            | BUILTIN_GRID_ID
            | BUILTIN_WIDE_RULED_ID
            | BUILTIN_COLLEGE_RULED_ID
            | BUILTIN_CORNELL_NOTES_ID
            | BUILTIN_ISOMETRIC_ID
            | BUILTIN_HEX_ID
            | BUILTIN_ENGINEERING_GRAPH_ID
            | BUILTIN_MUSIC_STAFF_ID
            | BUILTIN_DAILY_PLANNER_ID
            | BUILTIN_FULLFOCUS_DAILY_ID
            | BUILTIN_FRANKLIN_DAILY_ID
            | BUILTIN_FRANKLIN_WEEKLY_ID
            | BUILTIN_MONTHLY_GOALS_ID
            | BUILTIN_QUARTERLY_REVIEW_ID
            | BUILTIN_MILITARY_MEDEVAC_ID
            | BUILTIN_MILITARY_RANGE_CARD_ID
            | BUILTIN_MILITARY_OPORD_ID
            | BUILTIN_MILITARY_SALUTE_ID
            | BUILTIN_MILITARY_UXO_ID
            | BUILTIN_MILITARY_GOTWA_ID
            | BUILTIN_MILITARY_PACE_ID
            | BUILTIN_MILITARY_PCC_PCI_ID
    )
}

/// True if this notebook-template id matches one of the built-in
/// notebook-template ids.
pub fn is_builtin_notebook_template(id: journal_core::TemplateId) -> bool {
    matches!(id.0, BUILTIN_YEARLY_PLANNER_ID)
}
pub use canvas_bridge::page_template_to_background_config;
pub use error::TemplateError;
pub use format::{
    parse_template_toml, serialize_template_toml, template_file_from_page_template,
    template_file_to_page_template, TemplateFile,
};
pub use registry::{NotebookTemplateRegistry, TemplateRegistry};

#[cfg(test)]
mod tests {
    use super::*;
    use journal_core::BackgroundType;
    use tempfile::tempdir;

    #[test]
    fn builtins_non_empty() {
        let r = TemplateRegistry::with_builtins();
        assert!(!r.is_empty());
        assert!(r.list().iter().any(|t| t.name == "Blank"));
    }

    #[test]
    fn widget_round_trip() {
        use journal_core::{
            Color, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
            WidgetStyle,
        };
        use uuid::Uuid;
        let t = PageTemplate {
            id: TemplateId(Uuid::new_v4()),
            name: "With Widgets".into(),
            description: String::new(),
            background: BackgroundType::Blank,
            size_mm: (210.0, 297.0),
            tiling: TilingMode::None,
            default_viewport: None,
            widgets: vec![
                TemplateWidget {
                    id: Uuid::new_v4(),
                    kind: WidgetKind::GridRegion { spacing_mm: 5.0 },
                    rect: WidgetRect {
                        x: 10.0,
                        y: 20.0,
                        width: 100.0,
                        height: 80.0,
                    },
                    style: WidgetStyle::default(),
                },
                TemplateWidget {
                    id: Uuid::new_v4(),
                    kind: WidgetKind::CalendarMonth,
                    rect: WidgetRect {
                        x: 0.0,
                        y: 0.0,
                        width: 50.0,
                        height: 50.0,
                    },
                    style: WidgetStyle {
                        stroke_color: Color {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255,
                        },
                        fill_color: Some(Color {
                            r: 240,
                            g: 240,
                            b: 240,
                            a: 255,
                        }),
                        stroke_width_mm: 0.2,
                    },
                },
            ],
            category: String::new(),
        };
        let file = template_file_from_page_template(&t);
        let toml = serialize_template_toml(&file).expect("serialize");
        let parsed = parse_template_toml(&toml).expect("parse");
        let back = template_file_to_page_template(parsed);
        assert_eq!(back.widgets.len(), 2);
        assert!(
            matches!(&back.widgets[0].kind, WidgetKind::GridRegion { spacing_mm } if (*spacing_mm - 5.0).abs() < 1e-9)
        );
        assert!(matches!(&back.widgets[1].kind, WidgetKind::CalendarMonth));
    }

    #[test]
    fn old_template_without_widgets_parses() {
        let src = r#"
schema_version = 1
id = "00000000-0000-0000-0000-000000000099"
name = "Legacy"
[background]
type = "blank"
"#;
        let f = parse_template_toml(src).expect("parse");
        let t = template_file_to_page_template(f);
        assert_eq!(t.widgets.len(), 0);
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
        assert!(
            matches!(t.background, BackgroundType::Dots { spacing } if (spacing - 5.0).abs() < 1e-9)
        );
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
                (BackgroundType::Dots { spacing }, BackgroundConfig::Dots { spacing: s, .. }) => {
                    assert_eq!(*spacing, s);
                }
                (BackgroundType::Lines { spacing }, BackgroundConfig::Lines { spacing: s, .. }) => {
                    assert_eq!(*spacing, s);
                }
                (BackgroundType::Grid { spacing }, BackgroundConfig::Grid(g)) => {
                    assert_eq!(*spacing, g.base_spacing);
                }
                (
                    BackgroundType::Isometric { spacing },
                    BackgroundConfig::Isometric { spacing: s },
                ) => {
                    assert_eq!(*spacing, s);
                }
                (
                    BackgroundType::Hexagonal { spacing },
                    BackgroundConfig::Hexagonal { spacing: s },
                ) => {
                    assert_eq!(*spacing, s);
                }
                _ => panic!("unexpected mapping for {:?}", t.background),
            }
        }
    }

    #[test]
    fn canvas_bridge_maps_image() {
        use journal_canvas::BackgroundConfig;
        use journal_core::{PageTemplate, TemplateId, TilingMode};
        use uuid::Uuid;
        let t = PageTemplate {
            id: TemplateId(Uuid::new_v4()),
            name: "Img".into(),
            description: String::new(),
            background: BackgroundType::Image {
                path: "/tmp/x.png".into(),
            },
            size_mm: (200.0, 100.0),
            tiling: TilingMode::None,
            default_viewport: None,
            widgets: Vec::new(),
            category: String::new(),
        };
        match page_template_to_background_config(&t) {
            BackgroundConfig::Image { path, size_canvas } => {
                assert_eq!(path.to_string_lossy(), "/tmp/x.png");
                assert_eq!(size_canvas, (200.0, 100.0));
            }
            other => panic!("expected Image variant, got {:?}", other),
        }
    }

    #[test]
    fn canvas_bridge_pdf_maps_to_pdf_config() {
        use journal_canvas::BackgroundConfig;
        use journal_core::{PageTemplate, TemplateId, TilingMode};
        use uuid::Uuid;
        let t = PageTemplate {
            id: TemplateId(Uuid::new_v4()),
            name: "P".into(),
            description: String::new(),
            background: BackgroundType::Pdf {
                path: "/tmp/x.pdf".into(),
                page: 2,
            },
            size_mm: (215.9, 279.4),
            tiling: TilingMode::None,
            default_viewport: None,
            widgets: Vec::new(),
            category: String::new(),
        };
        match page_template_to_background_config(&t) {
            BackgroundConfig::Pdf {
                path,
                page,
                size_canvas,
            } => {
                assert_eq!(path.to_string_lossy(), "/tmp/x.pdf");
                assert_eq!(page, 2);
                assert_eq!(size_canvas, (215.9, 279.4));
            }
            other => panic!("expected Pdf variant, got {:?}", other),
        }
    }
}
