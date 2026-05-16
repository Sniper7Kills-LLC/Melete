//! Byte-equal round-trip preservation gate for `tools/seed-data/`.
//!
//! Each TOML in `tools/seed-data/page_templates/` is parsed and
//! re-serialized; the result must match the on-disk bytes exactly.
//! Each TOML in `tools/seed-data/notebook_templates/` is parsed as a
//! `NotebookTemplate` and re-serialized via `toml::to_string_pretty`;
//! the result must also match the on-disk bytes exactly.
//!
//! If this test fails after a `serialize_template_toml` change, the
//! seed bytes need to be re-extracted (run the ignored
//! `extract_seed_data` test) and re-checked into git.

use std::path::PathBuf;

use melete_core::{
    BackgroundType, Color, NotebookTemplate, PageTemplate, TemplateId, TemplateWidget, TilingMode,
    WidgetKind, WidgetRect, WidgetStyle,
};
use melete_templates::{
    parse_template_toml, serialize_template_toml, template_file_from_page_template,
    template_file_to_page_template,
};
use uuid::Uuid;

fn seed_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("tools");
    p.push("seed-data");
    p
}

#[test]
fn page_template_seeds_round_trip_byte_equal() {
    let dir = seed_root().join("page_templates");
    let mut count = 0usize;
    for entry in std::fs::read_dir(&dir).expect("read seed-data/page_templates") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let original = std::fs::read_to_string(&path).expect("read toml");
        let parsed = parse_template_toml(&original).expect("parse seed");
        let page = template_file_to_page_template(parsed);
        let file = template_file_from_page_template(&page);
        let reserialized = serialize_template_toml(&file).expect("serialize");
        assert_eq!(
            original,
            reserialized,
            "byte-equal round-trip failed for {}",
            path.display()
        );
        count += 1;
    }
    assert_eq!(count, 19, "expected 19 page-template seeds, found {count}");
}

#[test]
fn notebook_template_seeds_round_trip_byte_equal() {
    let dir = seed_root().join("notebook_templates");
    let mut count = 0usize;
    for entry in std::fs::read_dir(&dir).expect("read seed-data/notebook_templates") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let original = std::fs::read_to_string(&path).expect("read toml");
        let parsed: NotebookTemplate = toml::from_str(&original).expect("parse notebook seed");
        let reserialized = toml::to_string_pretty(&parsed).expect("serialize notebook");
        assert_eq!(
            original,
            reserialized,
            "byte-equal round-trip failed for {}",
            path.display()
        );
        count += 1;
    }
    assert_eq!(count, 1, "expected 1 notebook-template seed, found {count}");
}

// ---------------------------------------------------------------------------
// Chore Sheet (weekly) generator.
//
// Run once with `cargo test -p melete-templates -- --ignored
// extract_chore_weekly` to (re)write
// `tools/seed-data/page_templates/chore_weekly.toml` from the in-Rust
// definition below. The round-trip test above then guards the bytes.
// ---------------------------------------------------------------------------

fn wid_for(template_byte: u8, n: u16) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = 0xc0;
    bytes[1] = template_byte;
    bytes[2] = (n >> 8) as u8;
    bytes[3] = n as u8;
    Uuid::from_bytes(bytes)
}

fn wid(n: u16) -> Uuid {
    wid_for(0x1a, n)
}

fn wid_home(n: u16) -> Uuid {
    wid_for(0x1b, n)
}

fn wid_day(n: u16) -> Uuid {
    wid_for(0x1c, n)
}

fn default_style() -> WidgetStyle {
    WidgetStyle {
        stroke_color: Color {
            r: 60,
            g: 60,
            b: 80,
            a: 200,
        },
        fill_color: None,
        stroke_width_mm: 0.3,
    }
}

fn text_widget(id: Uuid, text: &str, font_size_mm: f64, x: f64, y: f64, w: f64, h: f64) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::TextBlock {
            text: text.into(),
            font_size_mm,
        },
        rect: WidgetRect { x, y, width: w, height: h },
        style: default_style(),
    }
}

fn checklist_widget(id: Uuid, items: &[&str], x: f64, y: f64, w: f64, h: f64) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Checklist {
            items: items.iter().map(|s| s.to_string()).collect(),
        },
        rect: WidgetRect { x, y, width: w, height: h },
        style: default_style(),
    }
}

