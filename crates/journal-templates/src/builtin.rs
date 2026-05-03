use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, Color, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect, WidgetStyle};

pub const BUILTIN_BLANK_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000001");
pub const BUILTIN_DOTTED_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000002");
pub const BUILTIN_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000003");
pub const BUILTIN_GRID_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000004");
pub const BUILTIN_DAILY_PLANNER_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000005");
pub const BUILTIN_FULLFOCUS_DAILY_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000006");
pub const BUILTIN_FRANKLIN_DAILY_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000007");
pub const BUILTIN_FRANKLIN_WEEKLY_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000008");
pub const BUILTIN_MONTHLY_GOALS_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000009");
pub const BUILTIN_QUARTERLY_REVIEW_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000a");
pub const BUILTIN_WIDE_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000b");
pub const BUILTIN_COLLEGE_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000c");
pub const BUILTIN_CORNELL_NOTES_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000d");
pub const BUILTIN_ISOMETRIC_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000e");
pub const BUILTIN_HEX_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000f");
pub const BUILTIN_ENGINEERING_GRAPH_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000010");
pub const BUILTIN_MUSIC_STAFF_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000011");

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
        category: "Paper".into(),
    }
}

pub fn builtin_dotted() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_DOTTED_ID),
        name: "Dotted Grid".into(),
        description: "Light dotted grid for general note-taking (5mm spacing).".into(),
        background: BackgroundType::Dots { spacing: 8.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
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
        category: "Paper".into(),
    }
}

pub fn builtin_grid() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_GRID_ID),
        name: "Grid".into(),
        description: "Tiling grid that repeats infinitely (5mm base spacing).".into(),
        background: BackgroundType::Grid { spacing: 10.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
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
        category: "Daily Planner".into(),
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
        category: "Daily Planner".into(),
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
        category: "Daily Planner".into(),
    }
}

