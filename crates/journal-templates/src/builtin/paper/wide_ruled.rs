//! Wide-ruled notebook paper  -  8.7mm lines + red margin + header strip.

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, Color, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind,
    WidgetRect, WidgetStyle,
};

use crate::builtin::US_LETTER;

pub const BUILTIN_WIDE_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000b");

pub fn builtin_wide_ruled() -> PageTemplate {
    let red = Color {
        r: 200,
        g: 60,
        b: 70,
        a: 230,
    };
    let red_style = WidgetStyle {
        stroke_color: red,
        fill_color: None,
        stroke_width_mm: 0.4,
    };
    let margin_x = 32.0_f64;
    let header_h = 14.0_f64;

    let header_line = TemplateWidget {
        id: uuid!("a000000b-0001-0000-0000-000000000000"),
        kind: WidgetKind::Line { thickness_mm: 0.3 },
        rect: WidgetRect {
            x: margin_x + 4.0,
            y: header_h,
            width: US_LETTER.0 - margin_x - 8.0,
            height: 0.0,
        },
        style: WidgetStyle::default(),
    };
    let red_margin = TemplateWidget {
        id: uuid!("a000000b-0002-0000-0000-000000000000"),
        kind: WidgetKind::Line { thickness_mm: 0.4 },
        rect: WidgetRect {
            x: margin_x,
            y: 0.0,
            width: 0.0,
            height: US_LETTER.1,
        },
        style: red_style,
    };

    PageTemplate {
        id: TemplateId(BUILTIN_WIDE_RULED_ID),
        name: "Wide Ruled Paper".into(),
        description: "Wide-ruled notebook paper (8.7mm lines) with red margin and header strip."
            .into(),
        background: BackgroundType::Lines { spacing: 8.7 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: vec![red_margin, header_line],
        category: "Paper".into(),
    }
}
