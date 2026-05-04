use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use journal_core::{NotebookId, Section, SectionId, TemplateId};

use crate::error::{Result, StorageError};
use crate::util::{blob_to_uuid, uuid_to_blob};

pub fn insert_section(conn: &Connection, section: &Section) -> Result<()> {
    let allowed_json = match &section.allowed_templates {
        Some(v) => Some(serde_json::to_string(v)?),
        None => None,
    };
    let parent_blob = section.parent_section_id.map(|p| uuid_to_blob(p.0));
    conn.execute(
        "INSERT INTO sections (id, notebook_id, name, position, allowed_templates_json, parent_section_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            uuid_to_blob(section.id.0),
            uuid_to_blob(section.notebook_id.0),
            section.name,
            section.position as i64,
            allowed_json,
            parent_blob,
        ],
    )?;
    Ok(())
}

pub fn get_section(conn: &Connection, id: SectionId) -> Result<Section> {
    let inner = conn
        .query_row(
            "SELECT id, notebook_id, name, position, allowed_templates_json, parent_section_id
             FROM sections WHERE id = ?1",
            params![uuid_to_blob(id.0)],
            row_to_section,
        )
        .optional()?;
    inner.ok_or(StorageError::NotFound)?
}

/// Lists ALL sections of the notebook (flat). Callers may still want this for
/// breadth-first walks; for hierarchical rendering use `list_root_sections` +
/// `list_child_sections`.
pub fn list_sections(conn: &Connection, notebook_id: NotebookId) -> Result<Vec<Section>> {
    let mut stmt = conn.prepare(
        "SELECT id, notebook_id, name, position, allowed_templates_json, parent_section_id
         FROM sections WHERE notebook_id = ?1 ORDER BY position ASC",
    )?;
    let rows = stmt.query_map(params![uuid_to_blob(notebook_id.0)], row_to_section)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r??);
    }
    Ok(out)
}

/// Lists root-level sections (no parent) for the notebook, ordered by position.
pub fn list_root_sections(conn: &Connection, notebook_id: NotebookId) -> Result<Vec<Section>> {
    let mut stmt = conn.prepare(
        "SELECT id, notebook_id, name, position, allowed_templates_json, parent_section_id
         FROM sections WHERE notebook_id = ?1 AND parent_section_id IS NULL
         ORDER BY position ASC",
    )?;
    let rows = stmt.query_map(params![uuid_to_blob(notebook_id.0)], row_to_section)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r??);
    }
    Ok(out)
}

