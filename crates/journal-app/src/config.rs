use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
