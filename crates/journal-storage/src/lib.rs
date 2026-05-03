//! Journal storage layer — SQLite persistence (Phase 2).

mod db;
mod error;
mod schema;
mod stroke_codec;
mod util;

pub mod notebook_store;
pub mod page_store;
pub mod section_store;
pub mod stroke_store;

pub use db::Db;
pub use error::{Result, StorageError};
pub use schema::init_schema;
pub use stroke_codec::{pack_points, unpack_points};
