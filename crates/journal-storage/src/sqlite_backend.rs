//! SQLite implementation of the [`JournalBackend`] trait family. Wraps the
//! existing `Db` (rusqlite `Connection`) and delegates each method to the
//! corresponding free function in the `*_store` modules.

use std::path::Path;

use journal_core::{
    Notebook, NotebookId, Page, PageId, PlannerPageAddress, Rect, Section, SectionId, Stroke,
};
use uuid::Uuid;

use crate::backend::{JournalBackend, NotebookStore, PageStore, SectionStore, StrokeStore};
use crate::db::Db;
use crate::error::Result;
use crate::{notebook_store, page_store, section_store, stroke_store};

pub struct SqliteBackend {
    db: Db,
}

impl SqliteBackend {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self { db: Db::open(path)? })
    }

    pub fn open_in_memory() -> Result<Self> {
        Ok(Self { db: Db::open_in_memory()? })
    }

    /// Escape hatch — only used by code that hasn't been migrated to traits
    /// yet. Will be removed in a follow-up commit.
    pub fn db_mut(&mut self) -> &mut Db {
        &mut self.db
    }
}

impl NotebookStore for SqliteBackend {
    fn insert_notebook(&mut self, notebook: &Notebook) -> Result<()> {
        notebook_store::insert_notebook(self.db.conn(), notebook)
    }
    fn get_notebook(&mut self, id: NotebookId) -> Result<Notebook> {
        notebook_store::get_notebook(self.db.conn(), id)
    }
    fn list_notebooks(&mut self) -> Result<Vec<Notebook>> {
        notebook_store::list_notebooks(self.db.conn())
    }
    fn update_notebook(&mut self, notebook: &Notebook) -> Result<()> {
        notebook_store::update_notebook(self.db.conn(), notebook)
    }
    fn delete_notebook(&mut self, id: NotebookId) -> Result<()> {
        notebook_store::delete_notebook(self.db.conn(), id)
    }
}

impl SectionStore for SqliteBackend {
    fn insert_section(&mut self, section: &Section) -> Result<()> {
        section_store::insert_section(self.db.conn(), section)
    }
    fn get_section(&mut self, id: SectionId) -> Result<Section> {
        section_store::get_section(self.db.conn(), id)
    }
    fn list_sections(&mut self, notebook_id: NotebookId) -> Result<Vec<Section>> {
        section_store::list_sections(self.db.conn(), notebook_id)
    }
    fn list_root_sections(&mut self, notebook_id: NotebookId) -> Result<Vec<Section>> {
        section_store::list_root_sections(self.db.conn(), notebook_id)
    }
    fn list_child_sections(&mut self, parent_id: SectionId) -> Result<Vec<Section>> {
        section_store::list_child_sections(self.db.conn(), parent_id)
    }
    fn update_section(&mut self, section: &Section) -> Result<()> {
        section_store::update_section(self.db.conn(), section)
    }
    fn delete_section(&mut self, id: SectionId) -> Result<()> {
        section_store::delete_section(self.db.conn(), id)
    }
    fn reorder_section(&mut self, id: SectionId, new_position: u32) -> Result<()> {
        section_store::reorder_section(self.db.conn_mut(), id, new_position)
    }
    fn ensure_section(
        &mut self,
        notebook_id: NotebookId,
        parent_section_id: Option<SectionId>,
        name: &str,
    ) -> Result<Section> {
        section_store::ensure_section(self.db.conn_mut(), notebook_id, parent_section_id, name)
    }
}

impl PageStore for SqliteBackend {
    fn insert_page(&mut self, page: &Page) -> Result<()> {
        page_store::insert_page(self.db.conn(), page)
    }
    fn get_page(&mut self, id: PageId) -> Result<Page> {
        page_store::get_page(self.db.conn(), id)
    }
    fn find_page_by_address(
        &mut self,
        section_id: SectionId,
        addr: &PlannerPageAddress,
    ) -> Result<Option<Page>> {
        page_store::find_page_by_address(self.db.conn(), section_id, addr)
    }
    fn list_pages(&mut self, section_id: SectionId) -> Result<Vec<Page>> {
        page_store::list_pages(self.db.conn(), section_id)
    }
    fn update_page(&mut self, page: &Page) -> Result<()> {
        page_store::update_page(self.db.conn(), page)
    }
    fn delete_page(&mut self, id: PageId) -> Result<()> {
        page_store::delete_page(self.db.conn(), id)
    }
    fn move_page(
        &mut self,
        id: PageId,
        target_section: SectionId,
        target_position: u32,
    ) -> Result<()> {
        page_store::move_page(self.db.conn_mut(), id, target_section, target_position)
    }
    fn reorder_page(&mut self, id: PageId, new_position: u32) -> Result<()> {
        page_store::reorder_page(self.db.conn_mut(), id, new_position)
    }
}

impl StrokeStore for SqliteBackend {
    fn insert_stroke(&mut self, stroke: &Stroke, page_id: PageId) -> Result<()> {
        stroke_store::insert_stroke(self.db.conn(), stroke, page_id)
    }
    fn delete_stroke(&mut self, id: Uuid) -> Result<()> {
        stroke_store::delete_stroke(self.db.conn(), id)
    }
    fn update_stroke(&mut self, stroke: &Stroke, page_id: PageId) -> Result<()> {
        stroke_store::update_stroke(self.db.conn(), stroke, page_id)
    }
    fn replace_stroke(
        &mut self,
        old_id: Uuid,
        new_strokes: &[Stroke],
        page_id: PageId,
    ) -> Result<()> {
        stroke_store::replace_stroke(self.db.conn(), old_id, new_strokes, page_id)
    }
    fn delete_strokes_batch(&mut self, ids: &[Uuid]) -> Result<()> {
        stroke_store::delete_strokes_batch(self.db.conn(), ids)
    }
    fn list_strokes_for_page(&mut self, page_id: PageId) -> Result<Vec<Stroke>> {
        stroke_store::list_strokes_for_page(self.db.conn(), page_id)
    }
    fn query_strokes_in_rect(&mut self, page_id: PageId, rect: Rect) -> Result<Vec<Stroke>> {
        stroke_store::query_strokes_in_rect(self.db.conn(), page_id, rect)
    }
}

impl JournalBackend for SqliteBackend {}
