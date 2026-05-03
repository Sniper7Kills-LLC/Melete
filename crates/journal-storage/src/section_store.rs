use rusqlite::{params, Connection, OptionalExtension};

use journal_core::{NotebookId, Section, SectionId, TemplateId};

use crate::error::{Result, StorageError};
use crate::util::{blob_to_uuid, uuid_to_blob};

pub fn insert_section(conn: &Connection, section: &Section) -> Result<()> {
    let allowed_json = match &section.allowed_templates {
        Some(v) => Some(serde_json::to_string(v)?),
        None => None,
    };
    conn.execute(
        "INSERT INTO sections (id, notebook_id, name, position, allowed_templates_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            uuid_to_blob(section.id.0),
            uuid_to_blob(section.notebook_id.0),
            section.name,
            section.position as i64,
            allowed_json,
        ],
    )?;
    Ok(())
}

pub fn get_section(conn: &Connection, id: SectionId) -> Result<Section> {
    let inner = conn
        .query_row(
            "SELECT id, notebook_id, name, position, allowed_templates_json
             FROM sections WHERE id = ?1",
            params![uuid_to_blob(id.0)],
            row_to_section,
        )
        .optional()?;
    inner.ok_or(StorageError::NotFound)?
}

pub fn list_sections(conn: &Connection, notebook_id: NotebookId) -> Result<Vec<Section>> {
    let mut stmt = conn.prepare(
        "SELECT id, notebook_id, name, position, allowed_templates_json
         FROM sections WHERE notebook_id = ?1 ORDER BY position ASC",
    )?;
    let rows = stmt.query_map(params![uuid_to_blob(notebook_id.0)], row_to_section)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r??);
    }
    Ok(out)
}

pub fn update_section(conn: &Connection, section: &Section) -> Result<()> {
    let allowed_json = match &section.allowed_templates {
        Some(v) => Some(serde_json::to_string(v)?),
        None => None,
    };
    let updated = conn.execute(
        "UPDATE sections SET notebook_id = ?2, name = ?3, position = ?4, allowed_templates_json = ?5
         WHERE id = ?1",
        params![
            uuid_to_blob(section.id.0),
            uuid_to_blob(section.notebook_id.0),
            section.name,
            section.position as i64,
            allowed_json,
        ],
    )?;
    if updated == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

pub fn delete_section(conn: &Connection, id: SectionId) -> Result<()> {
    let removed = conn.execute(
        "DELETE FROM sections WHERE id = ?1",
        params![uuid_to_blob(id.0)],
    )?;
    if removed == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

/// Move a section to `new_position` within its notebook, shifting siblings to keep positions contiguous.
pub fn reorder_section(conn: &mut Connection, id: SectionId, new_position: u32) -> Result<()> {
    let tx = conn.transaction()?;
    let (notebook_blob, old_position): (Vec<u8>, i64) = tx.query_row(
        "SELECT notebook_id, position FROM sections WHERE id = ?1",
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
            "UPDATE sections SET position = position + 1
             WHERE notebook_id = ?1 AND position >= ?2 AND position < ?3 AND id != ?4",
            params![notebook_blob, new_pos, old_position, uuid_to_blob(id.0)],
        )?;
    } else {
        tx.execute(
            "UPDATE sections SET position = position - 1
             WHERE notebook_id = ?1 AND position > ?2 AND position <= ?3 AND id != ?4",
            params![notebook_blob, old_position, new_pos, uuid_to_blob(id.0)],
        )?;
    }
    tx.execute(
        "UPDATE sections SET position = ?1 WHERE id = ?2",
        params![new_pos, uuid_to_blob(id.0)],
    )?;
    tx.commit()?;
    Ok(())
}

fn row_to_section(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Section>> {
    let id_blob: Vec<u8> = row.get(0)?;
    let notebook_blob: Vec<u8> = row.get(1)?;
    let name: String = row.get(2)?;
    let position: i64 = row.get(3)?;
    let allowed_json: Option<String> = row.get(4)?;
    Ok((|| {
        let allowed_templates = match allowed_json {
            Some(s) => Some(serde_json::from_str::<Vec<TemplateId>>(&s)?),
            None => None,
        };
        Ok(Section {
            id: SectionId(blob_to_uuid(&id_blob)?),
            notebook_id: NotebookId(blob_to_uuid(&notebook_blob)?),
            name,
            position: position as u32,
            allowed_templates,
        })
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::notebook_store::insert_notebook;
    use journal_core::{Notebook, NotebookKind};
    use uuid::Uuid;

    fn make_notebook(db: &Db) -> NotebookId {
        let nb = Notebook {
            id: NotebookId(Uuid::new_v4()),
            name: "n".into(),
            kind: NotebookKind::Standard,
            assigned_templates: vec![],
        };
        insert_notebook(db.conn(), &nb).unwrap();
        nb.id
    }

    fn make_section(notebook_id: NotebookId, position: u32, name: &str) -> Section {
        Section {
            id: SectionId(Uuid::new_v4()),
            notebook_id,
            name: name.into(),
            position,
            allowed_templates: None,
        }
    }

    #[test]
    fn round_trip_section() {
        let db = Db::open_in_memory().unwrap();
        let nb_id = make_notebook(&db);
        let s = make_section(nb_id, 0, "alpha");
        insert_section(db.conn(), &s).unwrap();
        let got = get_section(db.conn(), s.id).unwrap();
        assert_eq!(got, s);
    }

    #[test]
    fn list_sections_ordered() {
        let db = Db::open_in_memory().unwrap();
        let nb_id = make_notebook(&db);
        let s0 = make_section(nb_id, 0, "a");
        let s1 = make_section(nb_id, 1, "b");
        let s2 = make_section(nb_id, 2, "c");
        // Insert out of order to confirm ORDER BY.
        insert_section(db.conn(), &s2).unwrap();
        insert_section(db.conn(), &s0).unwrap();
        insert_section(db.conn(), &s1).unwrap();
        let listed = list_sections(db.conn(), nb_id).unwrap();
        let names: Vec<_> = listed.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn cascade_delete_from_notebook() {
        let db = Db::open_in_memory().unwrap();
        let nb_id = make_notebook(&db);
        insert_section(db.conn(), &make_section(nb_id, 0, "x")).unwrap();
        insert_section(db.conn(), &make_section(nb_id, 1, "y")).unwrap();
        crate::notebook_store::delete_notebook(db.conn(), nb_id).unwrap();
        let listed = list_sections(db.conn(), nb_id).unwrap();
        assert!(listed.is_empty());
    }
}