fn build_chore_weekly() -> PageTemplate {
    use uuid::uuid;

    // Landscape US Letter.
    let page_w = 279.4_f64;
    let page_h = 215.9_f64;
    let margin = 8.0_f64;

    // Three columns: Daily Reset | Weekly Rotation | Monthly / As-Needed.
    let col_gap = 4.0_f64;
    let col_w = (page_w - margin * 2.0 - col_gap * 2.0) / 3.0;
    let col1_x = margin;
    let col2_x = margin + col_w + col_gap;
    let col3_x = margin + (col_w + col_gap) * 2.0;

    let title_h = 10.0_f64;
    let header_h = 5.0_f64;
    let body_top = margin + title_h + 4.0 + header_h + 2.0;
    let body_bottom_margin = 6.0_f64;
    let body_h = page_h - body_top - body_bottom_margin;

    let mut widgets = Vec::new();

    // ---- Title ---------------------------------------------------------
    widgets.push(text_widget(
        wid(1),
        "Chore Sheet — Week of {month_name} {day}, {year}",
        7.0,
        margin,
        margin,
        page_w - margin * 2.0,
        title_h,
    ));

    // ---- Column headers -----------------------------------------------
    let headers_y = margin + title_h + 4.0;
    widgets.push(text_widget(
        wid(2),
        "DAILY RESET  (10–15 min total)",
        4.0,
        col1_x,
        headers_y,
        col_w,
        header_h,
    ));
    widgets.push(text_widget(
        wid(3),
        "WEEKLY ROTATION  (one focus / day, ~20 min)",
        4.0,
        col2_x,
        headers_y,
        col_w,
        header_h,
    ));
    widgets.push(text_widget(
        wid(4),
        "MONTHLY / AS-NEEDED  (pick one this week)",
        4.0,
        col3_x,
        headers_y,
        col_w,
        header_h,
    ));

    // ---- Left column: Daily Reset checklist ---------------------------
    widgets.push(checklist_widget(
        wid(10),
        &[
            "Make the bed",
            "Open blinds, let in light",
            "5-min kitchen reset",
            "Dishes → dishwasher",
            "Wipe one bathroom surface",
            "Start / swap / fold 1 load laundry",
            "10-min hot-spot pickup",
            "Quick sweep entryway",
            "Trash + recycling check",
            "Set out tomorrow: clothes, bag, keys",
        ],
        col1_x,
        body_top,
        col_w,
        body_h,
    ));

    // ---- Middle column: 7 day-cards -----------------------------------
    let day_card_h = body_h / 7.0;
    let days: [(&str, &[&str]); 7] = [
        (
            "MON · BATHROOMS",
            &[
                "Toilet bowl + lid + seat",
                "Sink, mirror, fixtures",
                "Sweep + spot-mop floor",
            ],
        ),
        (
            "TUE · KITCHEN",
            &[
                "Wipe counters + stovetop",
                "Wipe inside microwave",
                "Take trash + recycling out",
            ],
        ),
        (
            "WED · BEDROOM + BEDDING",
            &[
                "Strip + wash sheets",
                "Wash pillowcases",
                "Dust nightstand + lamp",
            ],
        ),
        (
            "THU · FLOORS",
            &[
                "Vacuum carpets + rugs",
                "Mop entry + kitchen",
                "Shake out doormats",
            ],
        ),
        (
            "FRI · LAUNDRY + ERRANDS",
            &[
                "Wash + fold remaining laundry",
                "Groceries / prescriptions",
                "Put away clean clothes",
            ],
        ),
        (
            "SAT · LIVING SPACES",
            &[
                "Dust surfaces + electronics",
                "Vacuum couch + cushions",
                "Declutter one drawer / shelf",
            ],
        ),
        (
            "SUN · RESET + PLAN",
            &[
                "Meal prep / freezer check",
                "Look at next week",
                "Light tidy + restock supplies",
            ],
        ),
    ];
    for (i, (label, chores)) in days.iter().enumerate() {
        let y = body_top + day_card_h * i as f64;
        // Day label.
        widgets.push(text_widget(
            wid(20 + i as u16 * 2),
            label,
            3.5,
            col2_x,
            y,
            col_w,
            4.5,
        ));
        // Day chores checklist.
        widgets.push(checklist_widget(
            wid(21 + i as u16 * 2),
            chores,
            col2_x,
            y + 5.0,
            col_w,
            day_card_h - 6.0,
        ));
    }

    // ---- Right column: Monthly / As-Needed checklist ------------------
    widgets.push(checklist_widget(
        wid(50),
        &[
            "Wipe inside microwave",
            "Clean fridge shelves top→bottom",
            "Wash bath mats + hand towels",
            "Vacuum under couch cushions",
            "Dust ceiling fans + vents",
            "Descale shower head + faucets",
            "Wash windows / mirrors deep",
            "Change HVAC filter",
            "Clean coffee maker / kettle",
            "Wipe doorknobs + light switches",
            "Vacuum + flip mattress",
            "Check pantry expirations",
            "Wash trash cans",
            "Clean inside of car",
            "Restock first-aid + meds",
        ],
        col3_x,
        body_top,
        col_w,
        body_h,
    ));

    PageTemplate {
        id: TemplateId(uuid!("00000000-0000-0000-0000-00000000001a")),
        name: "Chore Sheet (weekly)".into(),
        description: "Landscape weekly chore sheet for ADHD-friendly home upkeep: a short Daily Reset, a one-room-a-day Weekly Rotation, and a pick-one Monthly / As-Needed column. Designed to spread housework across the week instead of one massive cleaning session.".into(),
        background: BackgroundType::Blank,
        size_mm: (279.4, 215.9),
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Weekly Planner".into(),
    }
}

