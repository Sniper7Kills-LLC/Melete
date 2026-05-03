//! Built-in `NotebookTemplate`s. These describe the structure planner
//! notebooks generate (year sections, month/week wrappers, daily pages).

use chrono::Weekday;
use uuid::{uuid, Uuid};

use journal_core::{
    DailySlot, NotebookTemplate, PlannerGrouping, SectionTitleFormats, TemplateId,
};

use crate::builtin::BUILTIN_DAILY_PLANNER_ID;

pub const BUILTIN_YEARLY_PLANNER_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000101");

pub fn builtin_yearly_planner() -> NotebookTemplate {
    NotebookTemplate {
        id: TemplateId(BUILTIN_YEARLY_PLANNER_ID),
        name: "Yearly Planner".into(),
        description:
            "Year section per calendar year, month wrapper sections, one daily page per day."
                .into(),
        year_start: vec![],
        before_quarter: vec![],
        before_month: vec![],
        before_week: vec![],
        daily_slots: vec![DailySlot {
            days: vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat,
                Weekday::Sun,
            ],
            templates: vec![TemplateId(BUILTIN_DAILY_PLANNER_ID)],
        }],
        grouping: PlannerGrouping::Month,
        page_title_format: "{weekday} {month_name} {day}".into(),
        section_title_formats: SectionTitleFormats::default(),
        entry_options: std::collections::HashMap::new(),
    }
}

pub fn builtin_notebook_templates() -> Vec<NotebookTemplate> {
    vec![builtin_yearly_planner()]
}
