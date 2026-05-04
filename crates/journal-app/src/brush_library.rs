//! Persistent user-defined brush library.
//!
//! Stored as TOML at `~/.config/journal/brushes.toml` so it travels
//! beside `config.toml`. Each entry is a self-contained `Brush` value
//! (id + name + layers); the file is the source of truth for custom
//! brushes the user has saved from the Tool Editor.
//!
//! Built-in brushes live in `journal_canvas::built_in_brushes` and are
//! NOT stored here — the editor merges built-ins + user library at
//! display time. Saving a forked built-in writes a fresh UUID so the
//! original stays addressable.

use std::path::PathBuf;

use journal_core::Brush;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrushLibraryFile {
    #[serde(default)]
    pub brushes: Vec<Brush>,
}

fn library_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("journal").join("brushes.toml"))
}

/// Load the user's brush library. Missing file → empty list.
/// Parse failure → log + empty list (don't refuse to launch the app
/// because of a bad TOML edit).
pub fn load() -> Vec<Brush> {
    let Some(p) = library_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&p) else {
        return Vec::new();
    };
    match toml::from_str::<BrushLibraryFile>(&text) {
        Ok(f) => f.brushes,
        Err(e) => {
            tracing::warn!("brushes.toml parse failed: {}", e);
            Vec::new()
        }
    }
}

pub fn save(brushes: &[Brush]) -> std::io::Result<()> {
    let Some(p) = library_path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "config dir",
        ));
    };
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = BrushLibraryFile {
        brushes: brushes.to_vec(),
    };
    let text = toml::to_string(&file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(&p, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use journal_core::{BrushLayer, ColorMod, Geometry, TipShape, WidthMode};
    use uuid::Uuid;

    fn sample_brush() -> Brush {
        Brush {
            id: Uuid::new_v4(),
            name: "Sample".into(),
            layers: vec![BrushLayer {
                enabled: true,
                geometry: Geometry::Smooth { resample_step_mm: 1.0 },
                width: WidthMode::Pressure { floor: 0.6, amp: 0.4 },
                tip: TipShape::Round,
                color: ColorMod::default(),
                blend: journal_core::BlendMode::Normal,
            }],
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
