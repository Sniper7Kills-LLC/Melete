//! Journal storage layer (Phase 2) + trait abstraction over the storage
//! backend (Phase 6.1) so a future remote (AWS Amplify) impl can plug in
//! without app-layer changes.
//!
//! Public surface: the [`backend`] traits + the [`SqliteBackend`] impl + the
//! pure stroke codec. The rusqlite-specific `Db` and the per-store free
//! functions are crate-internal; consumers must go through the trait.

mod db;
mod error;
mod schema;
mod stroke_codec;
mod util;

pub mod backend;
pub mod sqlite_backend;

// SQLite-specific store modules. Crate-internal: only `SqliteBackend`
// delegates to them. App and library callers must use the trait surface.
pub(crate) mod notebook_store;
pub(crate) mod page_store;
pub(crate) mod section_store;
pub(crate) mod stroke_store;

pub use backend::{
    JournalBackend, NotebookStore, PageStore, PlannerQueries, SectionStore, StrokeStore,
};
pub use error::{Result, StorageError};
pub use sqlite_backend::SqliteBackend;
pub use stroke_codec::{pack_points, unpack_points};
