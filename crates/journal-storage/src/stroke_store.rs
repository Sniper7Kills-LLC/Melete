use rusqlite::{params, Connection};

use journal_core::{PageId, PenSettings, Rect, Stroke};

use crate::error::{Result, StorageError};
use crate::stroke_codec::{pack_points, unpack_points};
use crate::util::{blob_to_uuid, uuid_to_blob};

pub fn insert_stroke(conn: &Connection, stroke: &Stroke, page_id: PageId) -> Result<()> {
    let blob = pack_points(&stroke.points);
    let pen_json = serde_json::to_string(&stroke.pen)?;
    let bbox = stroke.bounding_box;

    conn.execute(
        "INSERT INTO strokes (id, page_id, points_blob, pen_json, zoom_at_creation, bbox_x, bbox_y, bbox_w, bbox_h)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            uuid_to_blob(stroke.id),
            uuid_to_blob(page_id.0),
            blob,
            pen_json,
            stroke.zoom_at_creation,
            bbox.x,
            bbox.y,
            bbox.width,
            bbox.height,
        ],
    )?;

    // Bridge into r-tree using the stroke's rowid as the spatial id; trigger handles deletion.
    let rowid: i64 = conn.query_row(
        "SELECT rowid FROM strokes WHERE id = ?1",
        params![uuid_to_blob(stroke.id)],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO strokes_rtree (id, min_x, max_x, min_y, max_y) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            rowid,
            bbox.x,
            bbox.x + bbox.width,
            bbox.y,
            bbox.y + bbox.height,
        ],
    )?;
    Ok(())
}

pub fn delete_stroke(conn: &Connection, id: uuid::Uuid) -> Result<()> {
    let removed = conn.execute(
        "DELETE FROM strokes WHERE id = ?1",
        params![uuid_to_blob(id)],
    )?;
    if removed == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

pub fn update_stroke(conn: &Connection, stroke: &Stroke, page_id: PageId) -> Result<()> {
    let blob = pack_points(&stroke.points);
    let pen_json = serde_json::to_string(&stroke.pen)?;
    let bbox = stroke.bounding_box;

    let updated = conn.execute(
        "UPDATE strokes SET points_blob = ?1, pen_json = ?2, zoom_at_creation = ?3,
         bbox_x = ?4, bbox_y = ?5, bbox_w = ?6, bbox_h = ?7
         WHERE id = ?8 AND page_id = ?9",
        params![
            blob,
            pen_json,
            stroke.zoom_at_creation,
            bbox.x,
            bbox.y,
            bbox.width,
            bbox.height,
            uuid_to_blob(stroke.id),
            uuid_to_blob(page_id.0),
        ],
    )?;
    if updated == 0 {
        return Err(StorageError::NotFound);
    }

    let rowid: i64 = conn.query_row(
        "SELECT rowid FROM strokes WHERE id = ?1",
        params![uuid_to_blob(stroke.id)],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE strokes_rtree SET min_x = ?1, max_x = ?2, min_y = ?3, max_y = ?4 WHERE id = ?5",
        params![
            bbox.x,
            bbox.x + bbox.width,
            bbox.y,
            bbox.y + bbox.height,
            rowid,
        ],
    )?;
    Ok(())
}

/// Delete one stroke and insert N children in a single transaction.
pub fn replace_stroke(
    conn: &Connection,
    old_id: uuid::Uuid,
    new_strokes: &[Stroke],
    page_id: PageId,
) -> Result<()> {
    delete_stroke(conn, old_id)?;
    for s in new_strokes {
        insert_stroke(conn, s, page_id)?;
    }
    Ok(())
}

pub fn delete_strokes_batch(conn: &Connection, ids: &[uuid::Uuid]) -> Result<()> {
    for id in ids {
        let _ = delete_stroke(conn, *id);
    }
    Ok(())
}

pub fn list_strokes_for_page(conn: &Connection, page_id: PageId) -> Result<Vec<Stroke>> {
    let mut stmt = conn.prepare(
        "SELECT id, points_blob, pen_json, zoom_at_creation, bbox_x, bbox_y, bbox_w, bbox_h
         FROM strokes WHERE page_id = ?1 ORDER BY rowid ASC",
    )?;
    let rows = stmt.query_map(params![uuid_to_blob(page_id.0)], row_to_stroke)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r??);
    }
    Ok(out)
}

pub fn query_strokes_in_rect(
    conn: &Connection,
    page_id: PageId,
    rect: Rect,
) -> Result<Vec<Stroke>> {
    let max_x = rect.x + rect.width;
    let max_y = rect.y + rect.height;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.points_blob, s.pen_json, s.zoom_at_creation, s.bbox_x, s.bbox_y, s.bbox_w, s.bbox_h
         FROM strokes_rtree r
         JOIN strokes s ON s.rowid = r.id
         WHERE s.page_id = ?1
           AND r.max_x >= ?2 AND r.min_x <= ?3
           AND r.max_y >= ?4 AND r.min_y <= ?5
         ORDER BY s.rowid ASC",
    )?;
    let rows = stmt.query_map(
        params![uuid_to_blob(page_id.0), rect.x, max_x, rect.y, max_y],
        row_to_stroke,
    )?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r??);
    }
    Ok(out)
}

