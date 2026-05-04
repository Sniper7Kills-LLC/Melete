use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PenPreset {
    pub name: String,
    pub color_rgba: [u8; 4],
    pub width_mm: f64,
}

fn default_pen_presets() -> Vec<PenPreset> {
    vec![
        PenPreset { name: "Black fine".into(), color_rgba: [20, 20, 20, 255], width_mm: 1.5 },
        PenPreset { name: "Blue".into(),       color_rgba: [30, 90, 200, 255], width_mm: 2.0 },
        PenPreset { name: "Red".into(),        color_rgba: [200, 50, 50, 255], width_mm: 2.0 },
        PenPreset { name: "Marker".into(),     color_rgba: [20, 20, 20, 255], width_mm: 4.0 },
    ]
}

fn default_color_slots() -> Vec<[u8; 4]> {
    vec![
        [20, 20, 20, 255],
        [200, 50, 50, 255],
        [30, 90, 200, 255],
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub placeholder_image_path: Option<PathBuf>,
    #[serde(default)]
    pub placeholder_text: Option<String>,
    #[serde(default)]
    pub window_width: Option<i32>,
    #[serde(default)]
    pub window_height: Option<i32>,
    #[serde(default)]
    pub recent_notebook_ids: Vec<uuid::Uuid>,
    #[serde(default = "default_true")]
    pub show_page_bounds: bool,
    #[serde(default)]
    pub toolbar_x: Option<i32>,
    #[serde(default)]
    pub toolbar_y: Option<i32>,
    #[serde(default)]
    pub toolbar_collapsed: bool,
    #[serde(default = "default_pen_presets")]
    pub pen_presets: Vec<PenPreset>,
    #[serde(default = "default_color_slots")]
    pub color_slots: Vec<[u8; 4]>,
    /// Developer-mode toggle. Off by default — gates UI surfaces that
    /// shouldn't be available to typical users (e.g. the per-tool
    /// brush-settings dialog). Either flip this to `true` in
    /// `~/.config/journal/config.toml` or set the `JOURNAL_DEV=1`
    /// environment variable to enable.
    #[serde(default)]
    pub developer_mode: bool,
    /// Per-tool brush-pipeline overrides. Empty when the user hasn't
    /// changed anything; defaults are filled in at load time. Kept as
    /// a flat single-active-snapshot for backward compat — the richer
    /// `tool_presets` field replaces it for users who want named
    /// presets.
    #[serde(default)]
    pub tool_settings: std::collections::HashMap<String, crate::tool_settings::ToolSettings>,
    /// Named brush presets per tool. Each tool has its own list of
    /// `(name, ToolSettings)` pairs the user can switch between via the
    /// Tool Options popup. The active preset for each tool is recorded
    /// in `active_tool_preset`.
    #[serde(default)]
    pub tool_presets:
        std::collections::HashMap<String, Vec<crate::tool_settings::NamedToolSettings>>,
    /// Currently-active preset name per tool key. When a tool is
    /// selected, its active preset's `ToolSettings` is copied into
    /// `tool_settings[key]` so the renderer keeps reading the same
    /// flat map.
    #[serde(default)]
    pub active_tool_preset: std::collections::HashMap<String, String>,
    /// True = dock the Tool Options panel to the right side of the
    /// canvas instead of showing it as a floating window.
    #[serde(default)]
    pub tool_options_docked: bool,
    /// Per-tool color palette — quick-pick swatches saved per drawing
    /// tool. Keyed by tool key ("pen", "pencil", …). Each value is the
    /// raw RGBA8 of one swatch. Empty by default; the user fills via
    /// the Tool Options popup.
    #[serde(default)]
    pub tool_palettes: std::collections::HashMap<String, Vec<[u8; 4]>>,
    /// Global per-brush-style internal tuning parameters. `None` means
    /// "use defaults" — populated when the user changes anything via
    /// the developer-mode tool settings dialog.
    #[serde(default)]
    pub brush_params: Option<journal_canvas::vello_renderer::BrushParams>,
}

/// True when developer-only UI (e.g. the per-tool brush settings dialog)
/// should be exposed. Combines the persisted config flag with a
/// `JOURNAL_DEV=1` environment opt-in for one-off debugging sessions.
pub fn developer_mode_enabled(cfg: &AppConfig) -> bool {
    if cfg.developer_mode {
        return true;
    }
    matches!(
        std::env::var("JOURNAL_DEV").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            placeholder_image_path: None,
            placeholder_text: None,
            window_width: None,
            window_height: None,
            recent_notebook_ids: Vec::new(),
            show_page_bounds: true,
            toolbar_x: None,
            toolbar_y: None,
            toolbar_collapsed: false,
            pen_presets: default_pen_presets(),
            color_slots: default_color_slots(),
            developer_mode: false,
            tool_settings: std::collections::HashMap::new(),
            tool_presets: std::collections::HashMap::new(),
            active_tool_preset: std::collections::HashMap::new(),
            tool_options_docked: false,
            tool_palettes: std::collections::HashMap::new(),
            brush_params: None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("journal").join("config.toml"))
}

pub fn load() -> AppConfig {
    let Some(p) = config_path() else { return AppConfig::default(); };
    let Ok(text) = std::fs::read_to_string(&p) else { return AppConfig::default(); };
    match toml::from_str(&text) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!("config parse failed: {}", e);
            AppConfig::default()
        }
    }
}

pub fn save(cfg: &AppConfig) -> std::io::Result<()> {
    let Some(p) = config_path() else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "config dir"));
    };
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(&p, text)
}
