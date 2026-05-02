use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::calendar::PlannerPageAddress;
use crate::notebook::SectionId;
use crate::template::TemplateId;

/// Unique identifier for a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub Uuid);

/// A single page within a notebook section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    pub template_id: Option<TemplateId>,
    pub section_id: SectionId,
    pub position: u32,
    pub planner_address: Option<PlannerPageAddress>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}
