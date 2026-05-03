use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use journal_core::{Page, PageId, PlannerPageAddress, SectionId, TemplateId};

use crate::error::{Result, StorageError};
use crate::util::{blob_to_uuid, uuid_to_blob};

const PAGE_COLUMNS: &str =
    "id, section_id, position, template_id, planner_address_json, created_at, modified_at, name, widget_overrides_json";

pub fn insert_page(conn: &Connection, page: &Page) -> Result<()> {
    let template_blob = page.template_id.map(|t| uuid_to_blob(t.0));
    let planner_json = match &page.planner_address {
        Some(a) => Some(serde_json::to_string(a)?),
        None => None,
    };
    let overrides_json = serde_json::to_string(&page.widget_overrides)?;
    conn.execute(
        "INSERT INTO pages (id, section_id, position, template_id, planner_address_json, created_at, modified_at, name, widget_overrides_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            uuid_to_blob(page.id.0),
            uuid_to_blob(page.section_id.0),
            page.position as i64,
            template_blob,
            planner_json,
            page.created_at.to_rfc3339(),
            page.modified_at.to_rfc3339(),
            page.name,
            overrides_json,
        ],
    )?;
    Ok(())
}

pub fn get_page(conn: &Connection, id: PageId) -> Result<Page> {
    let inner = conn
        .query_row(
            &format!("SELECT {} FROM pages WHERE id = ?1", PAGE_COLUMNS),
            params![uuid_to_blob(id.0)],
            row_to_page,
        )
        .optional()?;
    inner.ok_or(StorageError::NotFound)?
}

/// Find a page in `section_id` whose `planner_address` matches `address`.
/// Used by the planner page-generation flow to avoid duplicating pages that
/// already exist on disk for a given calendar slot.
pub fn find_page_by_address(
    conn: &Connection,
    section_id: SectionId,
    address: &PlannerPageAddress,
) -> Result<Option<Page>> {
    let needle = serde_json::to_string(address)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM pages WHERE section_id = ?1 AND planner_address_json = ?2 LIMIT 1",
        PAGE_COLUMNS,
    ))?;
    let mut rows = stmt.query(params![uuid_to_blob(section_id.0), needle])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(row_to_page(row)??));
    }
    Ok(None)
}

pub fn list_pages(conn: &Connection, section_id: SectionId) -> Result<Vec<Page>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM pages WHERE section_id = ?1 ORDER BY position ASC",
        PAGE_COLUMNS,
    ))?;
    let rows = stmt.query_map(params![uuid_to_blob(section_id.0)], row_to_page)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r??);
    }
    Ok(out)
}