/// Lists immediate child sections of `parent_id`, ordered by position.
pub fn list_child_sections(conn: &Connection, parent_id: SectionId) -> Result<Vec<Section>> {
    let mut stmt = conn.prepare(
        "SELECT id, notebook_id, name, position, allowed_templates_json, parent_section_id
         FROM sections WHERE parent_section_id = ?1 ORDER BY position ASC",
    )?;
    let rows = stmt.query_map(params![uuid_to_blob(parent_id.0)], row_to_section)?;
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
    let parent_blob = section.parent_section_id.map(|p| uuid_to_blob(p.0));
    let updated = conn.execute(
        "UPDATE sections SET notebook_id = ?2, name = ?3, position = ?4, allowed_templates_json = ?5,
            parent_section_id = ?6
         WHERE id = ?1",
        params![
            uuid_to_blob(section.id.0),
            uuid_to_blob(section.notebook_id.0),
            section.name,
            section.position as i64,
            allowed_json,
            parent_blob,
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

/// Move a section to `new_position` within its current parent group, shifting
/// siblings to keep positions contiguous. Cross-parent reorders are out of
/// scope and rejected by the caller (UI layer).
pub fn reorder_section(conn: &mut Connection, id: SectionId, new_position: u32) -> Result<()> {
    let tx = conn.transaction()?;
    let (notebook_blob, parent_blob, old_position): (Vec<u8>, Option<Vec<u8>>, i64) = tx
        .query_row(
            "SELECT notebook_id, parent_section_id, position FROM sections WHERE id = ?1",
            params![uuid_to_blob(id.0)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
    let new_pos = new_position as i64;
    if new_pos == old_position {
        tx.commit()?;
        return Ok(());
    }
    // Use IS comparison so NULL parent groups are treated equivalently.
    if new_pos < old_position {
        tx.execute(
            "UPDATE sections SET position = position + 1
             WHERE notebook_id = ?1 AND parent_section_id IS ?2
               AND position >= ?3 AND position < ?4 AND id != ?5",
            params![
                notebook_blob,
                parent_blob,
                new_pos,
                old_position,
                uuid_to_blob(id.0)
            ],
        )?;
    } else {
        tx.execute(
            "UPDATE sections SET position = position - 1
             WHERE notebook_id = ?1 AND parent_section_id IS ?2
               AND position > ?3 AND position <= ?4 AND id != ?5",
            params![
                notebook_blob,
                parent_blob,
                old_position,
                new_pos,
                uuid_to_blob(id.0)
            ],
        )?;
    }
    tx.execute(
        "UPDATE sections SET position = ?1 WHERE id = ?2",
        params![new_pos, uuid_to_blob(id.0)],
    )?;
    tx.commit()?;
    Ok(())
}

/// Find an existing section under `parent_section_id` (within `notebook_id`)
/// with the given `name`, or create a new one at the end of the sibling group.
pub fn ensure_section(
    conn: &mut Connection,
    notebook_id: NotebookId,
    parent_section_id: Option<SectionId>,
    name: &str,
) -> Result<Section> {
    let parent_blob = parent_section_id.map(|p| uuid_to_blob(p.0));
    let existing: Option<Section> = conn
        .query_row(
            "SELECT id, notebook_id, name, position, allowed_templates_json, parent_section_id
             FROM sections WHERE notebook_id = ?1 AND parent_section_id IS ?2 AND name = ?3",
            params![uuid_to_blob(notebook_id.0), parent_blob, name],
            row_to_section,
        )
        .optional()?
        .transpose()?;
    if let Some(s) = existing {
        return Ok(s);
    }
    let next_pos: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM sections
         WHERE notebook_id = ?1 AND parent_section_id IS ?2",
        params![uuid_to_blob(notebook_id.0), parent_blob],
        |r| r.get(0),
    )?;
    let section = Section {
        id: SectionId(Uuid::new_v4()),
        notebook_id,
        name: name.to_string(),
        position: next_pos as u32,
        allowed_templates: None,
        parent_section_id,
    };
    insert_section(conn, &section)?;
    Ok(section)
}

fn row_to_section(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Section>> {
    let id_blob: Vec<u8> = row.get(0)?;
    let notebook_blob: Vec<u8> = row.get(1)?;
    let name: String = row.get(2)?;
    let position: i64 = row.get(3)?;
    let allowed_json: Option<String> = row.get(4)?;
    let parent_blob: Option<Vec<u8>> = row.get(5)?;
    Ok((|| {
        let allowed_templates = match allowed_json {
            Some(s) => Some(serde_json::from_str::<Vec<TemplateId>>(&s)?),
            None => None,
        };
        let parent_section_id = match parent_blob {
            Some(b) => Some(SectionId(blob_to_uuid(&b)?)),
            None => None,
        };
        Ok(Section {
            id: SectionId(blob_to_uuid(&id_blob)?),
            notebook_id: NotebookId(blob_to_uuid(&notebook_blob)?),
            name,
            position: position as u32,
            allowed_templates,
            parent_section_id,
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
            parent_section_id: None,
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

    #[test]
    fn nested_sections_partition_lists() {
        let db = Db::open_in_memory().unwrap();
        let nb_id = make_notebook(&db);
        let root = make_section(nb_id, 0, "Root");
        insert_section(db.conn(), &root).unwrap();
        let mut child = make_section(nb_id, 0, "Child");
        child.parent_section_id = Some(root.id);
        insert_section(db.conn(), &child).unwrap();

        let roots = list_root_sections(db.conn(), nb_id).unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, root.id);

        let children = list_child_sections(db.conn(), root.id).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child.id);
        assert_eq!(children[0].parent_section_id, Some(root.id));

        // Flat list still returns both.
        let flat = list_sections(db.conn(), nb_id).unwrap();
        assert_eq!(flat.len(), 2);
    }

    #[test]
    fn child_cascades_when_parent_deleted() {
        let db = Db::open_in_memory().unwrap();
        let nb_id = make_notebook(&db);
        let root = make_section(nb_id, 0, "Root");
        insert_section(db.conn(), &root).unwrap();
        let mut child = make_section(nb_id, 0, "Child");
        child.parent_section_id = Some(root.id);
        insert_section(db.conn(), &child).unwrap();

        delete_section(db.conn(), root.id).unwrap();
        let flat = list_sections(db.conn(), nb_id).unwrap();
        assert!(flat.is_empty());
    }

    #[test]
    fn ensure_section_creates_then_reuses() {
        let mut db = Db::open_in_memory().unwrap();
        let nb_id = make_notebook(&db);
        let s1 = ensure_section(db.conn_mut(), nb_id, None, "2026").unwrap();
        let s2 = ensure_section(db.conn_mut(), nb_id, None, "2026").unwrap();
        assert_eq!(s1.id, s2.id);

        // Same name under a different parent is treated as a separate section.
        let inside = ensure_section(db.conn_mut(), nb_id, Some(s1.id), "2026").unwrap();
        assert_ne!(inside.id, s1.id);
    }

    #[test]
    fn reorder_within_parent_group() {
        let mut db = Db::open_in_memory().unwrap();
        let nb_id = make_notebook(&db);
        let root = ensure_section(db.conn_mut(), nb_id, None, "Root").unwrap();
        let a = ensure_section(db.conn_mut(), nb_id, Some(root.id), "A").unwrap();
        let b = ensure_section(db.conn_mut(), nb_id, Some(root.id), "B").unwrap();
        let c = ensure_section(db.conn_mut(), nb_id, Some(root.id), "C").unwrap();

        reorder_section(db.conn_mut(), c.id, 0).unwrap();
        let listed = list_child_sections(db.conn(), root.id).unwrap();
        let names: Vec<_> = listed.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["C", "A", "B"]);
        for (i, s) in listed.iter().enumerate() {
            assert_eq!(s.position, i as u32);
        }
        // A unrelated root sibling (not in this group) shouldn't have moved.
        let _ = (a.id, b.id);
    }
}
