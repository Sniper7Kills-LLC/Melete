//! Franklin Daily — date header, A/B/C priority list, hourly schedule
//! on US Letter.

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::US_LETTER;

pub const BUILTIN_FRANKLIN_DAILY_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000007");

pub fn builtin_franklin_daily() -> PageTemplate {
    let margin = 8.0_f64;
    let page_w = US_LETTER.0;
    let page_h = US_LETTER.1;

    let header_h = 14.0_f64;
    let header = TemplateWidget {
        id: Uuid::parse_str("a0000007-0001-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::TextBlock {
            text: "{weekday} {month_name} {day}, {year}".into(),
            font_size_mm: 8.0,
        },
        rect: WidgetRect {
            x: margin,
            y: margin,
            width: page_w - margin * 2.0,
            height: header_h,
        },
        style: WidgetStyle::default(),
    };

    let body_top = margin + header_h + margin;
    let body_h = page_h - body_top - margin;
    let half_w = (page_w - margin * 2.0 - margin) * 0.5;

    let priority = TemplateWidget {
        id: Uuid::parse_str("a0000007-0002-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::PriorityList { count: 14 },
        rect: WidgetRect {
            x: margin,
            y: body_top,
            width: half_w,
            height: body_h,
        },
        style: WidgetStyle::default(),
    };

    let appointments = TemplateWidget {
        id: Uuid::parse_str("a0000007-0003-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::DailyAppointments {
            start_hour: 7,
            end_hour: 21,
        },
        rect: WidgetRect {
            x: margin + half_w + margin,
            y: body_top,
            width: half_w,
            height: body_h,
        },
        style: WidgetStyle::default(),
    };

    PageTemplate {
        id: TemplateId(BUILTIN_FRANKLIN_DAILY_ID),
        name: "Franklin Daily".into(),
        description: "Franklin Planner-style daily page: date header, A/B/C priority list on the left, hourly schedule on the right.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: vec![header, priority, appointments],
        category: "Daily Planner".into(),
    }
}
