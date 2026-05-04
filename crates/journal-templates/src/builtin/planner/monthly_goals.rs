//! Monthly Goals — calendar, top-12 goals, reflection space.

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::US_LETTER;

pub const BUILTIN_MONTHLY_GOALS_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000009");

pub fn builtin_monthly_goals() -> PageTemplate {
    let margin = 8.0_f64;
    let page_w = US_LETTER.0;
    let page_h = US_LETTER.1;

    let header_h = 12.0_f64;
    let header = TemplateWidget {
        id: Uuid::parse_str("a0000009-0001-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::TextBlock {
            text: "{month_name} {year}".into(),
            font_size_mm: 9.0,
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
    let upper_h = (page_h - body_top - margin) * 0.55;
    let lower_h = (page_h - body_top - margin) - upper_h - margin;
    let half_w = (page_w - margin * 3.0) * 0.5;

    let calendar = TemplateWidget {
        id: Uuid::parse_str("a0000009-0002-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::CalendarMonth,
        rect: WidgetRect {
            x: margin,
            y: body_top,
            width: half_w,
            height: upper_h,
        },
        style: WidgetStyle::default(),
    };

    let goals = TemplateWidget {
        id: Uuid::parse_str("a0000009-0003-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::PriorityList { count: 12 },
        rect: WidgetRect {
            x: margin + half_w + margin,
            y: body_top,
            width: half_w,
            height: upper_h,
        },
        style: WidgetStyle::default(),
    };

    let notes_label = TemplateWidget {
        id: Uuid::parse_str("a0000009-0004-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::TextBlock {
            text: "Reflection / Notes".into(),
            font_size_mm: 4.0,
        },
        rect: WidgetRect {
            x: margin,
            y: body_top + upper_h + margin,
            width: page_w - margin * 2.0,
            height: 6.0,
        },
        style: WidgetStyle::default(),
    };

    let notes = TemplateWidget {
        id: Uuid::parse_str("a0000009-0005-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::LinesRegion { spacing_mm: 8.0 },
        rect: WidgetRect {
            x: margin,
            y: body_top + upper_h + margin + 6.0,
            width: page_w - margin * 2.0,
            height: lower_h - 6.0,
        },
        style: WidgetStyle::default(),
    };

    PageTemplate {
        id: TemplateId(BUILTIN_MONTHLY_GOALS_ID),
        name: "Monthly Goals".into(),
        description: "Monthly overview with calendar, top-12 goals, and reflection space.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: vec![header, calendar, goals, notes_label, notes],
        category: "Monthly Planner".into(),
    }
}
