use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use journal_core::{Notebook, NotebookId, NotebookKind, TemplateId};

use crate::error::{Result, StorageError};
use crate::util::{blob_to_uuid, uuid_to_blob};

const KIND_STANDARD: &str = "standard";
const KIND_PLANNER: &str = "planner";

pub fn insert_notebook(conn: &Connection, notebook: &Notebook) -> Result<()> {
    let (kind_str, planner_template_id, planner_creation_date) = match &notebook.kind {
        NotebookKind::Standard => (KIND_STANDARD, None, None),
        NotebookKind::Planner {
            template_id,
            creation_date,
        } => (
            KIND_PLANNER,
            Some(uuid_to_blob(template_id.0)),
            Some(creation_date.to_string()),
        ),
    };
    let assigned_json = serde_json::to_string(&notebook.assigned_templates)?;
    let created_at = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO notebooks (id, name, kind, planner_template_id, planner_creation_date, created_at, assigned_templates_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            uuid_to_blob(notebook.id.0),
            notebook.name,
            kind_str,
            planner_template_id,
            planner_creation_date,
            created_at,
            assigned_json,
        ],
    )?;
    Ok(())
}

pub fn get_notebook(conn: &Connection, id: NotebookId) -> Result<Notebook> {
    let inner = conn
        .query_row(
            "SELECT id, name, kind, planner_template_id, planner_creation_date, assigned_templates_json
             FROM notebooks WHERE id = ?1",
            params![uuid_to_blob(id.0)],
            row_to_notebook,
        )
        .optional()?;
    inner.ok_or(StorageError::NotFound)?
}

pub fn list_notebooks(conn: &Connection) -> Result<Vec<Notebook>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, kind, planner_template_id, planner_creation_date, assigned_templates_json
         FROM notebooks ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], row_to_notebook)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r??);
    }
    Ok(out)
}

pub fn update_notebook(conn: &Connection, notebook: &Notebook) -> Result<()> {
    let (kind_str, planner_template_id, planner_creation_date) = match &notebook.kind {
        NotebookKind::Standard => (KIND_STANDARD, None, None),
        NotebookKind::Planner {
            template_id,
            creation_date,
        } => (
            KIND_PLANNER,
            Some(uuid_to_blob(template_id.0)),
            Some(creation_date.to_string()),
        ),
    };
    let assigned_json = serde_json::to_string(&notebook.assigned_templates)?;

    let updated = conn.execute(
        "UPDATE notebooks SET name = ?2, kind = ?3, planner_template_id = ?4,
            planner_creation_date = ?5, assigned_templates_json = ?6 WHERE id = ?1",
        params![
            uuid_to_blob(notebook.id.0),
            notebook.name,
            kind_str,
            planner_template_id,
            planner_creation_date,
            assigned_json,
        ],
    )?;
    if updated == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

pub fn delete_notebook(conn: &Connection, id: NotebookId) -> Result<()> {
    let removed = conn.execute(
        "DELETE FROM notebooks WHERE id = ?1",
        params![uuid_to_blob(id.0)],
    )?;
    if removed == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

fn row_to_notebook(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Notebook>> {
    let id_blob: Vec<u8> = row.get(0)?;
    let name: String = row.get(1)?;
    let kind_str: String = row.get(2)?;
    let planner_template_blob: Option<Vec<u8>> = row.get(3)?;
    let planner_date_str: Option<String> = row.get(4)?;
    let assigned_json: String = row.get(5)?;

    Ok((|| {
        let id = NotebookId(blob_to_uuid(&id_blob)?);
        let kind = match kind_str.as_str() {
            KIND_STANDARD => NotebookKind::Standard,
            KIND_PLANNER => {
                let template_blob = planner_template_blob.ok_or_else(|| {
                    StorageError::InvalidData("planner missing template_id".into())
                })?;
                let date_str = planner_date_str.ok_or_else(|| {
                    StorageError::InvalidData("planner missing creation_date".into())
                })?;
                let creation_date: NaiveDate = date_str
                    .parse()
                    .map_err(|e: chrono::ParseError| StorageError::InvalidData(e.to_string()))?;
                NotebookKind::Planner {
                    template_id: TemplateId(blob_to_uuid(&template_blob)?),
                    creation_date,
                }
            }
            other => {
                return Err(StorageError::InvalidData(format!(
                    "unknown notebook kind: {}",
                    other
                )))
            }
        };
        let assigned_templates: Vec<TemplateId> = serde_json::from_str(&assigned_json)?;
        Ok(Notebook {
            id,
            name,
            kind,
            assigned_templates,
        })
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use uuid::Uuid;

    fn sample_notebook() -> Notebook {
        Notebook {
            id: NotebookId(Uuid::new_v4()),
            name: "Test".into(),
            kind: NotebookKind::Standard,
            assigned_templates: vec![TemplateId(Uuid::new_v4())],
        }
    }

    #[test]
    fn round_trip_standard() {
        let db = Db::open_in_memory().unwrap();
        let nb = sample_notebook();
        insert_notebook(db.conn(), &nb).unwrap();
        let got = get_notebook(db.conn(), nb.id).unwrap();
        assert_eq!(got, nb);
    }

    #[test]
    fn round_trip_planner() {
        let db = Db::open_in_memory().unwrap();
        let nb = Notebook {
            id: NotebookId(Uuid::new_v4()),
            name: "Planner 2026".into(),
            kind: NotebookKind::Planner {
                template_id: TemplateId(Uuid::new_v4()),
                creation_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            },
            assigned_templates: vec![],
        };
        insert_notebook(db.conn(), &nb).unwrap();
        let got = get_notebook(db.conn(), nb.id).unwrap();
        assert_eq!(got, nb);
    }

    #[test]
    fn list_and_delete() {
        let db = Db::open_in_memory().unwrap();
        let a = sample_notebook();
        let b = sample_notebook();
        insert_notebook(db.conn(), &a).unwrap();
        insert_notebook(db.conn(), &b).unwrap();
        let all = list_notebooks(db.conn()).unwrap();
        assert_eq!(all.len(), 2);
        delete_notebook(db.conn(), a.id).unwrap();
        let after = list_notebooks(db.conn()).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, b.id);
    }

    #[test]
    fn update_changes_name() {
        let db = Db::open_in_memory().unwrap();
        let mut nb = sample_notebook();
        insert_notebook(db.conn(), &nb).unwrap();
        nb.name = "Renamed".into();
        update_notebook(db.conn(), &nb).unwrap();
        let got = get_notebook(db.conn(), nb.id).unwrap();
        assert_eq!(got.name, "Renamed");
    }
}
