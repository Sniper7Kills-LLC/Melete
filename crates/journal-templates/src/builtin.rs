use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect, WidgetStyle};

pub const BUILTIN_BLANK_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000001");
pub const BUILTIN_DOTTED_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000002");
pub const BUILTIN_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000003");
pub const BUILTIN_GRID_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000004");
pub const BUILTIN_DAILY_PLANNER_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000005");
pub const BUILTIN_FULLFOCUS_DAILY_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000006");
pub const BUILTIN_FRANKLIN_DAILY_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000007");

const US_LETTER: (f64, f64) = (215.9, 279.4);

pub fn builtin_blank() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_BLANK_ID),
        name: "Blank".into(),
        description: "Plain page with no background pattern.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
    }
}

pub fn builtin_dotted() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_DOTTED_ID),
        name: "Dotted Grid".into(),
        description: "Light dotted grid for general note-taking (5mm spacing).".into(),
        background: BackgroundType::Dots { spacing: 5.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
    }
}

pub fn builtin_ruled() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_RULED_ID),
        name: "Ruled Lines".into(),
        description: "Ruled lines for prose (7mm spacing).".into(),
        background: BackgroundType::Lines { spacing: 7.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
    }
}

pub fn builtin_grid() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_GRID_ID),
        name: "Grid".into(),
        description: "Tiling grid that repeats infinitely (5mm base spacing).".into(),
        background: BackgroundType::Grid { spacing: 5.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
    }
}

pub fn builtin_daily_planner() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_DAILY_PLANNER_ID),
        name: "Daily Planner".into(),
        description: "Daily planner placeholder — ruled lines at 8mm spacing.".into(),
        background: BackgroundType::Lines { spacing: 8.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
    }
}

/// Full Focus Daily — US Letter, blank background.
/// Layout (all in mm within 215.9 × 279.4):
///   - BigThree: top third of page (full width, ~88mm tall)
///   - DailyAppointments 7–19: bottom-left two-thirds (left 60% of page, ~185mm tall)
///   - Checklist: bottom-right, aligned with appointments column (right 37%, ~185mm tall)
pub fn builtin_fullfocus_daily() -> PageTemplate {
    let margin = 8.0_f64;
    let page_w = US_LETTER.0;
    let page_h = US_LETTER.1;

    // BigThree occupies top ~30% of page (full width minus margins)
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

    // DailyAppointments occupies bottom-left ~60% width, remaining height
    let sched_top = margin + big_three_h + margin;
    let sched_h = page_h - sched_top - margin;
    let sched_w = (page_w - margin * 2.0) * 0.60;
    let appointments = TemplateWidget {
        id: Uuid::parse_str("a0000006-0002-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::DailyAppointments { start_hour: 7, end_hour: 19 },
        rect: WidgetRect {
            x: margin,
            y: sched_top,
            width: sched_w,
            height: sched_h,
        },
        style: WidgetStyle::default(),
    };

    // Checklist occupies bottom-right remaining width
    let checklist_x = margin + sched_w + margin;
    let checklist_w = page_w - checklist_x - margin;
    let checklist = TemplateWidget {
        id: Uuid::parse_str("a0000006-0003-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::Checklist { items: vec!["After-action review".into()] },
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
    }
}

/// Franklin Daily — US Letter, blank background.
/// Layout (all in mm within 215.9 × 279.4):
///   - TextBlock date header: full width at top (~12mm tall)
///   - PriorityList (14 rows): left half below header
///   - DailyAppointments 7–21: right half below header
pub fn builtin_franklin_daily() -> PageTemplate {
    let margin = 8.0_f64;
    let page_w = US_LETTER.0;
    let page_h = US_LETTER.1;

    // Date header at the top
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

    // PriorityList on the left half
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

    // DailyAppointments on the right half
    let appointments = TemplateWidget {
        id: Uuid::parse_str("a0000007-0003-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::DailyAppointments { start_hour: 7, end_hour: 21 },
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
    }
}

pub fn builtin_templates() -> Vec<PageTemplate> {
    vec![
        builtin_blank(),
        builtin_dotted(),
        builtin_ruled(),
        builtin_grid(),
        builtin_daily_planner(),
        builtin_fullfocus_daily(),
        builtin_franklin_daily(),
    ]
}
