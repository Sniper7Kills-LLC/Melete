//! File-per-notebook SQLite backend (Phase 6.2).
//!
//! On-disk layout:
//! ```text
//! ~/.local/share/journal/
//!   index.db                       -- catalog of notebooks (id, name, file_path)
//!   journals/{id}.journal          -- one self-contained SQLite file per notebook
//!   journal.db.legacy              -- safety backup of the pre-migration db
//! ```
//!
//! Every `.journal` file contains the FULL schema (notebooks, sections, pages,
//! strokes, strokes_rtree) but only one notebook's row in `notebooks` — making
//! it copy-pastable / shareable as a single artifact.
//!
//! Routing: trait methods that already carry `notebook_id` (via Section.id /
//! Page.section_id) route directly. Methods that take a bare `SectionId` or
//! `PageId` look the owning notebook up in a per-process cache, falling back
//! to a one-time scan of all `.journal` files when the cache misses.
//!
//! Migration: on the first `open()` call after upgrade, if the legacy
//! `journal.db` exists and has notebooks while `index.db` does not, the
//! contents are split into per-notebook files and the legacy file is renamed
//! `journal.db.legacy`. The migration is idempotent — subsequent boots just
//! see the index already populated.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use journal_core::{
    Notebook, NotebookId, Page, PageId, PlannerPageAddress, Rect, Section, SectionId, Stroke,
};

use crate::backend::{
    AssetBytes, AssetMeta, BrushRow, BrushStore, JournalBackend, NotebookStore, PageStore,
    SectionStore, StrokeStore, TemplateRow, TemplateStore,
};
use crate::error::{Result, StorageError};
use crate::schema::init_schema;
use crate::template_migration::{migrate_if_needed, MigrationPaths};
use crate::util::{blob_to_uuid, uuid_to_blob};
use crate::{
    brush_store, notebook_store, page_store, section_store, stroke_store, template_catalog_store,
};

const INDEX_FILE: &str = "index.db";
const JOURNALS_DIR: &str = "journals";
const LEGACY_NAME: &str = "journal.db";
const LEGACY_RENAMED: &str = "journal.db.legacy";

pub struct MultiFileSqliteBackend {
    root: PathBuf,
    index: Connection,
    notebooks: RefCell<HashMap<NotebookId, Connection>>,
    section_to_notebook: RefCell<HashMap<SectionId, NotebookId>>,
    page_to_notebook: RefCell<HashMap<PageId, NotebookId>>,
    stroke_to_notebook: RefCell<HashMap<Uuid, NotebookId>>,
}

