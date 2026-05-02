use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::template::TemplateId;

/// Unique identifier for a notebook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NotebookId(pub Uuid);

/// Unique identifier for a section within a notebook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SectionId(pub Uuid);

/// Whether a notebook is a plain notebook or a planner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NotebookKind {
    Standard,
    Planner {
        template_id: TemplateId,
        creation_date: NaiveDate,
    },
}

/// A notebook that holds sections and pages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notebook {
    pub id: NotebookId,
    pub name: String,
    pub kind: NotebookKind,
    pub assigned_templates: Vec<TemplateId>,
}

/// A section within a notebook, grouping pages together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: SectionId,
    pub notebook_id: NotebookId,
    pub name: String,
    pub position: u32,
    pub allowed_templates: Option<Vec<TemplateId>>,
}
