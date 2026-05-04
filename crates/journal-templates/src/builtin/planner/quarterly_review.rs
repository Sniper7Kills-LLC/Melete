//! Quarterly Review — month strips + wins/lessons/next list.

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::US_LETTER;

pub const BUILTIN_QUARTERLY_REVIEW_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000a");

pub fn builtin_quarterly_review() -> PageTemplate {
    let margin = 8.0_f64;
    let page_w = US_LETTER.0;
    let page_h = US_LETTER.1;

    let header_h = 14.0_f64;
    let header = TemplateWidget {
        id: Uuid::parse_str("a000000a-0001-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::TextBlock {
            text: "Quarterly Review — {year}".into(),
            font_size_mm: 10.0,
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
    let strips_total_h = body_h * 0.60;
    let bottom_h = body_h - strips_total_h - margin;
    let strip_h = strips_total_h / 3.0;
    let strip_label_h = 5.0_f64;

    let mut widgets = vec![header];
    for i in 0..3u32 {
        let y = body_top + strip_h * i as f64;
        widgets.push(TemplateWidget {
            id: Uuid::parse_str(&format!("a000000a-1{:03}-0000-0000-000000000000", i)).unwrap(),
            kind: WidgetKind::TextBlock {
                text: format!("Month {} — wins, decisions, blockers", i + 1),
                font_size_mm: 4.0,
            },
            rect: WidgetRect {
                x: margin,
                y,
                width: page_w - margin * 2.0,
                height: strip_label_h,
            },
            style: WidgetStyle::default(),
        });
        widgets.push(TemplateWidget {
            id: Uuid::parse_str(&format!("a000000a-2{:03}-0000-0000-000000000000", i)).unwrap(),
            kind: WidgetKind::LinesRegion { spacing_mm: 7.0 },
            rect: WidgetRect {
                x: margin,
                y: y + strip_label_h,
                width: page_w - margin * 2.0,
                height: strip_h - strip_label_h,
            },
            style: WidgetStyle::default(),
        });
    }

    widgets.push(TemplateWidget {
        id: Uuid::parse_str("a000000a-0002-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::TextBlock {
            text: "Wins · Lessons · Next quarter".into(),
            font_size_mm: 4.5,
        },
        rect: WidgetRect {
            x: margin,
            y: body_top + strips_total_h + margin,
            width: page_w - margin * 2.0,
            height: 6.0,
        },
        style: WidgetStyle::default(),
    });
    widgets.push(TemplateWidget {
        id: Uuid::parse_str("a000000a-0003-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::PriorityList { count: 9 },
        rect: WidgetRect {
            x: margin,
            y: body_top + strips_total_h + margin + 6.0,
            width: page_w - margin * 2.0,
            height: bottom_h - 6.0,
        },
        style: WidgetStyle::default(),
    });

    PageTemplate {
        id: TemplateId(BUILTIN_QUARTERLY_REVIEW_ID),
        name: "Quarterly Review".into(),
        description: "Per-month notes for the past quarter plus a 9-row wins/lessons/next list.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Quarterly Planner".into(),
    }
}