#[test]
#[ignore = "regenerator: writes tools/seed-data/page_templates/chore_weekly.toml"]
fn extract_chore_weekly() {
    let t = build_chore_weekly();
    let file = template_file_from_page_template(&t);
    let toml_text = serialize_template_toml(&file).expect("serialize");
    let out = seed_root().join("page_templates").join("chore_weekly.toml");
    std::fs::write(&out, toml_text).expect("write seed file");
    eprintln!("wrote {}", out.display());
}

// ---------------------------------------------------------------------------
// Stay-at-Home weekly variant.
//
// For someone home most of the day (stay-at-home parent, unemployed,
// remote-flexible) — denser than the standard weekly: four time-block
// daily anchors instead of one Daily Reset, a deeper Weekly Rotation,
// plus Kids/Family + Self-Care stacks. Still ADHD-friendly: each block
// is short and self-contained, no marathon clean.
// ---------------------------------------------------------------------------

fn build_chore_weekly_home() -> PageTemplate {
    use uuid::uuid;

    let page_w = 279.4_f64;
    let page_h = 215.9_f64;
    let margin = 8.0_f64;

    let col_gap = 4.0_f64;
    let col_w = (page_w - margin * 2.0 - col_gap * 2.0) / 3.0;
    let col1_x = margin;
    let col2_x = margin + col_w + col_gap;
    let col3_x = margin + (col_w + col_gap) * 2.0;

    let title_h = 10.0_f64;
    let header_h = 5.0_f64;
    let body_top = margin + title_h + 4.0 + header_h + 2.0;
    let body_bottom_margin = 6.0_f64;
    let body_h = page_h - body_top - body_bottom_margin;

    let mut widgets = Vec::new();

    // ---- Title ---------------------------------------------------------
    widgets.push(text_widget(
        wid_home(1),
        "Stay-at-Home Chore Sheet — Week of {month_name} {day}, {year}",
        7.0,
        margin,
        margin,
        page_w - margin * 2.0,
        title_h,
    ));

    // ---- Column headers -----------------------------------------------
    let headers_y = margin + title_h + 4.0;
    widgets.push(text_widget(
        wid_home(2),
        "DAILY ANCHORS  (by time block)",
        4.0,
        col1_x,
        headers_y,
        col_w,
        header_h,
    ));
    widgets.push(text_widget(
        wid_home(3),
        "WEEKLY ROTATION  (one focus / day)",
        4.0,
        col2_x,
        headers_y,
        col_w,
        header_h,
    ));
    widgets.push(text_widget(
        wid_home(4),
        "KIDS / FAMILY  +  SELF-CARE",
        4.0,
        col3_x,
        headers_y,
        col_w,
        header_h,
    ));

    // ---- Left column: 4 time-block daily anchor mini-checklists -------
    let block_h = body_h / 4.0;
    let blocks: [(&str, &[&str]); 4] = [
        (
            "MORNING  (wake → coffee)",
            &[
                "Make the bed",
                "Open blinds, get dressed (not pajamas)",
                "Breakfast for self / kids",
                "Start 1 load of laundry",
                "5-min kitchen reset",
            ],
        ),
        (
            "MIDDAY  (nap / school hours)",
            &[
                "Swap laundry to dryer",
                "15-min focused chore (rotation)",
                "Lunch + dishes done",
                "Water plants / pets fed",
                "Sit + eat — phone down",
            ],
        ),
        (
            "AFTERNOON  (energy dip)",
            &[
                "Fold laundry (podcast / show OK)",
                "Tidy main living area",
                "Snack prep + drink water",
                "Prep tomorrow: bag, outfits",
                "10-min outside / fresh air",
            ],
        ),
        (
            "EVENING  (wind down)",
            &[
                "Kitchen wiped, dishes done",
                "10-min hot-spot pickup",
                "Bath / wind-down routine",
                "Set out tomorrow: clothes, keys",
                "Lights + locks check",
            ],
        ),
    ];
    for (i, (label, items)) in blocks.iter().enumerate() {
        let y = body_top + block_h * i as f64;
        widgets.push(text_widget(
            wid_home(10 + i as u16 * 2),
            label,
            3.5,
            col1_x,
            y,
            col_w,
            4.5,
        ));
        widgets.push(checklist_widget(
            wid_home(11 + i as u16 * 2),
            items,
            col1_x,
            y + 5.0,
            col_w,
            block_h - 6.0,
        ));
    }

    // ---- Middle column: 7 day-cards (deeper rotation) -----------------
    let day_card_h = body_h / 7.0;
    let days: [(&str, &[&str]); 7] = [
        (
            "MON · BATHROOMS",
            &[
                "Toilet bowl + lid + seat",
                "Sink, mirror, fixtures",
                "Tub / shower walls",
                "Mop floor, restock TP + soap",
            ],
        ),
        (
            "TUE · KITCHEN",
            &[
                "Counters + stovetop",
                "Wipe appliances + handles",
                "Clean sink + drain",
                "Sweep + mop, trash out",
            ],
        ),
        (
            "WED · BEDROOMS + BEDDING",
            &[
                "Strip + wash sheets",
                "Wash pillowcases + duvet cover",
                "Dust nightstands + dressers",
                "Vacuum bedroom floors",
            ],
        ),
        (
            "THU · FLOORS + ENTRIES",
            &[
                "Vacuum all rooms",
                "Mop hard floors",
                "Shake doormats, sweep porch",
                "Wipe baseboards (one room)",
            ],
        ),
        (
            "FRI · LAUNDRY + ERRANDS",
            &[
                "Catch up all laundry",
                "Fold + put away",
                "Groceries / prescriptions",
                "Meal-plan for next week",
            ],
        ),
        (
            "SAT · LIVING ROOM + EXTRAS",
            &[
                "Dust surfaces + electronics",
                "Vacuum couch + cushions",
                "Wipe windows / mirrors",
                "Declutter one zone",
            ],
        ),
        (
            "SUN · RESET + PREP",
            &[
                "Meal prep / freezer check",
                "Kid lunches + outfits for week",
                "Calendar + bills review",
                "Light tidy + restock supplies",
            ],
        ),
    ];
    for (i, (label, chores)) in days.iter().enumerate() {
        let y = body_top + day_card_h * i as f64;
        widgets.push(text_widget(
            wid_home(30 + i as u16 * 2),
            label,
            3.5,
            col2_x,
            y,
            col_w,
            4.5,
        ));
        widgets.push(checklist_widget(
            wid_home(31 + i as u16 * 2),
            chores,
            col2_x,
            y + 5.0,
            col_w,
            day_card_h - 6.0,
        ));
    }

    // ---- Right column: Kids/Family (top) + Self-Care (bottom) ---------
    let split = body_h * 0.55;
    widgets.push(text_widget(
        wid_home(60),
        "KIDS / FAMILY  (daily)",
        3.5,
        col3_x,
        body_top,
        col_w,
        4.5,
    ));
    widgets.push(checklist_widget(
        wid_home(61),
        &[
            "Pack / unpack school bags",
            "School lunches prepped",
            "Permission slips + forms",
            "Kid laundry sorted",
            "Bath / shower for kids",
            "Read together / homework",
            "Calendar check (appts, events)",
            "Tomorrow's outfits laid out",
            "Library books / returns",
            "Toys reset to bins",
        ],
        col3_x,
        body_top + 5.0,
        col_w,
        split - 5.0,
    ));
    widgets.push(text_widget(
        wid_home(70),
        "SELF-CARE  (don't skip — daily)",
        3.5,
        col3_x,
        body_top + split + 2.0,
        col_w,
        4.5,
    ));
    widgets.push(checklist_widget(
        wid_home(71),
        &[
            "10-min outside / walk",
            "Drink water (aim 8 cups)",
            "1 thing JUST for you",
            "Stretch / move 5 min",
            "Eat a real lunch",
            "Connect with an adult",
            "Sit + breathe 5 min",
            "Shower + dress (not pajamas)",
            "Phone-free meal",
            "Bed before midnight",
        ],
        col3_x,
        body_top + split + 7.0,
        col_w,
        body_h - split - 7.0,
    ));

    PageTemplate {
        id: TemplateId(uuid!("00000000-0000-0000-0000-00000000001b")),
        name: "Chore Sheet — Stay at Home (weekly)".into(),
        description: "Landscape weekly chore sheet tuned for stay-at-home parents and anyone home most of the day: morning/midday/afternoon/evening anchor routines, a one-room-a-day Weekly Rotation, and Kids/Family + Self-Care stacks so housework spreads across the week without burnout.".into(),
        background: BackgroundType::Blank,
        size_mm: (279.4, 215.9),
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Weekly Planner".into(),
    }
}

