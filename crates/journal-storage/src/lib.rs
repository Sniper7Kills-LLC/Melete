//! Journal storage layer (Phase 2) + trait abstraction over the storage
//! backend (Phase 6.1) so a future remote (AWS Amplify) impl can plug in
//! without app-layer changes.
//!
//! Public surface: the [`backend`] traits + the [`SqliteBackend`] impl + the
//! pure stroke codec. The rusqlite-specific `Db` and the per-store free
//! functions are crate-internal; consumers must go through the trait.

mod db;
mod error;
pub mod fs_atomic;
mod schema;
mod stroke_codec;
mod util;

pub mod backend;
pub mod entitlement;
pub mod multi_file_backend;
pub mod sqlite_backend;
pub mod template_migration;

/// Phase 6.3 remote (AWS Amplify) template store. Speaks Cognito +
/// AppSync over plain HTTPS to keep the dependency footprint small;
/// SigV4 for S3 is hand-rolled on top of `sha2` + `hmac`.
#[cfg(feature = "remote")]
pub mod remote_template_store {
    pub mod auth;
    pub mod config;
    pub mod graphql;
    pub mod identity;
    pub(crate) mod jwt;
    pub mod s3;
    pub mod store;

    /// GraphQL query/mutation strings reused across `store.rs` and
    /// the entitlement service. Kept in one place so the wire shape
    /// is documented in a single file.
    pub const ENTITLEMENT_QUERY: &str = r#"
query GetMyEntitlement($id: ID!) {
  getUserEntitlement(id: $id) {
    id
    tier
    status
    periodEnd
    trialEndsAt
    educationVerified
    notebookCap
    strokesPerPageCap
    strokesPerNotebookCap
    dailyWriteCap
    s3BytesCap
    templatePublishCap
    historyDays
    liveSyncEnabled
  }
}
"#;

    pub const CREATE_CHECKOUT_SESSION_MUTATION: &str = r#"
mutation CreateCheckoutSession($tier: String!, $interval: String!) {
  createCheckoutSession(tier: $tier, interval: $interval) {
    url
  }
}
"#;

    pub const CREATE_PORTAL_SESSION_MUTATION: &str = r#"
mutation CreatePortalSession {
  createPortalSession {
    url
  }
}
"#;
}

/// Phase 6.5 — user-notebook (the actual document) sync. Snapshot
/// upload to S3 + per-stroke live event publishing to AppSync.
/// Reuses the auth / identity / config / graphql / s3 building
/// blocks from `remote_template_store`.
#[cfg(feature = "remote")]
pub mod remote_notebook_store;

// SQLite-specific store modules. Crate-internal: only `SqliteBackend`
// delegates to them. App and library callers must use the trait surface.
pub(crate) mod brush_store;
pub(crate) mod notebook_store;
pub(crate) mod page_store;
pub(crate) mod section_store;
pub(crate) mod stroke_store;
pub(crate) mod template_catalog_store;

pub use backend::{
    AssetBytes, AssetMeta, BrushRow, BrushStore, JournalBackend, NotebookStore, PageStore,
    PlannerQueries, SectionStore, StrokeStore, TemplateRow, TemplateStore,
};
pub use error::{Result, StorageError};
pub use multi_file_backend::{init_index_schema, MultiFileSqliteBackend};
pub use sqlite_backend::SqliteBackend;
pub use stroke_codec::{pack_points, unpack_points};
pub use template_migration::{migrate_if_needed, MigrationPaths};