/// Franklin Weekly — US Letter, blank background.
/// Layout (within 215.9 × 279.4 mm):
///   - TextBlock header at top: "Week {week} — {month_name} {year}" (12mm)
///   - WeeklyCompass on the left half of the body (roles + weekly goals)
///   - 7 LinesRegion blocks on the right half, one per day-of-week, each
///     prefixed with a tiny TextBlock day label.
pub fn builtin_franklin_weekly() -> PageTemplate {
    let margin = 8.0_f64;
    let page_w = US_LETTER.0;
    let page_h = US_LETTER.1;

    let header_h = 12.0_f64;
    let header = TemplateWidget {
        id: Uuid::parse_str("a0000008-0001-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::TextBlock {
            text: "Week {week} — {month_name} {year}".into(),
            font_size_mm: 7.0,
        },
        rect: WidgetRect { x: margin, y: margin, width: page_w - margin * 2.0, height: header_h },
        style: WidgetStyle::default(),
    };

    let body_top = margin + header_h + margin;
    let body_h = page_h - body_top - margin;
    let half_w = (page_w - margin * 3.0) * 0.5;

    let compass = TemplateWidget {
        id: Uuid::parse_str("a0000008-0002-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::WeeklyCompass,
        rect: WidgetRect { x: margin, y: body_top, width: half_w, height: body_h },
        style: WidgetStyle::default(),
    };

    let day_block_h = body_h / 7.0;
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut widgets = vec![header, compass];
    for (i, day) in days.iter().enumerate() {
        let y = body_top + day_block_h * i as f64;
        let label_h = 5.0_f64;
        // Day label
        widgets.push(TemplateWidget {
            id: Uuid::parse_str(&format!("a0000008-1{:03}-0000-0000-000000000000", i)).unwrap(),
            kind: WidgetKind::TextBlock { text: format!("{}", day), font_size_mm: 3.5 },
            rect: WidgetRect {
                x: margin + half_w + margin,
                y,
                width: half_w,
                height: label_h,
            },
            style: WidgetStyle::default(),
        });
        // Lines region underneath the label
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

/// Monthly Goals — US Letter, blank background.
/// Layout:
///   - TextBlock header: "{month_name} {year}" (12mm)
///   - CalendarMonth on the upper-left (~half page)
///   - PriorityList (12 rows) on the upper-right (monthly goals)
///   - LinesRegion across the bottom (monthly notes / reflection)
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
        rect: WidgetRect { x: margin, y: margin, width: page_w - margin * 2.0, height: header_h },
        style: WidgetStyle::default(),
    };

    let body_top = margin + header_h + margin;
    let upper_h = (page_h - body_top - margin) * 0.55;
    let lower_h = (page_h - body_top - margin) - upper_h - margin;
    let half_w = (page_w - margin * 3.0) * 0.5;

    let calendar = TemplateWidget {
        id: Uuid::parse_str("a0000009-0002-0000-0000-000000000000").unwrap(),
        kind: WidgetKind::CalendarMonth,
        rect: WidgetRect { x: margin, y: body_top, width: half_w, height: upper_h },
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
        kind: WidgetKind::TextBlock { text: "Reflection / Notes".into(), font_size_mm: 4.0 },
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

/// Quarterly Review — US Letter, blank background.
/// Layout:
///   - TextBlock header: "Quarterly Review — {year}"
///   - 3 stacked sections, one per month: month name (TextBlock) + LinesRegion
///   - "Wins / Lessons / Next quarter" priority list at the bottom
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
        rect: WidgetRect { x: margin, y: margin, width: page_w - margin * 2.0, height: header_h },
        style: WidgetStyle::default(),
    };

    let body_top = margin + header_h + margin;
    let body_h = page_h - body_top - margin;
    // 3 month strips on top take ~60% of remaining height; bottom 40% =
    // priority list for wins/lessons/next.
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

/// Wide-ruled notebook paper. Lines at 8.7mm (US "wide rule" = 11/32"),
/// red vertical margin line at ~32mm from the left edge, and a small
/// header strip at the top with a single line for date/title.
pub fn builtin_wide_ruled() -> PageTemplate {
    let red = Color { r: 200, g: 60, b: 70, a: 230 };
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
        description: "Wide-ruled notebook paper (8.7mm lines) with red margin and header strip.".into(),
        background: BackgroundType::Lines { spacing: 8.7 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: vec![red_margin, header_line],
        category: "Paper".into(),
    }
}

/// College-ruled notebook paper. Lines at 7.1mm (US "college rule" = 9/32")
/// with a red vertical margin line at ~32mm from the left edge.
pub fn builtin_college_ruled() -> PageTemplate {
    let red = Color { r: 200, g: 60, b: 70, a: 230 };
    let red_style = WidgetStyle {
        stroke_color: red,
        fill_color: None,
        stroke_width_mm: 0.4,
    };
    let margin_x = 32.0_f64;
    let header_h = 12.0_f64;

    let header_line = TemplateWidget {
        id: uuid!("a000000c-0001-0000-0000-000000000000"),
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
        id: uuid!("a000000c-0002-0000-0000-000000000000"),
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
        id: TemplateId(BUILTIN_COLLEGE_RULED_ID),
        name: "College Ruled Paper".into(),
        description: "College-ruled notebook paper (7.1mm lines) with red margin and header strip.".into(),
        background: BackgroundType::Lines { spacing: 7.1 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: vec![red_margin, header_line],
        category: "Paper".into(),
    }
}

/// Cornell Notes — left cue column (~58mm), main note area on the right,
/// and summary strip at the bottom (~50mm). Lines fill the note area.
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
        rect: WidgetRect { x: 6.0, y: 4.0, width: page_w - 12.0, height: header_h - 4.0 },
        style: WidgetStyle::default(),
    };
    let header_divider = TemplateWidget {
        id: uuid!("a000000d-0002-0000-0000-000000000000"),
        kind: WidgetKind::Line { thickness_mm: 0.4 },
        rect: WidgetRect { x: 0.0, y: header_h, width: page_w, height: 0.0 },
        style: WidgetStyle::default(),
    };
    let cue_divider = TemplateWidget {
        id: uuid!("a000000d-0003-0000-0000-000000000000"),
        kind: WidgetKind::Line { thickness_mm: 0.4 },
        rect: WidgetRect { x: cue_w, y: body_top, width: 0.0, height: body_h },
        style: WidgetStyle::default(),
    };
    let summary_divider = TemplateWidget {
        id: uuid!("a000000d-0004-0000-0000-000000000000"),
        kind: WidgetKind::Line { thickness_mm: 0.4 },
        rect: WidgetRect { x: 0.0, y: body_top + body_h, width: page_w, height: 0.0 },
        style: WidgetStyle::default(),
    };
    let cue_label = TemplateWidget {
        id: uuid!("a000000d-0005-0000-0000-000000000000"),
        kind: WidgetKind::TextBlock { text: "Cues / Questions".into(), font_size_mm: 3.5 },
        rect: WidgetRect { x: 4.0, y: body_top + 3.0, width: cue_w - 8.0, height: 5.0 },
        style: WidgetStyle::default(),
    };
    let notes_label = TemplateWidget {
        id: uuid!("a000000d-0006-0000-0000-000000000000"),
        kind: WidgetKind::TextBlock { text: "Notes".into(), font_size_mm: 3.5 },
        rect: WidgetRect { x: cue_w + 4.0, y: body_top + 3.0, width: 30.0, height: 5.0 },
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
        kind: WidgetKind::TextBlock { text: "Summary".into(), font_size_mm: 3.8 },
        rect: WidgetRect { x: 4.0, y: body_top + body_h + 3.0, width: 30.0, height: 5.0 },
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

/// Isometric grid paper. Tiles infinitely; great for technical sketches,
/// 3D drawings, and tabletop maps.
pub fn builtin_isometric() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_ISOMETRIC_ID),
        name: "Isometric Grid".into(),
        description: "Equilateral triangle isometric grid (15mm side length). Tiles infinitely.".into(),
        background: BackgroundType::Isometric { spacing: 15.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
    }
}

/// Hexagonal grid paper. Tiles infinitely; useful for hex-based games and
/// hex-grid notes.
pub fn builtin_hexagonal() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_HEX_ID),
        name: "Hex Grid".into(),
        description: "Pointy-top hexagonal grid (18mm centre spacing). Tiles infinitely.".into(),
        background: BackgroundType::Hexagonal { spacing: 18.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
    }
}

