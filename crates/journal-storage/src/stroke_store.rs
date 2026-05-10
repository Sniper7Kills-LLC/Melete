use rusqlite::{params, Connection};

use journal_core::{PageId, PenSettings, Rect, Stroke};

use crate::error::{Result, StorageError};
use crate::stroke_codec::{pack_points, unpack_points};
use crate::util::{blob_to_uuid, uuid_to_blob};

pub fn insert_stroke(conn: &Connection, stroke: &Stroke, page_id: PageId) -> Result<()> {
    let blob = pack_points(&stroke.points)?;
    let pen_json = serde_json::to_string(&stroke.pen)?;
    let recipe_json = stroke
        .brush_recipe
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let bbox = stroke.bounding_box;
    let now = chrono::Utc::now().to_rfc3339();
    tracing::debug!(
        "STROKE_MOD sqlite upsert: id={} page={:?} points={} updated_at={}",
        stroke.id,
        page_id,
        stroke.points.len(),
        now
    );

    // UPSERT: undo of a delete re-inserts the same id, but the row
    // still exists soft-deleted. Clear deleted_at + bump updated_at +
    // refresh fields. Same INSERT path covers fresh inserts.
    conn.execute(
        "INSERT INTO strokes (id, page_id, points_blob, pen_json, zoom_at_creation, bbox_x, bbox_y, bbox_w, bbox_h, brush_recipe_json, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, NULL)
         ON CONFLICT(id) DO UPDATE SET
           page_id = excluded.page_id,
           points_blob = excluded.points_blob,
           pen_json = excluded.pen_json,
           zoom_at_creation = excluded.zoom_at_creation,
           bbox_x = excluded.bbox_x,
           bbox_y = excluded.bbox_y,
           bbox_w = excluded.bbox_w,
           bbox_h = excluded.bbox_h,
           brush_recipe_json = excluded.brush_recipe_json,
           updated_at = excluded.updated_at,
           deleted_at = NULL",
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
            recipe_json,
            now,
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

/// Soft-delete: stamp `deleted_at` (and bump `updated_at`) instead of
/// removing the row. Reads filter the row out via `WHERE deleted_at
/// IS NULL`; sync finds rows-to-cloud-delete via the inverse
/// predicate; cloud-pull skips re-merging ids that are
/// present-but-deleted locally.
pub fn delete_stroke(conn: &Connection, id: uuid::Uuid) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let updated = conn.execute(
        "UPDATE strokes SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        params![now, uuid_to_blob(id)],
    )?;
    if updated == 0 {
        // Either the stroke never existed or was already deleted.
        // Treat both as already-in-target-state — callers do
        // best-effort sync follow-up regardless.
        tracing::debug!("STROKE_MOD sqlite soft-delete: id={} NOT FOUND (already deleted or absent)", id);
        return Err(StorageError::NotFound);
    }
    tracing::debug!("STROKE_MOD sqlite soft-delete OK: id={} deleted_at={}", id, now);
    // Drop from rtree so spatial queries don't return stale geometry.
    let _ = conn.execute(
        "DELETE FROM strokes_rtree WHERE id = (SELECT rowid FROM strokes WHERE id = ?1)",
        params![uuid_to_blob(id)],
    );
    Ok(())
}

/// True when the local DB has the stroke marked as soft-deleted.
/// Used by the cloud-pull path to skip re-inserting into the live
/// view (the row is still in SQLite under the soft-delete predicate).
pub fn is_deleted(conn: &Connection, id: uuid::Uuid) -> Result<bool> {
    let row: Option<Option<String>> = conn
        .query_row(
            "SELECT deleted_at FROM strokes WHERE id = ?1",
            params![uuid_to_blob(id)],
            |r| r.get(0),
        )
        .ok();
    Ok(matches!(row, Some(Some(_))))
}

/// List every (id, deleted_at) for strokes the user has erased
/// locally. Sync uses this to push cloud deletes; after a successful
/// delete, sync should also `purge_deleted` to free the row.
pub fn list_deleted(conn: &Connection) -> Result<Vec<(uuid::Uuid, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, deleted_at FROM strokes WHERE deleted_at IS NOT NULL ORDER BY deleted_at",
    )?;
    let rows = stmt.query_map([], |r| {
        let id: Vec<u8> = r.get(0)?;
        let ts: String = r.get(1)?;
        Ok((id, ts))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (id, ts) = r?;
        if let Ok(uuid) = blob_to_uuid(&id) {
            out.push((uuid, ts));
        }
    }
    Ok(out)
}

/// Hard-remove a soft-deleted stroke. Sync calls this after the
/// cloud-delete mutation succeeds (or returns "row already gone")
/// so the local table doesn't grow unbounded with tombstones.
#[allow(dead_code)]
pub fn purge_deleted(conn: &Connection, id: uuid::Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM strokes WHERE id = ?1 AND deleted_at IS NOT NULL",
        params![uuid_to_blob(id)],
    )?;
    Ok(())
}

