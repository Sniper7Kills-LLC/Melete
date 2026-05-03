use std::collections::HashMap;
use std::path::Path;

use journal_core::{PageTemplate, TemplateId};

use crate::builtin::builtin_templates;
use crate::error::TemplateError;
use crate::format::{parse_template_toml, template_file_to_page_template};

#[derive(Debug, Default, Clone)]
pub struct TemplateRegistry {
    templates: HashMap<TemplateId, PageTemplate>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        for t in builtin_templates() {
            r.insert(t);
        }
        r
    }

    pub fn insert(&mut self, t: PageTemplate) {
        self.templates.insert(t.id, t);
    }

    pub fn get(&self, id: TemplateId) -> Option<&PageTemplate> {
        self.templates.get(&id)
    }

    pub fn list(&self) -> Vec<&PageTemplate> {
        self.templates.values().collect()
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    pub fn load_dir(&mut self, dir: &Path) -> Result<usize, TemplateError> {
        let mut count = 0usize;
        let entries = std::fs::read_dir(dir)?;
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("failed to read directory entry in {:?}: {}", dir, e);
                    continue;
                }
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let contents = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("failed to read {:?}: {}", path, e);
                    continue;
                }
            };
            let parsed = match parse_template_toml(&contents) {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!("failed to parse template {:?}: {}", path, e);
                    continue;
                }
            };
            let template = template_file_to_page_template(parsed);
            self.insert(template);
            count += 1;
        }
        Ok(count)
    }
}
