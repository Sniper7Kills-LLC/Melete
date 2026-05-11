//! Per-tool brush-pipeline overrides.
//!
//! Tools (Pen / Pencil / Highlighter / Paintbrush / SprayCan / Calligraphy)
//! used to be hardcoded into `state::tool_brush_params`. This module turns
//! those four values (opacity multiplier, width multiplier, blend mode,
//! brush style) into a per-tool overridable struct that's persisted to
//! `~/.config/journal/config.toml` and editable via the tool-settings
//! dialog.
//!
//! `brush_style` is intentionally separate from the tool — that's how the
//! "Calligraphy → smooth render" toggle works: the user keeps the
//! Calligraphy tool selected (which still applies its width/opacity
//! multipliers and the per-stroke pressure mapping), but flips the
//! `brush_style` to `Pen` so the renderer dispatches to `draw_smooth`
//! instead of the polygon outline.

use std::collections::HashMap;

use melete_core::{BlendMode, ToolStyle};
use serde::{Deserialize, Serialize};

use crate::state::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToolSettings {
    pub opacity_mult: f32,
    pub width_mult: f64,
    pub blend_mode: BlendMode,
    pub brush_style: ToolStyle,
    /// Default base brush width (mm) the tool sets when selected. Lets
    /// the user keep e.g. Pen at 2mm and Highlighter at 20mm without
    /// having to touch the toolbar slider every switch. Default is 2.0
    /// for every tool to preserve historical behaviour; the
    /// `default_settings_for` table tunes each tool's sensible value.
    #[serde(default = "default_base_width_fallback")]
    pub default_base_width: f64,
}

fn default_base_width_fallback() -> f64 {
    2.0
}

/// A named brush preset — `name` is the user-visible label that appears
/// in the preset dropdown. `settings` is the ToolSettings snapshot the
/// preset applies when activated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NamedToolSettings {
    pub name: String,
    pub settings: ToolSettings,
}

/// Single source of truth for settable tools and their stable
/// config-key strings. Both `settable_tools()` and `tool_key()` derive
/// from this list, so the two can never drift. Adding a new settable
/// tool means appending one row here.
const SETTABLE: &[(Tool, &str)] = &[
    (Tool::Pen, "pen"),
    (Tool::Pencil, "pencil"),
    (Tool::Highlighter, "highlighter"),
    (Tool::Paintbrush, "paintbrush"),
    (Tool::SprayCan, "spraycan"),
    (Tool::Calligraphy, "calligraphy"),
];

/// Built-in defaults: every settable tool starts with a single preset
/// named "Default" matching its built-in `default_settings_for`. Users
/// add more presets via the Tool Options popup.
pub fn default_presets_map() -> std::collections::HashMap<String, Vec<NamedToolSettings>> {
    SETTABLE
        .iter()
        .map(|(t, k)| {
            (
                (*k).to_string(),
                vec![NamedToolSettings {
                    name: "Default".into(),
                    settings: default_settings_for(*t),
                }],
            )
        })
        .collect()
}

/// Built-in default active-preset map: every tool's active preset
/// starts at "Default".
pub fn default_active_preset_map() -> std::collections::HashMap<String, String> {
    SETTABLE
        .iter()
        .map(|(_, k)| ((*k).to_string(), "Default".to_string()))
        .collect()
}

/// Stable string key for each settable tool. Tools without settings
/// (Eraser, Selection) return `None` and are not surfaced in the UI.
pub fn tool_key(tool: Tool) -> Option<&'static str> {
    SETTABLE.iter().find_map(|(t, k)| (*t == tool).then_some(*k))
}

pub fn settable_tools() -> Vec<Tool> {
    SETTABLE.iter().map(|(t, _)| *t).collect()
}

/// Pretty label for a tool — used by the settings dialog.
pub fn tool_label(tool: Tool) -> &'static str {
    match tool {
        Tool::Pen => "Pen",
        Tool::Pencil => "Pencil",
        Tool::Highlighter => "Highlighter",
        Tool::Paintbrush => "Paintbrush",
        Tool::SprayCan => "Spray Can",
        Tool::Calligraphy => "Calligraphy",
        _ => "",
    }
}

/// Built-in defaults for each tool. Mirrors the original hardcoded values
/// in `state::tool_brush_params` so a fresh config behaves identically to
/// pre-customization builds.
pub fn default_settings_for(tool: Tool) -> ToolSettings {
    match tool {
        Tool::Pen => ToolSettings {
            opacity_mult: 1.0,
            width_mult: 1.0,
            blend_mode: BlendMode::Normal,
            brush_style: ToolStyle::Pen,
            default_base_width: 2.0,
        },
        Tool::Pencil => ToolSettings {
            opacity_mult: 0.85,
            width_mult: 1.0,
            blend_mode: BlendMode::Normal,
            brush_style: ToolStyle::Pencil,
            default_base_width: 1.2,
        },
        Tool::Highlighter => ToolSettings {
            opacity_mult: 0.35,
            width_mult: 1.0,
            blend_mode: BlendMode::Multiply,
            brush_style: ToolStyle::Highlighter,
            default_base_width: 20.0,
        },
        Tool::Paintbrush => ToolSettings {
            opacity_mult: 0.5,
            width_mult: 1.0,
            blend_mode: BlendMode::Normal,
            brush_style: ToolStyle::Paintbrush,
            default_base_width: 7.0,
        },
        Tool::SprayCan => ToolSettings {
            opacity_mult: 0.6,
            width_mult: 1.0,
            blend_mode: BlendMode::Normal,
            brush_style: ToolStyle::SprayCan,
            default_base_width: 10.0,
        },
        Tool::Calligraphy => ToolSettings {
            opacity_mult: 1.0,
            width_mult: 1.0,
            blend_mode: BlendMode::Normal,
            brush_style: ToolStyle::Calligraphy,
            default_base_width: 6.0,
        },
        _ => ToolSettings {
            opacity_mult: 1.0,
            width_mult: 1.0,
            blend_mode: BlendMode::Normal,
            brush_style: ToolStyle::Pen,
            default_base_width: 2.0,
        },
    }
}

/// Build the full default settings map (all settable tools mapped to
/// their built-in defaults). Used as the initial state when no user
/// overrides are present in the config.
pub fn default_settings_map() -> HashMap<String, ToolSettings> {
    SETTABLE
        .iter()
        .map(|(t, k)| ((*k).to_string(), default_settings_for(*t)))
        .collect()
}