pub fn update_stroke(conn: &Connection, stroke: &Stroke, page_id: PageId) -> Result<()> {
    let blob = pack_points(&stroke.points)?;
    let pen_json = serde_json::to_string(&stroke.pen)?;
    let recipe_json = stroke
        .brush_recipe
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let bbox = stroke.bounding_box;

    let now = chrono::Utc::now().to_rfc3339();
    let updated = conn.execute(
        "UPDATE strokes SET points_blob = ?1, pen_json = ?2, zoom_at_creation = ?3,
         bbox_x = ?4, bbox_y = ?5, bbox_w = ?6, bbox_h = ?7, brush_recipe_json = ?8,
         updated_at = ?11
         WHERE id = ?9 AND page_id = ?10",
        params![
            blob,
            pen_json,
            stroke.zoom_at_creation,
            bbox.x,
            bbox.y,
            bbox.width,
            bbox.height,
            recipe_json,
            uuid_to_blob(stroke.id),
            uuid_to_blob(page_id.0),
            now,
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

/// Delete one stroke and insert N children atomically. If any insert
/// fails the transaction rolls back so the caller never sees a state
/// where the original is gone but the children are partially inserted.
pub fn replace_stroke(
    conn: &mut Connection,
    old_id: uuid::Uuid,
    new_strokes: &[Stroke],
    page_id: PageId,
) -> Result<()> {
    tracing::debug!(
        "STROKE_MOD sqlite replace: old={} -> {} children on page {:?}",
        old_id,
        new_strokes.len(),
        page_id
    );
    let tx = conn.transaction()?;
    delete_stroke(&tx, old_id)?;
    for s in new_strokes {
        insert_stroke(&tx, s, page_id)?;
    }
    tx.commit()?;
    Ok(())
}

pub fn delete_strokes_batch(conn: &Connection, ids: &[uuid::Uuid]) -> Result<()> {
    tracing::debug!("STROKE_MOD sqlite delete_strokes_batch: {} ids", ids.len());
    // Per-id failures get logged but don't abort the batch — the caller
    // is the eraser/lasso path which treats best-effort delete as the
    // contract. NotFound is the common case (already-deleted on a prior
    // pass) and is downgraded to debug so it doesn't drown the log.
    for id in ids {
        if let Err(e) = delete_stroke(conn, *id) {
            match e {
                StorageError::NotFound => {
                    tracing::debug!("delete_strokes_batch: {} not found (already deleted?)", id);
                }
                _ => {
                    tracing::warn!("delete_strokes_batch: {} failed: {}", id, e);
                }
            }
        }
    }
    Ok(())
}

pub fn list_strokes_for_page(conn: &Connection, page_id: PageId) -> Result<Vec<Stroke>> {
    let mut stmt = conn.prepare(
        "SELECT id, points_blob, pen_json, zoom_at_creation, bbox_x, bbox_y, bbox_w, bbox_h, brush_recipe_json
         FROM strokes WHERE page_id = ?1 AND deleted_at IS NULL ORDER BY rowid ASC",
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
        "SELECT s.id, s.points_blob, s.pen_json, s.zoom_at_creation, s.bbox_x, s.bbox_y, s.bbox_w, s.bbox_h, s.brush_recipe_json
         FROM strokes_rtree r
         JOIN strokes s ON s.rowid = r.id
         WHERE s.page_id = ?1 AND s.deleted_at IS NULL
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
    let recipe_json: Option<String> = row.get(8)?;
    Ok((|| {
        let id = blob_to_uuid(&id_blob)?;
        let points = unpack_points(&points_blob)?;
        let pen: PenSettings = serde_json::from_str(&pen_json)?;
        let brush_recipe = match recipe_json.as_deref() {
            Some(s) if !s.is_empty() => Some(serde_json::from_str(s)?),
            _ => None,
        };
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
            brush_recipe,
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
            brush_style: journal_core::ToolStyle::Pen,
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
            widget_overrides: Default::default(),
            widget_data: Default::default(),
            flagged: false,
            bookmark_position: 0,
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
            brush_recipe: None,
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
        assert!(got.brush_recipe.is_none(), "default stroke has no recipe");
    }

    #[test]
    fn round_trip_stroke_with_brush_recipe() {
        use journal_core::{
            Brush, BrushLayer, ColorMod, CursorShape, Geometry, TipShape, WidthMode,
        };
        let (db, pid) = setup();
        let mut s = make_stroke(0.0, 0.0, 50.0);
        let recipe = Brush {
            id: Uuid::new_v4(),
            name: "Custom Pen".into(),
            layers: vec![BrushLayer {
                enabled: true,
                geometry: Geometry::Smooth {
                    resample_step_mm: 0.8,
                },
                width: WidthMode::Pressure {
                    floor: 0.5,
                    amp: 0.5,
                },
                tip: TipShape::Round,
                tip_scale: 1.0,
                color: ColorMod {
                    alpha_mult: 0.9,
                    hue_shift_deg: 0.0,
                },
                blend: BlendMode::Normal,
            }],
            cursor: CursorShape::Auto,
            default_color: None,
        };
        s.brush_recipe = Some(recipe.clone());
        insert_stroke(db.conn(), &s, pid).unwrap();
        let listed = list_strokes_for_page(db.conn(), pid).unwrap();
        assert_eq!(listed.len(), 1);
        let got = &listed[0];
        assert_eq!(got.brush_recipe.as_ref(), Some(&recipe));
    }

    #[test]
    fn rtree_culls_strokes_outside_rect() {
        let (db, pid) = setup();
        let near = make_stroke(10.0, 10.0, 5.0); // bbox (10..15, 10..15)
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

    #[test]
    fn replace_stroke_rolls_back_on_failure() {
        // Set up a parent stroke, then attempt a replace where one of
        // the children violates a NOT NULL constraint by reusing the
        // parent's id (UPSERT would clobber instead of fail) — so we
        // induce failure differently: pass a child whose page_id points
        // at a deleted page, triggering FK violation. Children that
        // reach insert_stroke first must NOT be visible after the
        // failed transaction.
        let (mut db, pid) = setup();
        let parent = make_stroke(0.0, 0.0, 5.0);
        insert_stroke(db.conn(), &parent, pid).unwrap();

        // Two children: first is fine, second references a non-existent
        // page so the FK constraint kicks in and aborts the tx.
        let good_child = make_stroke(1.0, 1.0, 1.0);
        let bad_child = make_stroke(2.0, 2.0, 1.0);
        let bogus_pid = PageId(uuid::Uuid::new_v4());

        // Manual splice: replace_stroke takes a single page_id arg, so
        // we hand-roll the failure case by inserting the bad child
        // against bogus_pid inside our own tx after the good child.
        let conn = db.conn_mut();
        let result: Result<()> = (|| {
            let tx = conn.transaction()?;
            delete_stroke(&tx, parent.id)?;
            insert_stroke(&tx, &good_child, pid)?;
            insert_stroke(&tx, &bad_child, bogus_pid)?;
            tx.commit()?;
            Ok(())
        })();
        assert!(result.is_err(), "expected FK violation to abort");

        // Parent still present (delete rolled back), neither child
        // committed.
        let listed = list_strokes_for_page(db.conn(), pid).unwrap();
        let ids: Vec<uuid::Uuid> = listed.iter().map(|s| s.id).collect();
        assert!(ids.contains(&parent.id), "parent should survive rollback");
        assert!(!ids.contains(&good_child.id), "good child must not commit");
        assert!(!ids.contains(&bad_child.id), "bad child must not commit");
    }

    #[test]
    fn delete_strokes_batch_logs_but_continues_on_missing() {
        // delete_strokes_batch must not abort on NotFound — eraser
        // hits an already-deleted stroke routinely.
        let (db, pid) = setup();
        let s1 = make_stroke(0.0, 0.0, 1.0);
        let s2 = make_stroke(5.0, 5.0, 1.0);
        insert_stroke(db.conn(), &s1, pid).unwrap();
        insert_stroke(db.conn(), &s2, pid).unwrap();

        let phantom = uuid::Uuid::new_v4();
        // Mix a real id, an unknown id, and another real id — all real
        // ones should soft-delete, unknown should be skipped.
        delete_strokes_batch(db.conn(), &[s1.id, phantom, s2.id]).unwrap();

        let listed = list_strokes_for_page(db.conn(), pid).unwrap();
        assert!(listed.is_empty(), "all real ids should be soft-deleted");
    }
}
