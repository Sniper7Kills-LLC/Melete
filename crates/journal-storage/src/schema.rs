use rusqlite::Connection;

use crate::error::Result;

pub const CREATE_NOTEBOOKS: &str = "
CREATE TABLE IF NOT EXISTS notebooks (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    planner_template_id BLOB,
    planner_creation_date TEXT,
    created_at TEXT NOT NULL,
    assigned_templates_json TEXT NOT NULL
);";

pub const CREATE_SECTIONS: &str = "
CREATE TABLE IF NOT EXISTS sections (
    id BLOB PRIMARY KEY NOT NULL,
    notebook_id BLOB NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    position INTEGER NOT NULL,
    allowed_templates_json TEXT,
    parent_section_id BLOB NULL REFERENCES sections(id) ON DELETE CASCADE
);";

pub const CREATE_PAGES: &str = "
CREATE TABLE IF NOT EXISTS pages (
    id BLOB PRIMARY KEY NOT NULL,
    section_id BLOB NOT NULL REFERENCES sections(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    template_id BLOB,
    planner_address_json TEXT,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL
);";

pub const CREATE_STROKES: &str = "
CREATE TABLE IF NOT EXISTS strokes (
    id BLOB PRIMARY KEY NOT NULL,
    page_id BLOB NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    points_blob BLOB NOT NULL,
    pen_json TEXT NOT NULL,
    zoom_at_creation REAL NOT NULL,
    bbox_x REAL NOT NULL,
    bbox_y REAL NOT NULL,
    bbox_w REAL NOT NULL,
    bbox_h REAL NOT NULL
);";

pub const CREATE_PAGE_TEMPLATES: &str = "
CREATE TABLE IF NOT EXISTS page_templates (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    definition_toml TEXT NOT NULL
);";

pub const CREATE_STROKES_RTREE: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS strokes_rtree USING rtree(
    id,
    min_x, max_x,
    min_y, max_y
);";

pub const CREATE_INDEX_PAGES_SECTION_POSITION: &str =
    "CREATE INDEX IF NOT EXISTS idx_pages_section_position ON pages(section_id, position);";

pub const CREATE_INDEX_SECTIONS_NOTEBOOK_POSITION: &str =
    "CREATE INDEX IF NOT EXISTS idx_sections_notebook_position ON sections(notebook_id, position);";

pub const CREATE_INDEX_SECTIONS_PARENT_POSITION: &str =
    "CREATE INDEX IF NOT EXISTS idx_sections_parent_position ON sections(notebook_id, parent_section_id, position);";

pub const CREATE_INDEX_STROKES_PAGE: &str =
    "CREATE INDEX IF NOT EXISTS idx_strokes_page ON strokes(page_id);";

// Bridges main strokes table → r-tree on delete (rtree has no ON DELETE CASCADE).
pub const CREATE_TRIGGER_STROKES_DELETE_RTREE: &str = "
CREATE TRIGGER IF NOT EXISTS trg_strokes_delete_rtree
AFTER DELETE ON strokes
BEGIN
    DELETE FROM strokes_rtree WHERE id = OLD.rowid;
END;";

pub fn init_schema(conn: &Connection) -> Result<()> {
    let v: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if v < 1 {
        conn.execute_batch(&format!(
            "{}{}{}{}{}{}{}{}{}{}{}",
            CREATE_NOTEBOOKS,
            CREATE_SECTIONS,
            CREATE_PAGES,
            CREATE_STROKES,
            CREATE_PAGE_TEMPLATES,
            CREATE_STROKES_RTREE,
            CREATE_INDEX_PAGES_SECTION_POSITION,
            CREATE_INDEX_SECTIONS_NOTEBOOK_POSITION,
            CREATE_INDEX_SECTIONS_PARENT_POSITION,
            CREATE_INDEX_STROKES_PAGE,
            CREATE_TRIGGER_STROKES_DELETE_RTREE,
        ))?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    if v < 2 {
        conn.execute(
            "ALTER TABLE pages ADD COLUMN name TEXT NOT NULL DEFAULT ''",
            [],
        )?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    if v < 3 {
        if !column_exists(conn, "sections", "parent_section_id")? {
            conn.execute(
                "ALTER TABLE sections ADD COLUMN parent_section_id BLOB NULL REFERENCES sections(id) ON DELETE CASCADE",
                [],
            )?;
        }
        conn.execute(CREATE_INDEX_SECTIONS_PARENT_POSITION, [])?;
        conn.pragma_update(None, "user_version", 3)?;
    }
    if v < 4 {
        // v3 added parent_section_id but earlier-versioned databases of v3
        // built between Phase 4 and Phase 4.5 may have user_version=3 without
        // the column. Re-check and ALTER if missing.
        if !column_exists(conn, "sections", "parent_section_id")? {
            conn.execute(
                "ALTER TABLE sections ADD COLUMN parent_section_id BLOB NULL REFERENCES sections(id) ON DELETE CASCADE",
                [],
            )?;
            conn.execute(CREATE_INDEX_SECTIONS_PARENT_POSITION, [])?;
        }
        conn.pragma_update(None, "user_version", 4)?;
    }
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for r in rows {
        if r? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