impl MultiFileSqliteBackend {
    /// Open (or create) a multi-file backend rooted at `root`. If a legacy
    /// single-file `journal.db` exists in `root` and `index.db` does not yet,
    /// the legacy file is split into per-notebook files before returning.
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_with_migration_paths(root, MigrationPaths::xdg_default())
    }

    /// Like [`Self::open`] but lets the caller (typically tests)
    /// inject the legacy filesystem layout the template/brush
    /// migration walks. Production callers should use [`Self::open`],
    /// which defaults to the XDG layout.
    pub fn open_with_migration_paths(
        root: &Path,
        migration_paths: Option<MigrationPaths>,
    ) -> Result<Self> {
        std::fs::create_dir_all(root.join(JOURNALS_DIR))
            .map_err(|e| StorageError::InvalidData(format!("create journals dir: {}", e)))?;

        let index_path = root.join(INDEX_FILE);
        let need_migrate = !index_path.exists() && root.join(LEGACY_NAME).exists();

        let mut index = Connection::open(&index_path)?;
        configure(&index)?;
        index.execute_batch(INDEX_SCHEMA)?;
        init_index_schema(&mut index)?;

        if let Some(paths) = migration_paths.as_ref() {
            migrate_if_needed(&mut index, paths)?;
        }

        let backend = Self {
            root: root.to_path_buf(),
            index,
            notebooks: RefCell::new(HashMap::new()),
            section_to_notebook: RefCell::new(HashMap::new()),
            page_to_notebook: RefCell::new(HashMap::new()),
            stroke_to_notebook: RefCell::new(HashMap::new()),
        };

        if need_migrate {
            backend.migrate_from_legacy(&root.join(LEGACY_NAME))?;
        }

        Ok(backend)
    }

    fn journal_path(&self, id: NotebookId) -> PathBuf {
        self.root
            .join(JOURNALS_DIR)
            .join(format!("{}.journal", id.0))
    }

    /// Get (lazily opening) the connection for a notebook. Errors if the
    /// notebook isn't registered in `index.db`.
    fn ensure_open(&self, id: NotebookId) -> Result<()> {
        if self.notebooks.borrow().contains_key(&id) {
            return Ok(());
        }
        let path = self.journal_path(id);
        if !path.exists() {
            return Err(StorageError::NotFound);
        }
        let conn = Connection::open(&path)?;
        configure(&conn)?;
        init_schema(&conn)?;
        self.notebooks.borrow_mut().insert(id, conn);
        Ok(())
    }

    fn with_conn<F, T>(&self, id: NotebookId, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        self.ensure_open(id)?;
        let map = self.notebooks.borrow();
        let conn = map.get(&id).ok_or(StorageError::NotFound)?;
        f(conn)
    }

    fn with_conn_mut<F, T>(&self, id: NotebookId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> Result<T>,
    {
        self.ensure_open(id)?;
        let mut map = self.notebooks.borrow_mut();
        let conn = map.get_mut(&id).ok_or(StorageError::NotFound)?;
        f(conn)
    }

    /// All registered notebook ids.
    fn list_index(&self) -> Result<Vec<NotebookId>> {
        let mut stmt = self
            .index
            .prepare("SELECT id FROM notebook_index ORDER BY created_at ASC")?;
        let rows = stmt.query_map([], |r| {
            let blob: Vec<u8> = r.get(0)?;
            Ok(blob)
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(NotebookId(blob_to_uuid(&r?)?));
        }
        Ok(out)
    }

    /// Insert into the index after a notebook file has been created.
    fn index_insert(&self, id: NotebookId, name: &str, path: &Path) -> Result<()> {
        let created_at = chrono::Utc::now().to_rfc3339();
        self.index.execute(
            "INSERT INTO notebook_index (id, name, file_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                uuid_to_blob(id.0),
                name,
                path.to_string_lossy().to_string(),
                created_at
            ],
        )?;
        Ok(())
    }

    fn index_update_name(&self, id: NotebookId, name: &str) -> Result<()> {
        self.index.execute(
            "UPDATE notebook_index SET name = ?2 WHERE id = ?1",
            params![uuid_to_blob(id.0), name],
        )?;
        Ok(())
    }

    fn index_delete(&self, id: NotebookId) -> Result<()> {
        self.index.execute(
            "DELETE FROM notebook_index WHERE id = ?1",
            params![uuid_to_blob(id.0)],
        )?;
        Ok(())
    }

    /// Resolve `SectionId` → `NotebookId`. Cache, then scan all known
    /// notebook dbs (opening them if needed) on miss.
    fn notebook_for_section(&self, section_id: SectionId) -> Result<NotebookId> {
        if let Some(nid) = self.section_to_notebook.borrow().get(&section_id).copied() {
            return Ok(nid);
        }
        for nid in self.list_index()? {
            self.ensure_open(nid)?;
            let found = self.with_conn(nid, |c| {
                section_store::get_section(c, section_id)
                    .map(Some)
                    .or_else(|e| match e {
                        StorageError::NotFound => Ok(None),
                        other => Err(other),
                    })
            })?;
            if let Some(s) = found {
                let owner = NotebookId(s.notebook_id.0);
                self.section_to_notebook
                    .borrow_mut()
                    .insert(section_id, owner);
                return Ok(owner);
            }
        }
        Err(StorageError::NotFound)
    }

    /// Resolve `PageId` → `NotebookId` via the same cache + scan strategy.
    fn notebook_for_page(&self, page_id: PageId) -> Result<NotebookId> {
        if let Some(nid) = self.page_to_notebook.borrow().get(&page_id).copied() {
            return Ok(nid);
        }
        for nid in self.list_index()? {
            self.ensure_open(nid)?;
            let found = self.with_conn(nid, |c| {
                page_store::get_page(c, page_id)
                    .map(Some)
                    .or_else(|e| match e {
                        StorageError::NotFound => Ok(None),
                        other => Err(other),
                    })
            })?;
            if let Some(p) = found {
                self.page_to_notebook.borrow_mut().insert(page_id, nid);
                self.section_to_notebook
                    .borrow_mut()
                    .insert(p.section_id, nid);
                return Ok(nid);
            }
        }
        Err(StorageError::NotFound)
    }

    fn notebook_for_stroke(&self, stroke_id: Uuid) -> Result<NotebookId> {
        if let Some(nid) = self.stroke_to_notebook.borrow().get(&stroke_id).copied() {
            return Ok(nid);
        }
        for nid in self.list_index()? {
            self.ensure_open(nid)?;
            let owns = self.with_conn(nid, |c| {
                let owns: bool = c
                    .query_row(
                        "SELECT 1 FROM strokes WHERE id = ?1 LIMIT 1",
                        params![uuid_to_blob(stroke_id)],
                        |_r| Ok(true),
                    )
                    .optional()?
                    .unwrap_or(false);
                Ok(owns)
            })?;
            if owns {
                self.stroke_to_notebook.borrow_mut().insert(stroke_id, nid);
                return Ok(nid);
            }
        }
        Err(StorageError::NotFound)
    }

    fn migrate_from_legacy(&self, legacy_path: &Path) -> Result<()> {
        tracing::info!(
            "Migrating legacy {} into per-notebook files",
            legacy_path.display()
        );
        let legacy = Connection::open(legacy_path)?;
        configure(&legacy)?;
        init_schema(&legacy)?;

        let nb_ids = notebook_store::list_notebooks(&legacy)?;
        for nb in &nb_ids {
            let new_path = self.journal_path(nb.id);
            if new_path.exists() {
                continue;
            }
            let new_db = Connection::open(&new_path)?;
            configure(&new_db)?;
            init_schema(&new_db)?;

            // ATTACH the legacy db, copy this notebook's slice, DETACH.
            let attach_sql = format!(
                "ATTACH DATABASE '{}' AS src",
                legacy_path.to_string_lossy().replace('\'', "''"),
            );
            new_db.execute_batch(&attach_sql)?;
            let nb_blob = uuid_to_blob(nb.id.0);
            // Notebooks
            new_db.execute(
                "INSERT INTO notebooks SELECT * FROM src.notebooks WHERE id = ?1",
                params![nb_blob],
            )?;
            // Sections (recursive — notebook_id matches directly)
            new_db.execute(
                "INSERT INTO sections SELECT * FROM src.sections WHERE notebook_id = ?1",
                params![nb_blob],
            )?;
            // Pages (via section→notebook)
            new_db.execute(
                "INSERT INTO pages SELECT * FROM src.pages
                 WHERE section_id IN (SELECT id FROM src.sections WHERE notebook_id = ?1)",
                params![nb_blob],
            )?;
            // Strokes (via page→section→notebook)
            new_db.execute(
                "INSERT INTO strokes SELECT * FROM src.strokes
                 WHERE page_id IN (
                    SELECT id FROM src.pages
                    WHERE section_id IN (SELECT id FROM src.sections WHERE notebook_id = ?1)
                 )",
                params![nb_blob],
            )?;
            // Rebuild rtree for the migrated strokes (strokes_rtree is by rowid).
            new_db.execute(
                "INSERT INTO strokes_rtree(id, min_x, max_x, min_y, max_y)
                 SELECT s.rowid, s.bbox_x, s.bbox_x + s.bbox_w, s.bbox_y, s.bbox_y + s.bbox_h
                 FROM strokes s",
                [],
            )?;
            new_db.execute_batch("DETACH DATABASE src")?;

            self.index_insert(nb.id, &nb.name, &new_path)?;
            tracing::info!(
                "Migrated notebook {} ({}) → {}",
                nb.id.0,
                nb.name,
                new_path.display()
            );
        }
        drop(legacy);

        let renamed = legacy_path.with_file_name(LEGACY_RENAMED);
        if let Err(e) = std::fs::rename(legacy_path, &renamed) {
            tracing::warn!("rename legacy db to {}: {}", renamed.display(), e);
        }
        Ok(())
    }
}