/// Engineering graph paper — fine grid at 2.54mm (10 squares per inch).
pub fn builtin_engineering_graph() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_ENGINEERING_GRAPH_ID),
        name: "Engineering Graph".into(),
        description: "Engineering graph paper at 5.08mm (5 squares per inch). Tiles infinitely.".into(),
        background: BackgroundType::Grid { spacing: 5.08 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
    }
}

/// Music staff paper — twelve 5-line staves stacked down the page.
pub fn builtin_music_staff() -> PageTemplate {
    let margin_x = 12.0_f64;
    let top_margin = 16.0_f64;
    let staff_count = 12_u32;
    let staff_line_spacing = 1.8_f64;
    let staff_height = staff_line_spacing * 4.0;
    let avail_h = US_LETTER.1 - top_margin - 16.0;
    let gap = (avail_h - staff_height * staff_count as f64) / (staff_count as f64 - 1.0).max(1.0);
    let mut widgets = Vec::with_capacity(staff_count as usize);
    for i in 0..staff_count {
        let y = top_margin + (staff_height + gap) * i as f64;
        widgets.push(TemplateWidget {
            id: Uuid::parse_str(&format!("a0000011-1{:03}-0000-0000-000000000000", i)).unwrap(),
            kind: WidgetKind::LinesRegion { spacing_mm: staff_line_spacing },
            rect: WidgetRect {
                x: margin_x,
                y,
                width: US_LETTER.0 - margin_x * 2.0,
                height: staff_height,
            },
            style: WidgetStyle::default(),
        });
    }
    PageTemplate {
        id: TemplateId(BUILTIN_MUSIC_STAFF_ID),
        name: "Music Staff".into(),
        description: "Twelve blank 5-line staves for music notation.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Note-taking".into(),
    }
}

pub fn builtin_templates() -> Vec<PageTemplate> {
    vec![
        builtin_blank(),
        builtin_dotted(),
        builtin_ruled(),
        builtin_grid(),
        builtin_wide_ruled(),
        builtin_college_ruled(),
        builtin_cornell_notes(),
        builtin_isometric(),
        builtin_hexagonal(),
        builtin_engineering_graph(),
        builtin_music_staff(),
        builtin_daily_planner(),
        builtin_fullfocus_daily(),
        builtin_franklin_daily(),
        builtin_franklin_weekly(),
        builtin_monthly_goals(),
        builtin_quarterly_review(),
    ]
}
