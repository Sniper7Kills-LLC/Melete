//! Journal storage layer — SQLite persistence (Phase 2) + trait abstraction
//! over the storage backend (Phase 6.1) so a future remote (Amplify) impl
//! can plug in without app-layer changes.

mod db;
mod error;
mod schema;
mod stroke_codec;
mod util;

pub mod backend;
pub mod notebook_store;
pub mod page_store;
pub mod section_store;
pub mod sqlite_backend;
pub mod stroke_store;

pub use backend::{JournalBackend, NotebookStore, PageStore, PlannerQueries, SectionStore, StrokeStore};
pub use db::Db;
pub use error::{Result, StorageError};
pub use schema::init_schema;
pub use sqlite_backend::SqliteBackend;
pub use stroke_codec::{pack_points, unpack_points};