fn row_to_stroke(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Stroke>> {
    let id_blob: Vec<u8> = row.get(0)?;
    let points_blob: Vec<u8> = row.get(1)?;
    let pen_json: String = row.get(2)?;
    let zoom: f64 = row.get(3)?;
    let bx: f64 = row.get(4)?;
    let by: f64 = row.get(5)?;
    let bw: f64 = row.get(6)?;
    let bh: f64 = row.get(7)?;
    Ok((|| {
        let id = blob_to_uuid(&id_blob)?;
        let points = unpack_points(&points_blob)?;
        let pen: PenSettings = serde_json::from_str(&pen_json)?;
        Ok(Stroke {
            id,
            points,
            pen,
            zoom_at_creation: zoom,
            bounding_box: Rect {
                x: bx,
                y: by,
                width: bw,
                height: bh,
            },
        })
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::notebook_store::insert_notebook;
    use crate::page_store::insert_page;
    use crate::section_store::insert_section;
    use chrono::Utc;
    use journal_core::{
        BlendMode, Color, Notebook, NotebookId, NotebookKind, Page, PenSettings, Section,
        SectionId, StrokePoint,
    };
    use uuid::Uuid;

    fn pen() -> PenSettings {
        PenSettings {
            color: Color {
                r: 10,
                g: 20,
                b: 30,
                a: 255,
            },
            base_width: 2.0,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
        }
    }

    fn setup() -> (Db, PageId) {
        let db = Db::open_in_memory().unwrap();
        let nb = Notebook {
            id: NotebookId(Uuid::new_v4()),
            name: "n".into(),
            kind: NotebookKind::Standard,
            assigned_templates: vec![],
        };
        insert_notebook(db.conn(), &nb).unwrap();
        let s = Section {
            id: SectionId(Uuid::new_v4()),
            notebook_id: nb.id,
            name: "s".into(),
            position: 0,
            allowed_templates: None,
            parent_section_id: None,
        };
        insert_section(db.conn(), &s).unwrap();
        let now = Utc::now();
        let p = Page {
            id: PageId(Uuid::new_v4()),
            section_id: s.id,
            position: 0,
            template_id: None,
            planner_address: None,
            created_at: now,
            modified_at: now,
            name: String::new(),
        };
        insert_page(db.conn(), &p).unwrap();
        (db, p.id)
    }

    fn make_stroke(at_x: f64, at_y: f64, size: f64) -> Stroke {
        let points = vec![
            StrokePoint {
                x: at_x,
                y: at_y,
                pressure: 0.5,
                tilt_x: 0.1,
                tilt_y: 0.2,
                timestamp_ms: 1,
            },
            StrokePoint {
                x: at_x + size,
                y: at_y + size,
                pressure: 0.8,
                tilt_x: 0.0,
                tilt_y: 0.0,
                timestamp_ms: 2,
            },
        ];
        Stroke {
            id: Uuid::new_v4(),
            points,
            pen: pen(),
            zoom_at_creation: 1.0,
            bounding_box: Rect {
                x: at_x,
                y: at_y,
                width: size,
                height: size,
            },
        }
    }

    #[test]
    fn round_trip_stroke_with_points() {
        let (db, pid) = setup();
        let s = make_stroke(0.0, 0.0, 100.0);
        insert_stroke(db.conn(), &s, pid).unwrap();
        let listed = list_strokes_for_page(db.conn(), pid).unwrap();
        assert_eq!(listed.len(), 1);
        let got = &listed[0];
        assert_eq!(got.id, s.id);
        assert_eq!(got.points, s.points);
        assert_eq!(got.pen, s.pen);
        assert_eq!(got.bounding_box, s.bounding_box);
    }

    #[test]
    fn rtree_culls_strokes_outside_rect() {
        let (db, pid) = setup();
        let near = make_stroke(10.0, 10.0, 5.0);   // bbox (10..15, 10..15)
        let far = make_stroke(1000.0, 1000.0, 5.0); // bbox (1000..1005, 1000..1005)
        insert_stroke(db.conn(), &near, pid).unwrap();
        insert_stroke(db.conn(), &far, pid).unwrap();

        let hit_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        };
        let hits = query_strokes_in_rect(db.conn(), pid, hit_rect).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, near.id);

        let big_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 2000.0,
            height: 2000.0,
        };
        let all_hits = query_strokes_in_rect(db.conn(), pid, big_rect).unwrap();
        assert_eq!(all_hits.len(), 2);
    }

    #[test]
    fn delete_removes_from_rtree() {
        let (db, pid) = setup();
        let s = make_stroke(0.0, 0.0, 10.0);
        insert_stroke(db.conn(), &s, pid).unwrap();
        delete_stroke(db.conn(), s.id).unwrap();
        let big_rect = Rect {
            x: -100.0,
            y: -100.0,
            width: 1000.0,
            height: 1000.0,
        };
        let hits = query_strokes_in_rect(db.conn(), pid, big_rect).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn cascade_delete_strokes_with_page() {
        let (db, pid) = setup();
        insert_stroke(db.conn(), &make_stroke(0.0, 0.0, 1.0), pid).unwrap();
        insert_stroke(db.conn(), &make_stroke(5.0, 5.0, 1.0), pid).unwrap();
        crate::page_store::delete_page(db.conn(), pid).unwrap();
        let listed = list_strokes_for_page(db.conn(), pid).unwrap();
        assert!(listed.is_empty());
    }
}