#[test]
#[ignore = "regenerator: writes tools/seed-data/page_templates/chore_weekly_home.toml"]
fn extract_chore_weekly_home() {
    let t = build_chore_weekly_home();
    let file = template_file_from_page_template(&t);
    let toml_text = serialize_template_toml(&file).expect("serialize");
    let out = seed_root().join("page_templates").join("chore_weekly_home.toml");
    std::fs::write(&out, toml_text).expect("write seed file");
    eprintln!("wrote {}", out.display());
}

// ---------------------------------------------------------------------------
// Single-day chore page.
//
// Portrait Letter. One sheet per day. Title auto-binds to the page's
// bound date. Daily anchor routines (morning / midday / afternoon /
// evening) plus a "Today's One-Room Focus" block + Quick Wins (5-min)
// + Self-Care + Tomorrow Setup. Drop into a notebook with one page per
// day and it works as a daily companion to the weekly sheets.
// ---------------------------------------------------------------------------

fn build_chore_daily() -> PageTemplate {
    use uuid::uuid;

    // Portrait US Letter.
    let page_w = 215.9_f64;
    let page_h = 279.4_f64;
    let margin = 8.0_f64;

    let col_gap = 4.0_f64;
    let col_w = (page_w - margin * 2.0 - col_gap) / 2.0;
    let col1_x = margin;
    let col2_x = margin + col_w + col_gap;

    let title_h = 12.0_f64;
    let header_h = 5.0_f64;
    let body_top = margin + title_h + 4.0;
    let body_bottom_margin = 6.0_f64;
    let body_h = page_h - body_top - body_bottom_margin;

    let mut widgets = Vec::new();

    // ---- Title (auto-date) --------------------------------------------
    widgets.push(text_widget(
        wid_day(1),
        "Chore Day — {weekday} {month_name} {day}, {year}",
        8.0,
        margin,
        margin,
        page_w - margin * 2.0,
        title_h,
    ));

    // ---- Left column: 4 time-block anchor mini-checklists -------------
    let blocks: [(&str, &[&str]); 4] = [
        (
            "MORNING",
            &[
                "Make the bed",
                "Get dressed (not pajamas)",
                "Breakfast + meds / vitamins",
                "Start 1 load of laundry",
                "5-min kitchen reset",
            ],
        ),
        (
            "MIDDAY",
            &[
                "Swap laundry to dryer",
                "Today's one-room focus (right →)",
                "Lunch + dishes done",
                "Water plants / pets fed",
            ],
        ),
        (
            "AFTERNOON",
            &[
                "Fold laundry (show / podcast OK)",
                "Tidy main living area",
                "10-min outside / fresh air",
                "Snack + water refill",
            ],
        ),
        (
            "EVENING",
            &[
                "Kitchen wiped, dishes done",
                "10-min hot-spot pickup",
                "Bath / wind-down routine",
                "Lights + locks check",
            ],
        ),
    ];
    let block_h = body_h / 4.0;
    for (i, (label, items)) in blocks.iter().enumerate() {
        let y = body_top + block_h * i as f64;
        widgets.push(text_widget(
            wid_day(10 + i as u16 * 2),
            label,
            4.0,
            col1_x,
            y,
            col_w,
            header_h,
        ));
        widgets.push(checklist_widget(
            wid_day(11 + i as u16 * 2),
            items,
            col1_x,
            y + header_h + 1.0,
            col_w,
            block_h - header_h - 2.0,
        ));
    }

    // ---- Right column: stacked utility blocks -------------------------
    // Sections: One-Room Focus, Quick Wins (5-min), Self-Care,
    // Tomorrow Setup.
    let right_h = body_h;
    let s_focus = right_h * 0.22;
    let s_quick = right_h * 0.28;
    let s_self = right_h * 0.28;
    let s_tomo = right_h - s_focus - s_quick - s_self;

    // Today's One-Room Focus (lines region under header)
    let mut y = body_top;
    widgets.push(text_widget(
        wid_day(30),
        "TODAY'S ONE-ROOM FOCUS",
        4.0,
        col2_x,
        y,
        col_w,
        header_h,
    ));
    widgets.push(TemplateWidget {
        id: wid_day(31),
        kind: WidgetKind::LinesRegion { spacing_mm: 7.0 },
        rect: WidgetRect {
            x: col2_x,
            y: y + header_h + 1.0,
            width: col_w,
            height: s_focus - header_h - 2.0,
        },
        style: default_style(),
    });
    y += s_focus;

    // Quick Wins (5-min)
    widgets.push(text_widget(
        wid_day(40),
        "QUICK WINS  (5-min each)",
        4.0,
        col2_x,
        y,
        col_w,
        header_h,
    ));
    widgets.push(checklist_widget(
        wid_day(41),
        &[
            "Wipe one surface (counter, table, sink)",
            "Clear one flat surface",
            "Empty one trash can",
            "Run / unload dishwasher",
            "Put 10 things away",
            "Sort today's mail",
        ],
        col2_x,
        y + header_h + 1.0,
        col_w,
        s_quick - header_h - 2.0,
    ));
    y += s_quick;

    // Self-Care
    widgets.push(text_widget(
        wid_day(50),
        "SELF-CARE  (non-negotiable)",
        4.0,
        col2_x,
        y,
        col_w,
        header_h,
    ));
    widgets.push(checklist_widget(
        wid_day(51),
        &[
            "Drink water (8 cups)",
            "Eat 3 meals",
            "Move 10 min",
            "Step outside",
            "5-min sit + breathe",
            "Bed before midnight",
        ],
        col2_x,
        y + header_h + 1.0,
        col_w,
        s_self - header_h - 2.0,
    ));
    y += s_self;

    // Tomorrow Setup
    widgets.push(text_widget(
        wid_day(60),
        "TOMORROW SETUP",
        4.0,
        col2_x,
        y,
        col_w,
        header_h,
    ));
    widgets.push(checklist_widget(
        wid_day(61),
        &[
            "Clothes laid out",
            "Bag / keys / wallet ready",
            "Coffee + breakfast plan",
            "Top 1 thing for tomorrow",
        ],
        col2_x,
        y + header_h + 1.0,
        col_w,
        s_tomo - header_h - 2.0,
    ));

    PageTemplate {
        id: TemplateId(uuid!("00000000-0000-0000-0000-00000000001c")),
        name: "Chore Day".into(),
        description: "Portrait single-day chore page: morning/midday/afternoon/evening anchor routines on the left, plus today's one-room focus, quick 5-min wins, self-care non-negotiables, and tomorrow setup on the right. ADHD-friendly daily companion to the weekly chore sheets.".into(),
        background: BackgroundType::Blank,
        size_mm: (215.9, 279.4),
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Daily Planner".into(),
    }
}

#[test]
#[ignore = "regenerator: writes tools/seed-data/page_templates/chore_daily.toml"]
fn extract_chore_daily() {
    let t = build_chore_daily();
    let file = template_file_from_page_template(&t);
    let toml_text = serialize_template_toml(&file).expect("serialize");
    let out = seed_root().join("page_templates").join("chore_daily.toml");
    std::fs::write(&out, toml_text).expect("write seed file");
    eprintln!("wrote {}", out.display());
}
