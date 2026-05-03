use std::collections::HashMap;
use std::path::Path;

use journal_core::{NotebookTemplate, PageTemplate, TemplateId};

use crate::builtin::builtin_templates;
use crate::error::TemplateError;
use crate::format::{parse_template_toml, template_file_to_page_template};
use crate::notebook_template_builtin::builtin_notebook_templates;

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

    pub fn remove(&mut self, id: TemplateId) -> Option<PageTemplate> {
        self.templates.remove(&id)
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

/// Registry of `NotebookTemplate`s. Built-in entries are seeded by
/// `with_builtins`; per-notebook clones (with overridden grouping or page
/// title format) get inserted at notebook creation time.
#[derive(Debug, Default, Clone)]
pub struct NotebookTemplateRegistry {
    templates: HashMap<TemplateId, NotebookTemplate>,
}

impl NotebookTemplateRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        for t in builtin_notebook_templates() {
            r.insert(t);
        }
        r
    }

    pub fn insert(&mut self, t: NotebookTemplate) {
        self.templates.insert(t.id, t);
    }

    pub fn remove(&mut self, id: TemplateId) -> Option<NotebookTemplate> {
        self.templates.remove(&id)
    }

    pub fn get(&self, id: TemplateId) -> Option<&NotebookTemplate> {
        self.templates.get(&id)
    }

    pub fn list(&self) -> Vec<&NotebookTemplate> {
        self.templates.values().collect()
    }

    pub fn load_dir(&mut self, dir: &Path) -> Result<usize, TemplateError> {
        let mut count = 0;
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(TemplateError::Io(e)),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!("read {:?} failed: {}", path, e);
                    continue;
                }
            };
            match toml::from_str::<NotebookTemplate>(&text) {
                Ok(t) => {
                    self.insert(t);
                    count += 1;
                }
                Err(e) => tracing::warn!("parse {:?} failed: {}", path, e),
            }
        }
        Ok(count)
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}
