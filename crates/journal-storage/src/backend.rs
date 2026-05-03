//! Storage trait abstraction (Phase 6.1).
//!
//! The journal app holds an `Rc<RefCell<dyn JournalBackend>>` instead of a
//! concrete SQLite `Db`. Today there's exactly one impl ([`SqliteBackend`])
//! delegating to the existing free functions in `notebook_store`,
//! `section_store`, `page_store`, `stroke_store`. Phase 6.3 will add a
//! second impl backed by AWS Amplify (Cognito + AppSync + DynamoDB + S3)
//! for template — and eventually notebook — sharing.
//!
//! Design notes:
//! - Every method takes `&mut self`. Reads could in principle take `&self`
//!   for the SQLite impl, but a remote impl needs to mutate connection /
//!   token state on most calls, and `Rc<RefCell<dyn _>>` callers always
//!   `borrow_mut` anyway.
//! - Errors flow through [`StorageError`] which already has `Network`,
//!   `Auth`, `Conflict` variants reserved for the remote backend.
//! - The trait surface mirrors the existing free-fn API — same method
//!   names, same arguments, no `Connection` leakage. Migrating call sites
//!   is mechanical.

use chrono::NaiveDate;
use journal_core::{
    Notebook, NotebookId, Page, PageId, PlannerPageAddress, Rect, Section, SectionId, Stroke,
};
use uuid::Uuid;

use crate::error::Result;

pub trait NotebookStore {
    fn insert_notebook(&mut self, notebook: &Notebook) -> Result<()>;
    fn get_notebook(&mut self, id: NotebookId) -> Result<Notebook>;
    fn list_notebooks(&mut self) -> Result<Vec<Notebook>>;
    fn update_notebook(&mut self, notebook: &Notebook) -> Result<()>;
    fn delete_notebook(&mut self, id: NotebookId) -> Result<()>;
}

pub trait SectionStore {
    fn insert_section(&mut self, section: &Section) -> Result<()>;
    fn get_section(&mut self, id: SectionId) -> Result<Section>;
    fn list_sections(&mut self, notebook_id: NotebookId) -> Result<Vec<Section>>;
    fn list_root_sections(&mut self, notebook_id: NotebookId) -> Result<Vec<Section>>;
    fn list_child_sections(&mut self, parent_id: SectionId) -> Result<Vec<Section>>;
    fn update_section(&mut self, section: &Section) -> Result<()>;
    fn delete_section(&mut self, id: SectionId) -> Result<()>;
    fn reorder_section(&mut self, id: SectionId, new_position: u32) -> Result<()>;
    fn ensure_section(
        &mut self,
        notebook_id: NotebookId,
        parent_section_id: Option<SectionId>,
        name: &str,
    ) -> Result<Section>;
}

pub trait PageStore {
    fn insert_page(&mut self, page: &Page) -> Result<()>;
    fn get_page(&mut self, id: PageId) -> Result<Page>;
    fn find_page_by_address(
        &mut self,
        section_id: SectionId,
        addr: &PlannerPageAddress,
    ) -> Result<Option<Page>>;
    fn list_pages(&mut self, section_id: SectionId) -> Result<Vec<Page>>;
    fn update_page(&mut self, page: &Page) -> Result<()>;
    fn delete_page(&mut self, id: PageId) -> Result<()>;
    fn move_page(
        &mut self,
        id: PageId,
        target_section: SectionId,
        target_position: u32,
    ) -> Result<()>;
    fn reorder_page(&mut self, id: PageId, new_position: u32) -> Result<()>;
}

pub trait StrokeStore {
    fn insert_stroke(&mut self, stroke: &Stroke, page_id: PageId) -> Result<()>;
    fn delete_stroke(&mut self, id: Uuid) -> Result<()>;
    fn update_stroke(&mut self, stroke: &Stroke, page_id: PageId) -> Result<()>;
    fn replace_stroke(
        &mut self,
        old_id: Uuid,
        new_strokes: &[Stroke],
        page_id: PageId,
    ) -> Result<()>;
    fn delete_strokes_batch(&mut self, ids: &[Uuid]) -> Result<()>;
    fn list_strokes_for_page(&mut self, page_id: PageId) -> Result<Vec<Stroke>>;
    fn query_strokes_in_rect(&mut self, page_id: PageId, rect: Rect) -> Result<Vec<Stroke>>;
}

/// One-stop trait for the storage layer. App holds `Rc<RefCell<dyn JournalBackend>>`.
pub trait JournalBackend: NotebookStore + SectionStore + PageStore + StrokeStore {}

/// Convenience marker for date-aware planner queries that the future remote
/// backend may want to push down (e.g. "give me all Day-addressed pages
/// between two dates"). Kept separate from the four core traits so it can
/// evolve without breaking impls.
pub trait PlannerQueries: PageStore {
    /// Default impl: list every page in `section_id` and filter client-side.
    /// Remote backends are encouraged to override with a server-side range
    /// query.
    fn pages_in_date_range(
        &mut self,
        section_id: SectionId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Page>> {
        let mut out = self.list_pages(section_id)?;
        out.retain(|p| match p.planner_address {
            Some(PlannerPageAddress::Day { date, .. }) => date >= from && date <= to,
            _ => false,
        });
        Ok(out)
    }
}

impl<T: JournalBackend> PlannerQueries for T {}
