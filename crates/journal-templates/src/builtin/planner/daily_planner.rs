//! Daily Planner  -  date header + Big Three + hourly schedule + tally
//! row + reflection lines.

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::{mw, US_LETTER};

pub const BUILTIN_DAILY_PLANNER_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000005");

pub fn builtin_daily_planner() -> PageTemplate {
    let t: u8 = 0x05;
    let margin = 8.0_f64;
    let (page_w, page_h) = US_LETTER;
    let mut widgets: Vec<TemplateWidget> = Vec::new();

    // Date header — uses {weekday} {month_name} {day}, {year}.
    let header_h = 12.0_f64;
    widgets.push(TemplateWidget {
        id: mw(t, 1),
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
    });

    let body_top = margin + header_h + margin;
    let big_three_h = 70.0_f64;
    let half_w = (page_w - margin * 2.0 - margin) * 0.5;

    // Big Three priorities — top-left.
    widgets.push(TemplateWidget {
        id: mw(t, 2),
        kind: WidgetKind::BigThree,
        rect: WidgetRect {
            x: margin,
            y: body_top,
            width: half_w,
            height: big_three_h,
        },
        style: WidgetStyle::default(),
    });

    // Tally row — water intake, top-right.
    let tally_h = 14.0_f64;
    widgets.push(TemplateWidget {
        id: mw(t, 3),
        kind: WidgetKind::Tally {
            label: "Water".into(),
            count: 8,
        },
        rect: WidgetRect {
            x: margin + half_w + margin,
            y: body_top,
            width: half_w,
            height: tally_h,
        },
        style: WidgetStyle::default(),
    });
    // Tally row — exercise minutes (or reps), below.
    widgets.push(TemplateWidget {
        id: mw(t, 4),
        kind: WidgetKind::Tally {
            label: "Move".into(),
            count: 6,
        },
        rect: WidgetRect {
            x: margin + half_w + margin,
            y: body_top + tally_h + 2.0,
            width: half_w,
            height: tally_h,
        },
        style: WidgetStyle::default(),
    });
    // Notes block under the tallies — same height as Big Three.
    let notes_top = body_top + tally_h * 2.0 + 4.0;
    let notes_h = big_three_h - (tally_h * 2.0 + 4.0);
    widgets.push(TemplateWidget {
        id: mw(t, 5),
        kind: WidgetKind::TextBlock {
            text: "Reflection".into(),
            font_size_mm: 4.0,
        },
        rect: WidgetRect {
            x: margin + half_w + margin,
            y: notes_top,
            width: half_w,
            height: 5.5,
        },
        style: WidgetStyle::default(),
    });
    widgets.push(TemplateWidget {
        id: mw(t, 6),
        kind: WidgetKind::LinesRegion { spacing_mm: 7.0 },
        rect: WidgetRect {
            x: margin + half_w + margin,
            y: notes_top + 5.5,
            width: half_w,
            height: notes_h - 5.5,
        },
        style: WidgetStyle::default(),
    });

    // Daily Appointments — full width below.
    let sched_top = body_top + big_three_h + margin;
    let sched_h = page_h - sched_top - margin;
    widgets.push(TemplateWidget {
        id: mw(t, 7),
        kind: WidgetKind::DailyAppointments {
            start_hour: 6,
            end_hour: 22,
        },
        rect: WidgetRect {
            x: margin,
            y: sched_top,
            width: page_w - margin * 2.0,
            height: sched_h,
        },
        style: WidgetStyle::default(),
    });

    PageTemplate {
        id: TemplateId(BUILTIN_DAILY_PLANNER_ID),
        name: "Daily Planner".into(),
        description: "Daily planner: date header, Big Three priorities, water + move tally rows, reflection lines, and a 6-22h appointment schedule.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Daily Planner".into(),
    }
}
