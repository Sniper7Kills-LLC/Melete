//! Cornell Notes — cue column, note area, summary band.

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::US_LETTER;

pub const BUILTIN_CORNELL_NOTES_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000d");

pub fn builtin_cornell_notes() -> PageTemplate {
    let cue_w = 58.0_f64;
    let summary_h = 50.0_f64;
    let header_h = 14.0_f64;
    let page_w = US_LETTER.0;
    let page_h = US_LETTER.1;
    let body_top = header_h;
    let body_h = page_h - header_h - summary_h;

    let header_label = TemplateWidget {
        id: uuid!("a000000d-0001-0000-0000-000000000000"),
        kind: WidgetKind::TextBlock {
            text: "Topic ____________   Date ____________".into(),
            font_size_mm: 4.0,
        },
        rect: WidgetRect {
            x: 6.0,
            y: 4.0,
            width: page_w - 12.0,
            height: header_h - 4.0,
        },
        style: WidgetStyle::default(),
    };
    let header_divider = TemplateWidget {
        id: uuid!("a000000d-0002-0000-0000-000000000000"),
        kind: WidgetKind::Line { thickness_mm: 0.4 },
        rect: WidgetRect {
            x: 0.0,
            y: header_h,
            width: page_w,
            height: 0.0,
        },
        style: WidgetStyle::default(),
    };
    let cue_divider = TemplateWidget {
        id: uuid!("a000000d-0003-0000-0000-000000000000"),
        kind: WidgetKind::Line { thickness_mm: 0.4 },
        rect: WidgetRect {
            x: cue_w,
            y: body_top,
            width: 0.0,
            height: body_h,
        },
        style: WidgetStyle::default(),
    };
    let summary_divider = TemplateWidget {
        id: uuid!("a000000d-0004-0000-0000-000000000000"),
        kind: WidgetKind::Line { thickness_mm: 0.4 },
        rect: WidgetRect {
            x: 0.0,
            y: body_top + body_h,
            width: page_w,
            height: 0.0,
        },
        style: WidgetStyle::default(),
    };
    let cue_label = TemplateWidget {
        id: uuid!("a000000d-0005-0000-0000-000000000000"),
        kind: WidgetKind::TextBlock {
            text: "Cues / Questions".into(),
            font_size_mm: 3.5,
        },
        rect: WidgetRect {
            x: 4.0,
            y: body_top + 3.0,
            width: cue_w - 8.0,
            height: 5.0,
        },
        style: WidgetStyle::default(),
    };
    let notes_label = TemplateWidget {
        id: uuid!("a000000d-0006-0000-0000-000000000000"),
        kind: WidgetKind::TextBlock {
            text: "Notes".into(),
            font_size_mm: 3.5,
        },
        rect: WidgetRect {
            x: cue_w + 4.0,
            y: body_top + 3.0,
            width: 30.0,
            height: 5.0,
        },
        style: WidgetStyle::default(),
    };
    let notes_lines = TemplateWidget {
        id: uuid!("a000000d-0007-0000-0000-000000000000"),
        kind: WidgetKind::LinesRegion { spacing_mm: 7.0 },
        rect: WidgetRect {
            x: cue_w + 2.0,
            y: body_top + 9.0,
            width: page_w - cue_w - 4.0,
            height: body_h - 11.0,
        },
        style: WidgetStyle::default(),
    };
    let summary_label = TemplateWidget {
        id: uuid!("a000000d-0008-0000-0000-000000000000"),
        kind: WidgetKind::TextBlock {
            text: "Summary".into(),
            font_size_mm: 3.8,
        },
        rect: WidgetRect {
            x: 4.0,
            y: body_top + body_h + 3.0,
            width: 30.0,
            height: 5.0,
        },
        style: WidgetStyle::default(),
    };
    let summary_lines = TemplateWidget {
        id: uuid!("a000000d-0009-0000-0000-000000000000"),
        kind: WidgetKind::LinesRegion { spacing_mm: 7.0 },
        rect: WidgetRect {
            x: 4.0,
            y: body_top + body_h + 9.0,
            width: page_w - 8.0,
            height: summary_h - 12.0,
        },
        style: WidgetStyle::default(),
    };

    PageTemplate {
        id: TemplateId(BUILTIN_CORNELL_NOTES_ID),
        name: "Cornell Notes".into(),
        description: "Cornell note-taking layout: cue column, note area, summary band.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: vec![
            header_label,
            header_divider,
            cue_divider,
            summary_divider,
            cue_label,
            notes_label,
            notes_lines,
            summary_label,
            summary_lines,
        ],
        category: "Note-taking".into(),
    }
}