pub fn update_page(conn: &Connection, page: &Page) -> Result<()> {
    let template_blob = page.template_id.map(|t| uuid_to_blob(t.0));
    let planner_json = match &page.planner_address {
        Some(a) => Some(serde_json::to_string(a)?),
        None => None,
    };
    let overrides_json = serde_json::to_string(&page.widget_overrides)?;
    let updated = conn.execute(
        "UPDATE pages SET section_id = ?2, position = ?3, template_id = ?4,
            planner_address_json = ?5, created_at = ?6, modified_at = ?7, name = ?8,
            widget_overrides_json = ?9
         WHERE id = ?1",
        params![
            uuid_to_blob(page.id.0),
            uuid_to_blob(page.section_id.0),
            page.position as i64,
            template_blob,
            planner_json,
            page.created_at.to_rfc3339(),
            page.modified_at.to_rfc3339(),
            page.name,
            overrides_json,
        ],
    )?;
    if updated == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

pub fn delete_page(conn: &Connection, id: PageId) -> Result<()> {
    let removed = conn.execute(
        "DELETE FROM pages WHERE id = ?1",
        params![uuid_to_blob(id.0)],
    )?;
    if removed == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

/// Move a page into `new_section_id` at `new_position`, recomputing positions in both sections.
pub fn move_page(
    conn: &mut Connection,
    id: PageId,
    new_section_id: SectionId,
    new_position: u32,
) -> Result<()> {
    let tx = conn.transaction()?;
    let (old_section_blob, old_position): (Vec<u8>, i64) = tx.query_row(
        "SELECT section_id, position FROM pages WHERE id = ?1",
        params![uuid_to_blob(id.0)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let new_section_blob = uuid_to_blob(new_section_id.0);
    if old_section_blob == new_section_blob {
        drop(tx);
        return reorder_page(conn, id, new_position);
    }
    tx.execute(
        "UPDATE pages SET position = position - 1
         WHERE section_id = ?1 AND position > ?2",
        params![old_section_blob, old_position],
    )?;
    tx.execute(
        "UPDATE pages SET position = position + 1
         WHERE section_id = ?1 AND position >= ?2",
        params![new_section_blob, new_position as i64],
    )?;
    tx.execute(
        "UPDATE pages SET section_id = ?1, position = ?2 WHERE id = ?3",
        params![new_section_blob, new_position as i64, uuid_to_blob(id.0)],
    )?;
    tx.commit()?;
    Ok(())
}

/// Move a page to `new_position` within its section, shifting siblings to keep positions contiguous.
pub fn reorder_page(conn: &mut Connection, id: PageId, new_position: u32) -> Result<()> {
    let tx = conn.transaction()?;
    let (section_blob, old_position): (Vec<u8>, i64) = tx.query_row(
        "SELECT section_id, position FROM pages WHERE id = ?1",
        params![uuid_to_blob(id.0)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let new_pos = new_position as i64;
    if new_pos == old_position {
        tx.commit()?;
        return Ok(());
    }
    if new_pos < old_position {
        tx.execute(
            "UPDATE pages SET position = position + 1
             WHERE section_id = ?1 AND position >= ?2 AND position < ?3 AND id != ?4",
            params![section_blob, new_pos, old_position, uuid_to_blob(id.0)],
        )?;
    } else {
        tx.execute(
            "UPDATE pages SET position = position - 1
             WHERE section_id = ?1 AND position > ?2 AND position <= ?3 AND id != ?4",
            params![section_blob, old_position, new_pos, uuid_to_blob(id.0)],
        )?;
    }
    tx.execute(
        "UPDATE pages SET position = ?1 WHERE id = ?2",
        params![new_pos, uuid_to_blob(id.0)],
    )?;
    tx.commit()?;
    Ok(())
}

fn row_to_page(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Page>> {
    let id_blob: Vec<u8> = row.get(0)?;
    let section_blob: Vec<u8> = row.get(1)?;
    let position: i64 = row.get(2)?;
    let template_blob: Option<Vec<u8>> = row.get(3)?;
    let planner_json: Option<String> = row.get(4)?;
    let created_at_str: String = row.get(5)?;
    let modified_at_str: String = row.get(6)?;
    let name: String = row.get(7)?;
    let overrides_json: String = row.get::<_, Option<String>>(8)?.unwrap_or_else(|| "{}".into());
    Ok((|| {
        let template_id = match template_blob {
            Some(b) => Some(TemplateId(blob_to_uuid(&b)?)),
            None => None,
        };
        let planner_address = match planner_json {
            Some(s) => Some(serde_json::from_str::<PlannerPageAddress>(&s)?),
            None => None,
        };
        let created_at = parse_dt(&created_at_str)?;
        let modified_at = parse_dt(&modified_at_str)?;
        let widget_overrides = serde_json::from_str(&overrides_json).unwrap_or_default();
        Ok(Page {
            id: PageId(blob_to_uuid(&id_blob)?),
            section_id: SectionId(blob_to_uuid(&section_blob)?),
            position: position as u32,
            template_id,
            planner_address,
            created_at,
            modified_at,
            name,
            widget_overrides,
        })
    })())
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| StorageError::InvalidData(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::notebook_store::insert_notebook;
    use crate::section_store::insert_section;
    use journal_core::{Notebook, NotebookId, NotebookKind, Section};
    use uuid::Uuid;

    fn setup() -> (Db, SectionId) {
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
        (db, s.id)
    }

    fn make_page(section_id: SectionId, position: u32) -> Page {
        let now = Utc::now();
        Page {
            id: PageId(Uuid::new_v4()),
            section_id,
            position,
            template_id: None,
            planner_address: None,
            created_at: now,
            modified_at: now,
            name: String::new(),
            widget_overrides: Default::default(),
        }
    }

    #[test]
    fn round_trip_page() {
        let (db, sid) = setup();
        let p = make_page(sid, 0);
        insert_page(db.conn(), &p).unwrap();
        let got = get_page(db.conn(), p.id).unwrap();
        assert_eq!(got.id, p.id);
        assert_eq!(got.section_id, p.section_id);
        assert_eq!(got.position, p.position);
    }

    #[test]
    fn reorder_pages_keeps_positions_contiguous() {
        let (mut db, sid) = setup();
        let p0 = make_page(sid, 0);
        let p1 = make_page(sid, 1);
        let p2 = make_page(sid, 2);
        let p3 = make_page(sid, 3);
        for p in [&p0, &p1, &p2, &p3] {
            insert_page(db.conn(), p).unwrap();
        }
        // Move p3 to position 1.
        reorder_page(db.conn_mut(), p3.id, 1).unwrap();
        let listed = list_pages(db.conn(), sid).unwrap();
        let order: Vec<_> = listed.iter().map(|p| p.id).collect();
        assert_eq!(order, vec![p0.id, p3.id, p1.id, p2.id]);
        for (i, p) in listed.iter().enumerate() {
            assert_eq!(p.position, i as u32);
        }

        // Move p3 back to position 3.
        reorder_page(db.conn_mut(), p3.id, 3).unwrap();
        let listed = list_pages(db.conn(), sid).unwrap();
        let order: Vec<_> = listed.iter().map(|p| p.id).collect();
        assert_eq!(order, vec![p0.id, p1.id, p2.id, p3.id]);
        for (i, p) in listed.iter().enumerate() {
            assert_eq!(p.position, i as u32);
        }
    }

    #[test]
    fn page_name_round_trips() {
        let (db, sid) = setup();
        let mut p = make_page(sid, 0);
        p.name = "My Page".into();
        insert_page(db.conn(), &p).unwrap();
        let got = get_page(db.conn(), p.id).unwrap();
        assert_eq!(got.name, "My Page");

        let mut updated = got;
        updated.name = "Renamed".into();
        update_page(db.conn(), &updated).unwrap();
        let got = get_page(db.conn(), updated.id).unwrap();
        assert_eq!(got.name, "Renamed");
    }

    #[test]
    fn delete_removes_page() {
        let (db, sid) = setup();
        let p = make_page(sid, 0);
        insert_page(db.conn(), &p).unwrap();
        delete_page(db.conn(), p.id).unwrap();
        assert!(matches!(
            get_page(db.conn(), p.id),
            Err(StorageError::NotFound)
        ));
    }
}
