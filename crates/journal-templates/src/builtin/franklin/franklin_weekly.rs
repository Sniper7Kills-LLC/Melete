//! Franklin Weekly  -  weekly compass on the left, day-by-day notes on
//! the right.

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::US_LETTER;

pub const BUILTIN_FRANKLIN_WEEKLY_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000008");

pub fn builtin_franklin_weekly() -> PageTemplate {
    let margin = 8.0_f64;
    let page_w = US_LETTER.0;
    let page_h = US_LETTER.1;

    let header_h = 12.0_f64;
    let header = TemplateWidget {
        id: Uuid::parse_str("a0000008-0001-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::TextBlock {
            text: "Week {week}  -  {month_name} {year}".into(),
            font_size_mm: 7.0,
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
    let half_w = (page_w - margin * 3.0) * 0.5;

    let compass = TemplateWidget {
        id: Uuid::parse_str("a0000008-0002-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::WeeklyCompass,
        rect: WidgetRect {
            x: margin,
            y: body_top,
            width: half_w,
            height: body_h,
        },
        style: WidgetStyle::default(),
    };

    let day_block_h = body_h / 7.0;
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut widgets = vec![header, compass];
    for (i, day) in days.iter().enumerate() {
        let y = body_top + day_block_h * i as f64;
        let label_h = 5.0_f64;
        widgets.push(TemplateWidget {
            id: Uuid::parse_str(&format!("a0000008-1{:03}-0000-0000-000000000000", i)).unwrap(),
            kind: WidgetKind::TextBlock {
                text: day.to_string(),
                font_size_mm: 3.5,
            },
            rect: WidgetRect {
                x: margin + half_w + margin,
                y,
                width: half_w,
                height: label_h,
            },
            style: WidgetStyle::default(),
        });
        widgets.push(TemplateWidget {
            id: Uuid::parse_str(&format!("a0000008-2{:03}-0000-0000-000000000000", i)).unwrap(),
            kind: WidgetKind::LinesRegion { spacing_mm: 6.0 },
            rect: WidgetRect {
                x: margin + half_w + margin,
                y: y + label_h,
                width: half_w,
                height: day_block_h - label_h,
            },
            style: WidgetStyle::default(),
        });
    }

    PageTemplate {
        id: TemplateId(BUILTIN_FRANKLIN_WEEKLY_ID),
        name: "Franklin Weekly".into(),
        description: "Franklin Planner-style weekly spread: weekly compass on the left, day-by-day notes on the right.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Weekly Planner".into(),
    }
}