const INDEX_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS notebook_index (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    created_at TEXT NOT NULL
);";

// Phase 6.3 catalog schema. Lives in `index.db` next to the existing
// `notebook_index` so brushes / page templates / notebook templates /
// page-template assets are addressable independent of any one
// notebook file. `user_version` on the catalog connection tracks
// these adds (notebook-content schema stays under each
// `*.journal` file's own version). Bumping `INDEX_USER_VERSION`
// adds a new arm here — never edit existing arms in place.
//
// `updated_at_sort` mirrors the wire shape the future Amplify
// (AppSync / DynamoDB) backend will store: Amplify Gen 2 rejects
// auto-managed `updatedAt` as a GSI sort key, so the model carries
// an explicit RFC3339 string we control. Keeping the local schema in
// lockstep means the Amplify impl can map the row → DynamoDB item
// 1:1 without a translation layer.
const INDEX_TEMPLATE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS brushes (
    id              BLOB PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    body_toml       TEXT NOT NULL,
    sha256          TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    updated_at_sort TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS page_templates (
    id              BLOB PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    category        TEXT NOT NULL DEFAULT '',
    body_toml       TEXT NOT NULL,
    sha256          TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    updated_at_sort TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS notebook_templates (
    id              BLOB PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    body_toml       TEXT NOT NULL,
    sha256          TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    updated_at_sort TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS page_template_assets (
    template_id BLOB NOT NULL REFERENCES page_templates(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    mime        TEXT NOT NULL,
    sha256      TEXT NOT NULL,
    bytes       BLOB NOT NULL,
    PRIMARY KEY (template_id, name)
);";

const INDEX_USER_VERSION: u32 = 1;

/// Idempotent schema ladder for the `index.db` catalog. Caller has
/// already executed `INDEX_SCHEMA`; this adds the brush / template
/// tables on top. Tracked separately from each notebook's
/// `user_version` because the catalog evolves independently of
/// notebook content.
pub fn init_index_schema(conn: &mut Connection) -> Result<()> {
    let v: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if v < 1 {
        conn.execute_batch(INDEX_TEMPLATE_SCHEMA)?;
        conn.pragma_update(None, "user_version", INDEX_USER_VERSION)?;
    }
    Ok(())
}

fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

// ──────────────────────────── trait impls ────────────────────────────

impl NotebookStore for MultiFileSqliteBackend {
    fn insert_notebook(&mut self, notebook: &Notebook) -> Result<()> {
        let path = self.journal_path(notebook.id);
        if path.exists() {
            return Err(StorageError::InvalidData(format!(
                "notebook file already exists: {}",
                path.display()
            )));
        }
        let conn = Connection::open(&path)?;
        configure(&conn)?;
        init_schema(&conn)?;
        notebook_store::insert_notebook(&conn, notebook)?;
        self.notebooks.borrow_mut().insert(notebook.id, conn);
        self.index_insert(notebook.id, &notebook.name, &path)?;
        Ok(())
    }

    fn get_notebook(&mut self, id: NotebookId) -> Result<Notebook> {
        self.with_conn(id, |c| notebook_store::get_notebook(c, id))
    }

    fn list_notebooks(&mut self) -> Result<Vec<Notebook>> {
        let ids = self.list_index()?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.get_notebook(id) {
                Ok(nb) => out.push(nb),
                Err(StorageError::NotFound) => {
                    tracing::warn!("index references missing notebook file: {}", id.0);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    fn update_notebook(&mut self, notebook: &Notebook) -> Result<()> {
        self.with_conn(notebook.id, |c| {
            notebook_store::update_notebook(c, notebook)
        })?;
        self.index_update_name(notebook.id, &notebook.name)?;
        Ok(())
    }

    fn delete_notebook(&mut self, id: NotebookId) -> Result<()> {
        // Drop the in-memory connection so we can remove the file.
        self.notebooks.borrow_mut().remove(&id);
        let path = self.journal_path(id);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                StorageError::InvalidData(format!("delete {}: {}", path.display(), e))
            })?;
        }
        self.index_delete(id)?;
        // Invalidate caches scoped to this notebook.
        self.section_to_notebook
            .borrow_mut()
            .retain(|_, v| *v != id);
        self.page_to_notebook.borrow_mut().retain(|_, v| *v != id);
        self.stroke_to_notebook.borrow_mut().retain(|_, v| *v != id);
        Ok(())
    }
}

impl SectionStore for MultiFileSqliteBackend {
    fn insert_section(&mut self, section: &Section) -> Result<()> {
        self.with_conn(section.notebook_id, |c| {
            section_store::insert_section(c, section)
        })?;
        self.section_to_notebook
            .borrow_mut()
            .insert(section.id, section.notebook_id);
        Ok(())
    }

    fn get_section(&mut self, id: SectionId) -> Result<Section> {
        let nid = self.notebook_for_section(id)?;
        self.with_conn(nid, |c| section_store::get_section(c, id))
    }

    fn list_sections(&mut self, notebook_id: NotebookId) -> Result<Vec<Section>> {
        let v = self.with_conn(notebook_id, |c| {
            section_store::list_sections(c, notebook_id)
        })?;
        let mut cache = self.section_to_notebook.borrow_mut();
        for s in &v {
            cache.insert(s.id, notebook_id);
        }
        Ok(v)
    }

    fn list_root_sections(&mut self, notebook_id: NotebookId) -> Result<Vec<Section>> {
        let v = self.with_conn(notebook_id, |c| {
            section_store::list_root_sections(c, notebook_id)
        })?;
        let mut cache = self.section_to_notebook.borrow_mut();
        for s in &v {
            cache.insert(s.id, notebook_id);
        }
        Ok(v)
    }

    fn list_child_sections(&mut self, parent_id: SectionId) -> Result<Vec<Section>> {
        let nid = self.notebook_for_section(parent_id)?;
        let v = self.with_conn(nid, |c| section_store::list_child_sections(c, parent_id))?;
        let mut cache = self.section_to_notebook.borrow_mut();
        for s in &v {
            cache.insert(s.id, nid);
        }
        Ok(v)
    }

    fn update_section(&mut self, section: &Section) -> Result<()> {
        self.with_conn(section.notebook_id, |c| {
            section_store::update_section(c, section)
        })
    }

    fn delete_section(&mut self, id: SectionId) -> Result<()> {
        let nid = self.notebook_for_section(id)?;
        self.with_conn(nid, |c| section_store::delete_section(c, id))?;
        self.section_to_notebook.borrow_mut().remove(&id);
        Ok(())
    }

    fn reorder_section(&mut self, id: SectionId, new_position: u32) -> Result<()> {
        let nid = self.notebook_for_section(id)?;
        self.with_conn_mut(nid, |c| section_store::reorder_section(c, id, new_position))
    }

    fn ensure_section(
        &mut self,
        notebook_id: NotebookId,
        parent_section_id: Option<SectionId>,
        name: &str,
    ) -> Result<Section> {
        let s = self.with_conn_mut(notebook_id, |c| {
            section_store::ensure_section(c, notebook_id, parent_section_id, name)
        })?;
        self.section_to_notebook
            .borrow_mut()
            .insert(s.id, notebook_id);
        Ok(s)
    }
}

impl PageStore for MultiFileSqliteBackend {
    fn insert_page(&mut self, page: &Page) -> Result<()> {
        let nid = self.notebook_for_section(page.section_id)?;
        self.with_conn(nid, |c| page_store::insert_page(c, page))?;
        self.page_to_notebook.borrow_mut().insert(page.id, nid);
        Ok(())
    }

    fn get_page(&mut self, id: PageId) -> Result<Page> {
        let nid = self.notebook_for_page(id)?;
        self.with_conn(nid, |c| page_store::get_page(c, id))
    }

    fn find_page_by_address(
        &mut self,
        section_id: SectionId,
        addr: &PlannerPageAddress,
    ) -> Result<Option<Page>> {
        let nid = self.notebook_for_section(section_id)?;
        let p = self.with_conn(nid, |c| {
            page_store::find_page_by_address(c, section_id, addr)
        })?;
        if let Some(ref p) = p {
            self.page_to_notebook.borrow_mut().insert(p.id, nid);
        }
        Ok(p)
    }

    fn list_pages(&mut self, section_id: SectionId) -> Result<Vec<Page>> {
        let nid = self.notebook_for_section(section_id)?;
        let v = self.with_conn(nid, |c| page_store::list_pages(c, section_id))?;
        let mut cache = self.page_to_notebook.borrow_mut();
        for p in &v {
            cache.insert(p.id, nid);
        }
        Ok(v)
    }

    fn update_page(&mut self, page: &Page) -> Result<()> {
        let nid = self.notebook_for_section(page.section_id)?;
        self.with_conn(nid, |c| page_store::update_page(c, page))
    }

    fn delete_page(&mut self, id: PageId) -> Result<()> {
        let nid = self.notebook_for_page(id)?;
        self.with_conn(nid, |c| page_store::delete_page(c, id))?;
        self.page_to_notebook.borrow_mut().remove(&id);
        Ok(())
    }

    fn move_page(
        &mut self,
        id: PageId,
        target_section: SectionId,
        target_position: u32,
    ) -> Result<()> {
        let src = self.notebook_for_page(id)?;
        let dst = self.notebook_for_section(target_section)?;
        if src != dst {
            return Err(StorageError::InvalidData(
                "moving pages across notebooks is not supported in file-per-notebook mode".into(),
            ));
        }
        self.with_conn_mut(src, |c| {
            page_store::move_page(c, id, target_section, target_position)
        })
    }

    fn reorder_page(&mut self, id: PageId, new_position: u32) -> Result<()> {
        let nid = self.notebook_for_page(id)?;
        self.with_conn_mut(nid, |c| page_store::reorder_page(c, id, new_position))
    }
}

impl StrokeStore for MultiFileSqliteBackend {
    fn insert_stroke(&mut self, stroke: &Stroke, page_id: PageId) -> Result<()> {
        let nid = self.notebook_for_page(page_id)?;
        self.with_conn(nid, |c| stroke_store::insert_stroke(c, stroke, page_id))?;
        self.stroke_to_notebook.borrow_mut().insert(stroke.id, nid);
        Ok(())
    }

    fn delete_stroke(&mut self, id: Uuid) -> Result<()> {
        let nid = self.notebook_for_stroke(id)?;
        self.with_conn(nid, |c| stroke_store::delete_stroke(c, id))?;
        // Keep the stroke_to_notebook mapping — soft-delete leaves
        // the row in place so we can later push the delete to cloud
        // and `purge_deleted_stroke` once the cloud confirms.
        Ok(())
    }

    fn update_stroke(&mut self, stroke: &Stroke, page_id: PageId) -> Result<()> {
        let nid = self.notebook_for_page(page_id)?;
        self.with_conn(nid, |c| stroke_store::update_stroke(c, stroke, page_id))
    }

    fn replace_stroke(
        &mut self,
        old_id: Uuid,
        new_strokes: &[Stroke],
        page_id: PageId,
    ) -> Result<()> {
        let nid = self.notebook_for_page(page_id)?;
        self.with_conn_mut(nid, |c| {
            stroke_store::replace_stroke(c, old_id, new_strokes, page_id)
        })?;
        let mut cache = self.stroke_to_notebook.borrow_mut();
        cache.remove(&old_id);
        for s in new_strokes {
            cache.insert(s.id, nid);
        }
        Ok(())
    }

    fn delete_strokes_batch(&mut self, ids: &[Uuid]) -> Result<()> {
        // Group ids by owning notebook so we issue one batch per file.
        let mut by_nb: HashMap<NotebookId, Vec<Uuid>> = HashMap::new();
        for id in ids {
            match self.notebook_for_stroke(*id) {
                Ok(nid) => by_nb.entry(nid).or_default().push(*id),
                Err(StorageError::NotFound) => {
                    tracing::warn!("delete_strokes_batch: unknown stroke {}", id);
                }
                Err(e) => return Err(e),
            }
        }
        for (nid, group) in by_nb {
            self.with_conn(nid, |c| stroke_store::delete_strokes_batch(c, &group))?;
            // Soft-delete: keep cache entries so later cloud-sync /
            // purge can find the owning notebook.
        }
        Ok(())
    }

    fn list_strokes_for_page(&mut self, page_id: PageId) -> Result<Vec<Stroke>> {
        let nid = self.notebook_for_page(page_id)?;
        let v = self.with_conn(nid, |c| stroke_store::list_strokes_for_page(c, page_id))?;
        let mut cache = self.stroke_to_notebook.borrow_mut();
        for s in &v {
            cache.insert(s.id, nid);
        }
        Ok(v)
    }

    fn query_strokes_in_rect(&mut self, page_id: PageId, rect: Rect) -> Result<Vec<Stroke>> {
        let nid = self.notebook_for_page(page_id)?;
        self.with_conn(nid, |c| {
            stroke_store::query_strokes_in_rect(c, page_id, rect)
        })
    }

    fn list_deleted_strokes(
        &mut self,
        notebook_id: NotebookId,
    ) -> Result<Vec<(Uuid, String)>> {
        self.with_conn(notebook_id, stroke_store::list_deleted)
    }

    fn purge_deleted_stroke(&mut self, id: Uuid) -> Result<()> {
        let nid = self.notebook_for_stroke(id)?;
        self.with_conn(nid, |c| stroke_store::purge_deleted(c, id))?;
        self.stroke_to_notebook.borrow_mut().remove(&id);
        Ok(())
    }

    fn is_stroke_deleted(&mut self, id: Uuid) -> Result<bool> {
        let nid = match self.notebook_for_stroke(id) {
            Ok(n) => n,
            Err(StorageError::NotFound) => return Ok(false),
            Err(e) => return Err(e),
        };
        self.with_conn(nid, |c| stroke_store::is_deleted(c, id))
    }
}

impl BrushStore for MultiFileSqliteBackend {
    fn list_brushes(&mut self) -> Result<Vec<BrushRow>> {
        brush_store::list_brushes(&self.index)
    }

    fn get_brush(&mut self, id: Uuid) -> Result<BrushRow> {
        brush_store::get_brush(&self.index, id)
    }

    fn put_brush(&mut self, row: &BrushRow) -> Result<()> {
        brush_store::put_brush(&self.index, row)
    }

    fn delete_brush(&mut self, id: Uuid) -> Result<()> {
        brush_store::delete_brush(&self.index, id)
    }
}

impl TemplateStore for MultiFileSqliteBackend {
    fn list_page_templates(&mut self) -> Result<Vec<TemplateRow>> {
        template_catalog_store::list_page_templates(&self.index)
    }

    fn get_page_template(&mut self, id: Uuid) -> Result<TemplateRow> {
        template_catalog_store::get_page_template(&self.index, id)
    }

    fn put_page_template(&mut self, row: &TemplateRow, assets: &[AssetBytes]) -> Result<()> {
        template_catalog_store::put_page_template(&mut self.index, row, assets)
    }

    fn delete_page_template(&mut self, id: Uuid) -> Result<()> {
        template_catalog_store::delete_page_template(&self.index, id)
    }

    fn list_page_template_assets(&mut self, template_id: Uuid) -> Result<Vec<AssetMeta>> {
        template_catalog_store::list_page_template_assets(&self.index, template_id)
    }

    fn get_page_template_asset(
        &mut self,
        template_id: Uuid,
        name: &str,
    ) -> Result<Option<AssetBytes>> {
        template_catalog_store::get_page_template_asset(&self.index, template_id, name)
    }

    fn list_notebook_templates(&mut self) -> Result<Vec<TemplateRow>> {
        template_catalog_store::list_notebook_templates(&self.index)
    }

    fn get_notebook_template(&mut self, id: Uuid) -> Result<TemplateRow> {
        template_catalog_store::get_notebook_template(&self.index, id)
    }

    fn put_notebook_template(&mut self, row: &TemplateRow) -> Result<()> {
        template_catalog_store::put_notebook_template(&self.index, row)
    }

    fn delete_notebook_template(&mut self, id: Uuid) -> Result<()> {
        template_catalog_store::delete_notebook_template(&self.index, id)
    }
}

impl JournalBackend for MultiFileSqliteBackend {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use journal_core::{Notebook, NotebookKind, Page, PageId, Section, SectionId};
    use uuid::Uuid;

    fn tmpdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn mk_notebook() -> Notebook {
        Notebook {
            id: NotebookId(Uuid::new_v4()),
            name: "n".into(),
            kind: NotebookKind::Standard,
            assigned_templates: Vec::new(),
        }
    }

    fn mk_section(nid: NotebookId, position: u32, name: &str) -> Section {
        Section {
            id: SectionId(Uuid::new_v4()),
            notebook_id: nid,
            name: name.into(),
            position,
            allowed_templates: None,
            parent_section_id: None,
        }
    }

    fn mk_page(sid: SectionId, position: u32) -> Page {
        let now = Utc::now();
        Page {
            id: PageId(Uuid::new_v4()),
            section_id: sid,
            position,
            template_id: None,
            planner_address: None,
            created_at: now,
            modified_at: now,
            name: String::new(),
            widget_overrides: Default::default(),
            widget_data: Default::default(),
            flagged: false,
            bookmark_position: 0,
        }
    }

    #[test]
    fn insert_then_list_round_trip() {
        let dir = tmpdir();
        let mut be = MultiFileSqliteBackend::open(dir.path()).unwrap();
        let nb = mk_notebook();
        be.insert_notebook(&nb).unwrap();
        let listed = be.list_notebooks().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, nb.id);
        // File exists on disk.
        let path = dir
            .path()
            .join(JOURNALS_DIR)
            .join(format!("{}.journal", nb.id.0));
        assert!(path.exists());
    }

    #[test]
    fn section_and_page_routing() {
        let dir = tmpdir();
        let mut be = MultiFileSqliteBackend::open(dir.path()).unwrap();
        let nb = mk_notebook();
        be.insert_notebook(&nb).unwrap();
        let sec = mk_section(nb.id, 0, "S");
        be.insert_section(&sec).unwrap();
        let page = mk_page(sec.id, 0);
        be.insert_page(&page).unwrap();

        // Cold lookup by id should resolve via the cache populated on insert.
        let got = be.get_page(page.id).unwrap();
        assert_eq!(got.id, page.id);
        assert_eq!(be.list_pages(sec.id).unwrap().len(), 1);
    }

    #[test]
    fn delete_notebook_removes_file_and_index() {
        let dir = tmpdir();
        let mut be = MultiFileSqliteBackend::open(dir.path()).unwrap();
        let nb = mk_notebook();
        be.insert_notebook(&nb).unwrap();
        be.delete_notebook(nb.id).unwrap();
        assert!(be.list_notebooks().unwrap().is_empty());
        let path = dir
            .path()
            .join(JOURNALS_DIR)
            .join(format!("{}.journal", nb.id.0));
        assert!(!path.exists());
    }
}
