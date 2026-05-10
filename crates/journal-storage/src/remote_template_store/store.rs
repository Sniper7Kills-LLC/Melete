//! `RemoteTemplateStore` — Rust client for the Amplify-backed
//! template + brush catalog.
//!
//! Composes the four building blocks in this module:
//!   * [`super::auth`] for User-Pool sign-in / token refresh
//!   * [`super::identity`] for Identity-Pool credential exchange
//!   * [`super::graphql`] for AppSync queries / mutations
//!   * [`super::s3`] for SigV4-signed asset GET / PUT (private)
//!     plus presigned-URL PUT for published assets via the
//!     `getAssetUploadUrl` Lambda
//!
//! The trait surface ([`RemoteTemplateOps`]) is the seam the desktop
//! app + the seed-publish CLI consume. Both call sites live behind
//! the `remote` feature so the wasm web-viewer bundle stays free of
//! reqwest / hmac / chrono.

use thiserror::Error;
use uuid::Uuid;

use crate::backend::{AssetBytes, AssetMeta, BrushRow, TemplateRow};

#[derive(Debug, Error)]
pub enum RemoteError {
    #[error("auth: {0}")]
    Auth(#[from] super::auth::AuthError),
    #[error("identity: {0}")]
    Identity(#[from] super::identity::IdentityError),
    #[error("graphql: {0}")]
    GraphQl(#[from] super::graphql::GraphQlError),
    #[error("s3: {0}")]
    S3(#[from] super::s3::S3Error),
    #[error("config: {0}")]
    Config(#[from] super::config::ConfigError),
    #[error("not signed in")]
    NotSignedIn,
    #[error("malformed remote payload: {0}")]
    Malformed(String),
}

/// Visibility tier on a remote model. Mirrors the GraphQL enum
/// declared in `amplify/data/resource.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Unlisted,
    Public,
}

impl Visibility {
    pub fn as_gql(&self) -> &'static str {
        match self {
            Self::Private => "PRIVATE",
            Self::Unlisted => "UNLISTED",
            Self::Public => "PUBLIC",
        }
    }
}

/// Summary row returned by `list*By*` queries — enough to render
/// a browse list without fetching the body. `body_toml` stays at
/// the server until the client fetches a single template.
#[derive(Debug, Clone)]
pub struct RemoteTemplateSummary {
    pub id: Uuid,
    pub owner: Option<String>,
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub visibility: Visibility,
    pub fork_count: i64,
    pub view_count: i64,
    pub forked_from: Option<Uuid>,
    pub updated_at_sort: String,
}

/// Operations the desktop app (and seed-publish CLI) call against
/// the remote catalog. All methods are `&mut self` because the
/// underlying connection caches tokens + identity creds and
/// transparently refreshes when expired.
///
/// Not part of [`crate::JournalBackend`]: remote ops are intentionally
/// distinct from the local CRUD surface so a future "offline-only"
/// build can drop them without breaking anything.
pub trait RemoteTemplateOps {
    // ── browse ──
    fn list_public_page_templates(&mut self) -> Result<Vec<RemoteTemplateSummary>, RemoteError>;
    fn list_public_notebook_templates(&mut self)
        -> Result<Vec<RemoteTemplateSummary>, RemoteError>;
    fn list_public_brushes(&mut self) -> Result<Vec<RemoteTemplateSummary>, RemoteError>;

    fn get_page_template(&mut self, id: Uuid) -> Result<(TemplateRow, Vec<AssetMeta>), RemoteError>;
    fn get_notebook_template(&mut self, id: Uuid) -> Result<TemplateRow, RemoteError>;
    fn get_brush(&mut self, id: Uuid) -> Result<BrushRow, RemoteError>;

    // ── publish — uploads the local row to the catalog with the chosen visibility ──
    fn publish_page_template(
        &mut self,
        row: &TemplateRow,
        assets: &[AssetBytes],
        visibility: Visibility,
    ) -> Result<TemplateRow, RemoteError>;
    fn publish_notebook_template(
        &mut self,
        row: &TemplateRow,
        visibility: Visibility,
    ) -> Result<TemplateRow, RemoteError>;
    fn publish_brush(
        &mut self,
        row: &BrushRow,
        visibility: Visibility,
    ) -> Result<BrushRow, RemoteError>;

    // ── fork — server-side clone of a public/unlisted entry into the caller's namespace ──
    fn fork_page_template(&mut self, id: Uuid) -> Result<TemplateRow, RemoteError>;
    fn fork_notebook_template(&mut self, id: Uuid) -> Result<TemplateRow, RemoteError>;
    fn fork_brush(&mut self, id: Uuid) -> Result<BrushRow, RemoteError>;

    // ── asset bytes (page templates only) ──
    fn fetch_asset_bytes(&mut self, sha256_hex: &str) -> Result<Vec<u8>, RemoteError>;
}

// ── GraphQL operations ──────────────────────────────────────────────
//
// One string constant per operation, matching the schema declared in
// `amplify/data/resource.ts`. Hand-rolled per Decision #12. Several
// constants below are scaffolding for trait impls that get wired up
// post-sandbox (`#[allow(dead_code)]` on those keeps the build quiet
// without hiding genuinely-orphaned constants once the impls land).

pub(crate) const Q_LIST_PAGE_TEMPLATES_PUBLIC: &str = r#"
query ListPublicPageTemplates {
  listPageTemplatesByVisibility(visibility: PUBLIC, sortDirection: DESC) {
    items {
      id owner name description category visibility
      forkedFrom forkCount viewCount updatedAtSort
    }
  }
}
"#;

pub(crate) const Q_LIST_NOTEBOOK_TEMPLATES_PUBLIC: &str = r#"
query ListPublicNotebookTemplates {
  listNotebookTemplatesByVisibility(visibility: PUBLIC, sortDirection: DESC) {
    items {
      id owner name description visibility
      forkedFrom forkCount viewCount updatedAtSort
    }
  }
}
"#;

pub(crate) const Q_LIST_BRUSHES_PUBLIC: &str = r#"
query ListPublicBrushes {
  listBrushesByVisibility(visibility: PUBLIC, sortDirection: DESC) {
    items {
      id owner name description visibility
      forkedFrom forkCount viewCount updatedAtSort
    }
  }
}
"#;

#[allow(dead_code)]
pub(crate) const Q_GET_PAGE_TEMPLATE: &str = r#"
query GetPageTemplate($id: ID!) {
  getPageTemplate(id: $id) {
    id owner name description category visibility bodyToml assets
    forkedFrom forkCount viewCount updatedAtSort
  }
}
"#;

#[allow(dead_code)]
pub(crate) const Q_GET_NOTEBOOK_TEMPLATE: &str = r#"
query GetNotebookTemplate($id: ID!) {
  getNotebookTemplate(id: $id) {
    id owner name description visibility bodyToml
    forkedFrom forkCount viewCount updatedAtSort
  }
}
"#;

#[allow(dead_code)]
pub(crate) const Q_GET_BRUSH: &str = r#"
query GetBrush($id: ID!) {
  getBrush(id: $id) {
    id owner name description visibility bodyToml
    forkedFrom forkCount viewCount updatedAtSort
  }
}
"#;

#[allow(dead_code)]
pub(crate) const M_CREATE_PAGE_TEMPLATE: &str = r#"
mutation CreatePageTemplate($input: CreatePageTemplateInput!) {
  createPageTemplate(input: $input) {
    id name description category visibility bodyToml assets
    forkedFrom forkCount viewCount updatedAtSort
  }
}
"#;

#[allow(dead_code)]
pub(crate) const M_CREATE_NOTEBOOK_TEMPLATE: &str = r#"
mutation CreateNotebookTemplate($input: CreateNotebookTemplateInput!) {
  createNotebookTemplate(input: $input) {
    id name description visibility bodyToml
    forkedFrom forkCount viewCount updatedAtSort
  }
}
"#;

#[allow(dead_code)]
pub(crate) const M_CREATE_BRUSH: &str = r#"
mutation CreateBrush($input: CreateBrushInput!) {
  createBrush(input: $input) {
    id name description visibility bodyToml
    forkedFrom forkCount viewCount updatedAtSort
  }
}
"#;

#[allow(dead_code)]
pub(crate) const M_PUBLISH_PAGE_TEMPLATE: &str = r#"
mutation PublishPageTemplate($id: ID!, $visibility: Visibility!) {
  publishPageTemplate(id: $id, visibility: $visibility) { id visibility updatedAtSort }
}
"#;

#[allow(dead_code)]
pub(crate) const M_PUBLISH_NOTEBOOK_TEMPLATE: &str = r#"
mutation PublishNotebookTemplate($id: ID!, $visibility: Visibility!) {
  publishNotebookTemplate(id: $id, visibility: $visibility) { id visibility updatedAtSort }
}
"#;

#[allow(dead_code)]
pub(crate) const M_PUBLISH_BRUSH: &str = r#"
mutation PublishBrush($id: ID!, $visibility: Visibility!) {
  publishBrush(id: $id, visibility: $visibility) { id visibility updatedAtSort }
}
"#;

#[allow(dead_code)]
pub(crate) const M_FORK_PAGE_TEMPLATE: &str = r#"
mutation ForkPageTemplate($id: ID!) {
  forkPageTemplate(id: $id) {
    id name description category visibility bodyToml assets
    forkedFrom forkCount viewCount updatedAtSort
  }
}
"#;

#[allow(dead_code)]
pub(crate) const M_FORK_NOTEBOOK_TEMPLATE: &str = r#"
mutation ForkNotebookTemplate($id: ID!) {
  forkNotebookTemplate(id: $id) {
    id name description visibility bodyToml
    forkedFrom forkCount viewCount updatedAtSort
  }
}
"#;

#[allow(dead_code)]
pub(crate) const M_FORK_BRUSH: &str = r#"
mutation ForkBrush($id: ID!) {
  forkBrush(id: $id) {
    id name description visibility bodyToml
    forkedFrom forkCount viewCount updatedAtSort
  }
}
"#;

#[allow(dead_code)]
pub(crate) const M_GET_ASSET_UPLOAD_URL: &str = r#"
mutation GetAssetUploadUrl(
  $templateId: ID!,
  $sha256: String!,
  $contentType: String!,
  $sizeBytes: Int!,
) {
  getAssetUploadUrl(
    templateId: $templateId,
    sha256: $sha256,
    contentType: $contentType,
    sizeBytes: $sizeBytes,
  ) { uploadUrl s3Key }
}
"#;

// ── client struct ───────────────────────────────────────────────────

/// Stateful client over the four building-block modules. Holds the
/// loaded `amplify_outputs.json`, the persisted Cognito tokens, and
/// (lazily) the Identity-Pool AWS credentials. Refreshes
/// transparently when [`auth::Tokens::needs_refresh`] /
/// [`identity::AwsCredentials::needs_refresh`] return `true`.
///
/// Construction is fallible:
///   * If `amplify_outputs.json` is empty / missing required fields,
///     [`connect`] returns [`RemoteError::Config`] — the caller
///     treats this as "remote not configured, hide UI".
///   * If no tokens are persisted yet, [`connect`] still succeeds;
///     [`is_signed_in`] returns `false` and the caller surfaces a
///     sign-in prompt.
#[cfg(feature = "remote")]
pub struct RemoteTemplateStore {
    config: super::config::AmplifyOutputs,
    tokens: Option<super::auth::Tokens>,
    creds: Option<(String, super::identity::AwsCredentials)>,
}

#[cfg(feature = "remote")]
impl RemoteTemplateStore {
    /// Load `amplify_outputs.json` + any persisted tokens. Does not
    /// touch the network.
    pub fn connect() -> Result<Self, RemoteError> {
        let config = super::config::load()?;
        let tokens = super::auth::load_tokens()?;
        Ok(Self {
            config,
            tokens,
            creds: None,
        })
    }

    pub fn is_signed_in(&self) -> bool {
        self.tokens.is_some()
    }

    /// Sign in with email + password. On success persists tokens +
    /// caches them on the struct. On failure leaves any prior
    /// tokens intact (so a wrong-password retry doesn't sign the
    /// user out).
    pub fn sign_in(&mut self, username: &str, password: &str) -> Result<(), RemoteError> {
        let tokens = super::auth::sign_in(
            &self.config.auth_region,
            &self.config.user_pool_client_id,
            username,
            password,
        )?;
        self.tokens = Some(tokens);
        self.creds = None;
        Ok(())
    }

    pub fn sign_out(&mut self) -> Result<(), RemoteError> {
        super::auth::clear_tokens()?;
        self.tokens = None;
        self.creds = None;
        Ok(())
    }

    fn now() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Return a borrow of the active token bundle, refreshing first
    /// if it's within the 60-s expiry buffer. Returns
    /// [`RemoteError::NotSignedIn`] if the user has no tokens.
    fn ensure_tokens(&mut self) -> Result<&super::auth::Tokens, RemoteError> {
        let needs_refresh = match &self.tokens {
            Some(t) => t.needs_refresh(Self::now(), 60),
            None => return Err(RemoteError::NotSignedIn),
        };
        if needs_refresh {
            let current = self.tokens.as_ref().expect("checked above").clone();
            let refreshed = super::auth::refresh(
                &self.config.auth_region,
                &self.config.user_pool_client_id,
                &current,
            )?;
            self.tokens = Some(refreshed);
            // Refreshed id_token invalidates the cached
            // Identity-Pool creds — drop them so the next
            // ensure_creds call re-fetches.
            self.creds = None;
        }
        Ok(self.tokens.as_ref().expect("set above"))
    }

    /// Return a borrow of the Identity-Pool AWS credentials,
    /// fetching / refreshing first if needed.
    #[allow(dead_code)]
    fn ensure_creds(&mut self) -> Result<&super::identity::AwsCredentials, RemoteError> {
        // First make sure tokens are valid (creds depend on id_token).
        let id_token = self.ensure_tokens()?.id_token.clone();
        let now = Self::now();
        let needs_fetch = match &self.creds {
            Some((_, c)) => c.needs_refresh(now, 60),
            None => true,
        };
        if needs_fetch {
            let login_key = super::identity::login_key(
                &self.config.auth_region,
                &self.config.user_pool_id,
            );
            let identity_id = super::identity::get_identity_id(
                &self.config.auth_region,
                &self.config.identity_pool_id,
                &login_key,
                &id_token,
            )?;
            let creds = super::identity::get_credentials(
                &self.config.auth_region,
                &identity_id,
                &login_key,
                &id_token,
            )?;
            self.creds = Some((identity_id, creds));
        }
        Ok(&self.creds.as_ref().expect("set above").1)
    }

    fn gql(
        &mut self,
        query: &str,
        operation: Option<&str>,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, RemoteError> {
        let id_token = self.ensure_tokens()?.id_token.clone();
        let data_url = self.config.data_url.clone();
        let v = super::graphql::post(&data_url, &id_token, query, operation, variables)?;
        Ok(v)
    }
}

#[cfg(feature = "remote")]
impl RemoteTemplateOps for RemoteTemplateStore {
    fn list_public_page_templates(&mut self) -> Result<Vec<RemoteTemplateSummary>, RemoteError> {
        let v = self.gql(Q_LIST_PAGE_TEMPLATES_PUBLIC, None, serde_json::json!({}))?;
        let items = v
            .pointer("/listPageTemplatesByVisibility/items")
            .and_then(|x| x.as_array())
            .ok_or_else(|| RemoteError::Malformed("missing items".into()))?;
        items.iter().map(parse_summary).collect()
    }

    fn list_public_notebook_templates(
        &mut self,
    ) -> Result<Vec<RemoteTemplateSummary>, RemoteError> {
        let v = self.gql(Q_LIST_NOTEBOOK_TEMPLATES_PUBLIC, None, serde_json::json!({}))?;
        let items = v
            .pointer("/listNotebookTemplatesByVisibility/items")
            .and_then(|x| x.as_array())
            .ok_or_else(|| RemoteError::Malformed("missing items".into()))?;
        items.iter().map(parse_summary).collect()
    }

    fn list_public_brushes(&mut self) -> Result<Vec<RemoteTemplateSummary>, RemoteError> {
        let v = self.gql(Q_LIST_BRUSHES_PUBLIC, None, serde_json::json!({}))?;
        let items = v
            .pointer("/listBrushesByVisibility/items")
            .and_then(|x| x.as_array())
            .ok_or_else(|| RemoteError::Malformed("missing items".into()))?;
        items.iter().map(parse_summary).collect()
    }

    fn get_page_template(
        &mut self,
        _id: Uuid,
    ) -> Result<(TemplateRow, Vec<AssetMeta>), RemoteError> {
        // TODO(sandbox): GraphQL get + parse bodyToml + assets json. Needs
        //                a live AppSync endpoint to verify the response shape
        //                Amplify generates for `assets: a.json()`.
        Err(RemoteError::Malformed(
            "get_page_template not yet wired (needs sandbox)".into(),
        ))
    }
    fn get_notebook_template(&mut self, _id: Uuid) -> Result<TemplateRow, RemoteError> {
        Err(RemoteError::Malformed(
            "get_notebook_template not yet wired (needs sandbox)".into(),
        ))
    }
    fn get_brush(&mut self, _id: Uuid) -> Result<BrushRow, RemoteError> {
        Err(RemoteError::Malformed(
            "get_brush not yet wired (needs sandbox)".into(),
        ))
    }
    fn publish_page_template(
        &mut self,
        _row: &TemplateRow,
        _assets: &[AssetBytes],
        _visibility: Visibility,
    ) -> Result<TemplateRow, RemoteError> {
        Err(RemoteError::Malformed(
            "publish_page_template not yet wired (needs sandbox)".into(),
        ))
    }
    fn publish_notebook_template(
        &mut self,
        _row: &TemplateRow,
        _visibility: Visibility,
    ) -> Result<TemplateRow, RemoteError> {
        Err(RemoteError::Malformed(
            "publish_notebook_template not yet wired (needs sandbox)".into(),
        ))
    }
    fn publish_brush(
        &mut self,
        _row: &BrushRow,
        _visibility: Visibility,
    ) -> Result<BrushRow, RemoteError> {
        Err(RemoteError::Malformed(
            "publish_brush not yet wired (needs sandbox)".into(),
        ))
    }
    fn fork_page_template(&mut self, _id: Uuid) -> Result<TemplateRow, RemoteError> {
        Err(RemoteError::Malformed(
            "fork_page_template not yet wired (needs sandbox)".into(),
        ))
    }
    fn fork_notebook_template(&mut self, _id: Uuid) -> Result<TemplateRow, RemoteError> {
        Err(RemoteError::Malformed(
            "fork_notebook_template not yet wired (needs sandbox)".into(),
        ))
    }
    fn fork_brush(&mut self, _id: Uuid) -> Result<BrushRow, RemoteError> {
        Err(RemoteError::Malformed(
            "fork_brush not yet wired (needs sandbox)".into(),
        ))
    }
    fn fetch_asset_bytes(&mut self, _sha256_hex: &str) -> Result<Vec<u8>, RemoteError> {
        Err(RemoteError::Malformed(
            "fetch_asset_bytes not yet wired (needs sandbox)".into(),
        ))
    }
}

// ── helpers / parsers ───────────────────────────────────────────────

pub(crate) fn parse_visibility(s: &str) -> Result<Visibility, RemoteError> {
    match s {
        "PRIVATE" => Ok(Visibility::Private),
        "UNLISTED" => Ok(Visibility::Unlisted),
        "PUBLIC" => Ok(Visibility::Public),
        other => Err(RemoteError::Malformed(format!(
            "unknown visibility {:?}",
            other
        ))),
    }
}

pub(crate) fn parse_uuid(s: &str) -> Result<Uuid, RemoteError> {
    Uuid::parse_str(s).map_err(|e| RemoteError::Malformed(format!("uuid {:?}: {}", s, e)))
}

pub(crate) fn parse_summary(item: &serde_json::Value) -> Result<RemoteTemplateSummary, RemoteError> {
    let id_str = item
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| RemoteError::Malformed("missing id".into()))?;
    let id = parse_uuid(id_str)?;
    let owner = item
        .get("owner")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let name = item
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let description = item
        .get("description")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let category = item
        .get("category")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let visibility = parse_visibility(
        item.get("visibility")
            .and_then(|x| x.as_str())
            .ok_or_else(|| RemoteError::Malformed("missing visibility".into()))?,
    )?;
    let fork_count = item.get("forkCount").and_then(|x| x.as_i64()).unwrap_or(0);
    let view_count = item.get("viewCount").and_then(|x| x.as_i64()).unwrap_or(0);
    let forked_from = match item.get("forkedFrom").and_then(|x| x.as_str()) {
        Some(s) if !s.is_empty() => Some(parse_uuid(s)?),
        _ => None,
    };
    let updated_at_sort = item
        .get("updatedAtSort")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Ok(RemoteTemplateSummary {
        id,
        owner,
        name,
        description,
        category,
        visibility,
        fork_count,
        view_count,
        forked_from,
        updated_at_sort,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn visibility_round_trips() {
        for v in [Visibility::Private, Visibility::Unlisted, Visibility::Public] {
            assert_eq!(parse_visibility(v.as_gql()).unwrap(), v);
        }
    }

    #[test]
    fn parse_visibility_rejects_unknown() {
        assert!(matches!(
            parse_visibility("HIDDEN").unwrap_err(),
            RemoteError::Malformed(_)
        ));
    }

    #[test]
    fn parse_summary_full() {
        let raw = json!({
            "id": "c8e7e6d4-2c4f-4ff2-9f8d-9b7e6f5a4c3b",
            "owner": "alice",
            "name": "Daily plan",
            "description": "a8 grid",
            "category": "Planner",
            "visibility": "PUBLIC",
            "forkCount": 12,
            "viewCount": 340,
            "forkedFrom": "f0d0d0d0-1111-2222-3333-444444444444",
            "updatedAtSort": "2026-05-01T00:00:00Z"
        });
        let s = parse_summary(&raw).unwrap();
        assert_eq!(s.name, "Daily plan");
        assert_eq!(s.fork_count, 12);
        assert_eq!(s.view_count, 340);
        assert_eq!(s.visibility, Visibility::Public);
        assert!(s.forked_from.is_some());
        assert_eq!(s.owner.as_deref(), Some("alice"));
    }

    #[test]
    fn parse_summary_minimal() {
        let raw = json!({
            "id": "c8e7e6d4-2c4f-4ff2-9f8d-9b7e6f5a4c3b",
            "name": "Bare",
            "visibility": "UNLISTED",
            "updatedAtSort": "2026-05-01T00:00:00Z"
        });
        let s = parse_summary(&raw).unwrap();
        assert_eq!(s.visibility, Visibility::Unlisted);
        assert_eq!(s.fork_count, 0);
        assert!(s.forked_from.is_none());
        assert!(s.owner.is_none());
        assert!(s.category.is_none());
    }

    #[test]
    fn parse_summary_missing_id_errors() {
        let raw = json!({ "name": "x", "visibility": "PUBLIC", "updatedAtSort": "" });
        assert!(matches!(
            parse_summary(&raw).unwrap_err(),
            RemoteError::Malformed(_)
        ));
    }
}
