//! Full Focus Daily  -  Big Three priorities, hourly schedule, AAR
//! checklist on US Letter.
//!
//! Layout (within 215.9 x 279.4 mm):
//!   - BigThree: top ~30% of page (full width)
//!   - DailyAppointments 7-19: bottom-left ~60% width
//!   - Checklist: bottom-right column

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::US_LETTER;

pub const BUILTIN_FULLFOCUS_DAILY_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000006");

pub fn builtin_fullfocus_daily() -> PageTemplate {
    let margin = 8.0_f64;
    let page_w = US_LETTER.0;
    let page_h = US_LETTER.1;

    let big_three_h = page_h * 0.30;
    let big_three = TemplateWidget {
        id: Uuid::parse_str("a0000006-0001-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::BigThree,
        rect: WidgetRect {
            x: margin,
            y: margin,
            width: page_w - margin * 2.0,
            height: big_three_h,
        },
        style: WidgetStyle::default(),
    };

    let sched_top = margin + big_three_h + margin;
    let sched_h = page_h - sched_top - margin;
    let sched_w = (page_w - margin * 2.0) * 0.60;
    let appointments = TemplateWidget {
        id: Uuid::parse_str("a0000006-0002-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::DailyAppointments {
            start_hour: 7,
            end_hour: 19,
        },
        rect: WidgetRect {
            x: margin,
            y: sched_top,
            width: sched_w,
            height: sched_h,
        },
        style: WidgetStyle::default(),
    };

    let checklist_x = margin + sched_w + margin;
    let checklist_w = page_w - checklist_x - margin;
    let checklist = TemplateWidget {
        id: Uuid::parse_str("a0000006-0003-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::Checklist {
            items: vec!["After-action review".into()],
        },
        rect: WidgetRect {
            x: checklist_x,
            y: sched_top,
            width: checklist_w,
            height: sched_h,
        },
        style: WidgetStyle::default(),
    };

    PageTemplate {
        id: TemplateId(BUILTIN_FULLFOCUS_DAILY_ID),
        name: "Full Focus Daily".into(),
        description: "Full Focus Planner-style daily page: Big Three priorities, hourly schedule, and after-action checklist.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: vec![big_three, appointments, checklist],
        category: "Daily Planner".into(),
    }
}
