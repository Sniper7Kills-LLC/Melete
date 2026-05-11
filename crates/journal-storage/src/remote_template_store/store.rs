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

    /// Decode the JWT `sub` claim from the current id_token without
    /// verifying the signature (Cognito already validated server-side
    /// at token issue). Returns `RemoteError::NotSignedIn` if no
    /// tokens are loaded; `RemoteError::Malformed` if the JWT payload
    /// is unparseable.
    pub fn user_sub(&mut self) -> Result<String, RemoteError> {
        let id_token = self.ensure_tokens()?.id_token.clone();
        super::jwt::decode_sub(&id_token)
    }

    /// Returns the `cognito:groups` membership for the signed-in
    /// user. Used by the admin panel to decide whether to render
    /// itself. Empty vec when no groups are assigned.
    pub fn user_groups(&mut self) -> Result<Vec<String>, RemoteError> {
        let id_token = self.ensure_tokens()?.id_token.clone();
        super::jwt::decode_groups(&id_token)
    }

    /// Public façade over the internal `gql` helper. Lets the admin
    /// panel + future ad-hoc query sites ship without re-exporting
    /// the entire `graphql` module from the storage crate.
    pub fn graphql(
        &mut self,
        query: &str,
        operation: Option<&str>,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, RemoteError> {
        self.gql(query, operation, variables)
    }

    /// Pull the caller's `UserEntitlement` row. Used by the desktop
    /// settings UI + sync gate. Returns the free-tier default if no
    /// row exists yet (new user pre-checkout).
    pub fn fetch_my_entitlement(
        &mut self,
    ) -> Result<crate::entitlement::Entitlement, RemoteError> {
        let sub = self.user_sub()?;
        let v = self.gql(
            super::ENTITLEMENT_QUERY,
            Some("GetMyEntitlement"),
            serde_json::json!({ "id": sub }),
        )?;
        let row = v.get("getUserEntitlement").cloned().unwrap_or(serde_json::Value::Null);
        if row.is_null() {
            return Ok(crate::entitlement::Entitlement::free_default(sub));
        }
        serde_json::from_value::<crate::entitlement::Entitlement>(row)
            .map_err(|e| RemoteError::Malformed(format!("entitlement decode: {e}")))
    }

    /// Mint a Stripe Checkout Session URL. Returns the hosted URL
    /// the desktop opens in the user's default browser.
    pub fn create_checkout_session(
        &mut self,
        tier: &str,
        interval: &str,
    ) -> Result<String, RemoteError> {
        let v = self.gql(
            super::CREATE_CHECKOUT_SESSION_MUTATION,
            Some("CreateCheckoutSession"),
            serde_json::json!({ "tier": tier, "interval": interval }),
        )?;
        v.pointer("/createCheckoutSession/url")
            .and_then(|x| x.as_str())
            .map(String::from)
            .ok_or_else(|| RemoteError::Malformed("missing checkout url".into()))
    }

    /// Mint a Stripe Customer Portal URL for the caller. Surfaces a
    /// `NO_STRIPE_CUSTOMER` error when the caller has never
    /// subscribed; the UI routes those to Checkout instead.
    pub fn create_portal_session(&mut self) -> Result<String, RemoteError> {
        let v = self.gql(
            super::CREATE_PORTAL_SESSION_MUTATION,
            Some("CreatePortalSession"),
            serde_json::json!({}),
        )?;
        v.pointer("/createPortalSession/url")
            .and_then(|x| x.as_str())
            .map(String::from)
            .ok_or_else(|| RemoteError::Malformed("missing portal url".into()))
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
        id: Uuid,
    ) -> Result<(TemplateRow, Vec<AssetMeta>), RemoteError> {
        let v = self.gql(
            Q_GET_PAGE_TEMPLATE,
            None,
            serde_json::json!({ "id": id.to_string() }),
        )?;
        let item = v
            .get("getPageTemplate")
            .ok_or_else(|| RemoteError::Malformed("missing getPageTemplate".into()))?;
        if item.is_null() {
            return Err(RemoteError::Malformed(format!("page template {} not found", id)));
        }
        parse_page_template_row_with_assets(item)
    }
    fn get_notebook_template(&mut self, id: Uuid) -> Result<TemplateRow, RemoteError> {
        let v = self.gql(
            Q_GET_NOTEBOOK_TEMPLATE,
            None,
            serde_json::json!({ "id": id.to_string() }),
        )?;
        let item = v
            .get("getNotebookTemplate")
            .ok_or_else(|| RemoteError::Malformed("missing getNotebookTemplate".into()))?;
        if item.is_null() {
            return Err(RemoteError::Malformed(format!(
                "notebook template {} not found",
                id
            )));
        }
        parse_template_row(item)
    }
    fn get_brush(&mut self, id: Uuid) -> Result<BrushRow, RemoteError> {
        let v = self.gql(
            Q_GET_BRUSH,
            None,
            serde_json::json!({ "id": id.to_string() }),
        )?;
        let item = v
            .get("getBrush")
            .ok_or_else(|| RemoteError::Malformed("missing getBrush".into()))?;
        if item.is_null() {
            return Err(RemoteError::Malformed(format!("brush {} not found", id)));
        }
        parse_brush_row(item)
    }
    fn publish_page_template(
        &mut self,
        row: &TemplateRow,
        assets: &[AssetBytes],
        visibility: Visibility,
    ) -> Result<TemplateRow, RemoteError> {
        // 1. Upload each asset's bytes to S3 via the presigned-URL Lambda.
        //    Skip already-uploaded shas (S3 PUT is idempotent on the
        //    content-addressed key, but skipping saves a round trip).
        for asset in assets {
            self.upload_asset(row.id, asset)?;
        }
        // 2. createPageTemplate to upsert the row + asset metadata.
        let assets_meta: Vec<serde_json::Value> = assets
            .iter()
            .map(|a| {
                serde_json::json!({
                    "name": a.name,
                    "mime": a.mime,
                    "sha256": a.sha256,
                    "size": a.bytes.len(),
                })
            })
            .collect();
        let input = serde_json::json!({
            "id": row.id.to_string(),
            "name": row.name,
            "description": row.description,
            "category": row.category,
            "visibility": Visibility::Private.as_gql(),
            "bodyToml": row.body_toml,
            "assets": assets_meta,
            "updatedAtSort": row.updated_at_sort,
        });
        // createPageTemplate is generated by Amplify; ignore conflicts
        // (already-published row keeps its existing record). The
        // followup publishPageTemplate flips visibility regardless.
        let _ = self.gql(M_CREATE_PAGE_TEMPLATE, None, serde_json::json!({ "input": input }));
        // 3. publishPageTemplate flips visibility + bumps updatedAtSort.
        let v = self.gql(
            M_PUBLISH_PAGE_TEMPLATE,
            None,
            serde_json::json!({ "id": row.id.to_string(), "visibility": visibility.as_gql() }),
        )?;
        let item = v
            .get("publishPageTemplate")
            .ok_or_else(|| RemoteError::Malformed("missing publishPageTemplate".into()))?;
        let updated_at_sort = item
            .get("updatedAtSort")
            .and_then(|x| x.as_str())
            .unwrap_or(row.updated_at_sort.as_str())
            .to_string();
        Ok(TemplateRow {
            updated_at_sort,
            ..row.clone()
        })
    }
    fn publish_notebook_template(
        &mut self,
        row: &TemplateRow,
        visibility: Visibility,
    ) -> Result<TemplateRow, RemoteError> {
        let input = serde_json::json!({
            "id": row.id.to_string(),
            "name": row.name,
            "description": row.description,
            "visibility": Visibility::Private.as_gql(),
            "bodyToml": row.body_toml,
            "updatedAtSort": row.updated_at_sort,
        });
        let _ = self.gql(M_CREATE_NOTEBOOK_TEMPLATE, None, serde_json::json!({ "input": input }));
        let v = self.gql(
            M_PUBLISH_NOTEBOOK_TEMPLATE,
            None,
            serde_json::json!({ "id": row.id.to_string(), "visibility": visibility.as_gql() }),
        )?;
        let item = v
            .get("publishNotebookTemplate")
            .ok_or_else(|| RemoteError::Malformed("missing publishNotebookTemplate".into()))?;
        let updated_at_sort = item
            .get("updatedAtSort")
            .and_then(|x| x.as_str())
            .unwrap_or(row.updated_at_sort.as_str())
            .to_string();
        Ok(TemplateRow {
            updated_at_sort,
            ..row.clone()
        })
    }
    fn publish_brush(
        &mut self,
        row: &BrushRow,
        visibility: Visibility,
    ) -> Result<BrushRow, RemoteError> {
        let input = serde_json::json!({
            "id": row.id.to_string(),
            "name": row.name,
            "visibility": Visibility::Private.as_gql(),
            "bodyToml": row.body_toml,
            "updatedAtSort": row.updated_at_sort,
        });
        let _ = self.gql(M_CREATE_BRUSH, None, serde_json::json!({ "input": input }));
        let v = self.gql(
            M_PUBLISH_BRUSH,
            None,
            serde_json::json!({ "id": row.id.to_string(), "visibility": visibility.as_gql() }),
        )?;
        let item = v
            .get("publishBrush")
            .ok_or_else(|| RemoteError::Malformed("missing publishBrush".into()))?;
        let updated_at_sort = item
            .get("updatedAtSort")
            .and_then(|x| x.as_str())
            .unwrap_or(row.updated_at_sort.as_str())
            .to_string();
        Ok(BrushRow {
            updated_at_sort,
            ..row.clone()
        })
    }
    fn fork_page_template(&mut self, id: Uuid) -> Result<TemplateRow, RemoteError> {
        let v = self.gql(
            M_FORK_PAGE_TEMPLATE,
            None,
            serde_json::json!({ "id": id.to_string() }),
        )?;
        let item = v
            .get("forkPageTemplate")
            .ok_or_else(|| RemoteError::Malformed("missing forkPageTemplate".into()))?;
        let (row, _assets) = parse_page_template_row_with_assets(item)?;
        Ok(row)
    }
    fn fork_notebook_template(&mut self, id: Uuid) -> Result<TemplateRow, RemoteError> {
        let v = self.gql(
            M_FORK_NOTEBOOK_TEMPLATE,
            None,
            serde_json::json!({ "id": id.to_string() }),
        )?;
        let item = v
            .get("forkNotebookTemplate")
            .ok_or_else(|| RemoteError::Malformed("missing forkNotebookTemplate".into()))?;
        parse_template_row(item)
    }
    fn fork_brush(&mut self, id: Uuid) -> Result<BrushRow, RemoteError> {
        let v = self.gql(
            M_FORK_BRUSH,
            None,
            serde_json::json!({ "id": id.to_string() }),
        )?;
        let item = v
            .get("forkBrush")
            .ok_or_else(|| RemoteError::Malformed("missing forkBrush".into()))?;
        parse_brush_row(item)
    }
    fn fetch_asset_bytes(&mut self, sha256_hex: &str) -> Result<Vec<u8>, RemoteError> {
        // SigV4 GET against the bucket. Owner-readable path uses the
        // caller's identity sub; published assets live under
        // `public/templates/...` and are guest-readable but keyed by
        // (templateId, sha) — out of scope for this content-addressed
        // helper. For now we only support owner reads; published-asset
        // fetches go through the web viewer's direct CDN URL.
        let creds = self.ensure_creds()?.clone();
        let bucket = self.config.storage_bucket.clone();
        let region = self.config.storage_region.clone();
        let bytes = super::s3::get(&creds, &bucket, &region, sha256_hex)?;
        Ok(bytes)
    }
}

#[cfg(feature = "remote")]
impl RemoteTemplateStore {
    /// Upload one asset's bytes via the `getAssetUploadUrl` Lambda's
    /// presigned PUT URL.
    fn upload_asset(&mut self, template_id: Uuid, asset: &AssetBytes) -> Result<(), RemoteError> {
        let v = self.gql(
            M_GET_ASSET_UPLOAD_URL,
            None,
            serde_json::json!({
                "templateId": template_id.to_string(),
                "sha256": asset.sha256,
                "contentType": asset.mime,
                "sizeBytes": asset.bytes.len(),
            }),
        )?;
        let url = v
            .pointer("/getAssetUploadUrl/uploadUrl")
            .and_then(|x| x.as_str())
            .ok_or_else(|| RemoteError::Malformed("missing uploadUrl".into()))?
            .to_string();
        let client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|e| RemoteError::S3(super::s3::S3Error::Transport(e.to_string())))?;
        let resp = client
            .put(&url)
            .header("Content-Type", asset.mime.as_str())
            .body(asset.bytes.clone())
            .send()
            .map_err(|e| RemoteError::S3(super::s3::S3Error::Transport(e.to_string())))?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().unwrap_or_default();
            return Err(RemoteError::S3(super::s3::S3Error::Http { status, body }));
        }
        Ok(())
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

/// sha256(body_toml) — clients re-derive on get/fork because the
/// server doesn't store the body digest separately.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Parse a generic NotebookTemplate / Brush full row payload into the
/// shared `TemplateRow` shape (notebooks have no `category`; brushes
/// have no `description` either, but we keep both fields tolerant).
pub(crate) fn parse_template_row(item: &serde_json::Value) -> Result<TemplateRow, RemoteError> {
    let id = parse_uuid(
        item.get("id")
            .and_then(|x| x.as_str())
            .ok_or_else(|| RemoteError::Malformed("missing id".into()))?,
    )?;
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
        .unwrap_or("")
        .to_string();
    let body_toml = item
        .get("bodyToml")
        .and_then(|x| x.as_str())
        .ok_or_else(|| RemoteError::Malformed("missing bodyToml".into()))?
        .to_string();
    let updated_at_sort = item
        .get("updatedAtSort")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let sha256 = sha256_hex(body_toml.as_bytes());
    Ok(TemplateRow {
        id,
        name,
        description,
        category,
        body_toml,
        sha256,
        updated_at_sort,
    })
}

/// Parse a Brush full row. Same shape as `parse_template_row`
/// minus `description` / `category` (BrushRow doesn't carry them).
pub(crate) fn parse_brush_row(item: &serde_json::Value) -> Result<BrushRow, RemoteError> {
    let id = parse_uuid(
        item.get("id")
            .and_then(|x| x.as_str())
            .ok_or_else(|| RemoteError::Malformed("missing id".into()))?,
    )?;
    let name = item
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let body_toml = item
        .get("bodyToml")
        .and_then(|x| x.as_str())
        .ok_or_else(|| RemoteError::Malformed("missing bodyToml".into()))?
        .to_string();
    let updated_at_sort = item
        .get("updatedAtSort")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let sha256 = sha256_hex(body_toml.as_bytes());
    Ok(BrushRow {
        id,
        name,
        body_toml,
        sha256,
        updated_at_sort,
    })
}

/// Page templates also carry an `assets` JSON array with per-asset
/// metadata. Returns the row + parsed metadata; bytes are fetched on
/// demand via `fetch_asset_bytes`.
pub(crate) fn parse_page_template_row_with_assets(
    item: &serde_json::Value,
) -> Result<(TemplateRow, Vec<AssetMeta>), RemoteError> {
    let row = parse_template_row(item)?;
    let assets = match item.get("assets") {
        // The `assets` column is `a.json()` → AppSync may return it as a
        // JSON-encoded string, a parsed array, or null.
        Some(serde_json::Value::Array(arr)) => parse_asset_meta_array(arr)?,
        Some(serde_json::Value::String(s)) if !s.is_empty() => {
            let parsed: serde_json::Value = serde_json::from_str(s)
                .map_err(|e| RemoteError::Malformed(format!("assets json: {}", e)))?;
            match parsed {
                serde_json::Value::Array(arr) => parse_asset_meta_array(&arr)?,
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    };
    Ok((row, assets))
}

fn parse_asset_meta_array(arr: &[serde_json::Value]) -> Result<Vec<AssetMeta>, RemoteError> {
    arr.iter()
        .map(|v| {
            let name = v
                .get("name")
                .and_then(|x| x.as_str())
                .ok_or_else(|| RemoteError::Malformed("asset missing name".into()))?
                .to_string();
            let mime = v
                .get("mime")
                .and_then(|x| x.as_str())
                .unwrap_or("application/octet-stream")
                .to_string();
            let sha256 = v
                .get("sha256")
                .and_then(|x| x.as_str())
                .ok_or_else(|| RemoteError::Malformed("asset missing sha256".into()))?
                .to_string();
            let size = v
                .get("size")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            Ok(AssetMeta {
                name,
                mime,
                sha256,
                size,
            })
        })
        .collect()
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

    #[test]
    fn parse_template_row_full() {
        let raw = json!({
            "id": "c8e7e6d4-2c4f-4ff2-9f8d-9b7e6f5a4c3b",
            "name": "Daily plan",
            "description": "a8 grid",
            "category": "Planner",
            "bodyToml": "[meta]\nname = \"x\"\n",
            "updatedAtSort": "2026-05-01T00:00:00Z"
        });
        let r = parse_template_row(&raw).unwrap();
        assert_eq!(r.name, "Daily plan");
        assert_eq!(r.body_toml, "[meta]\nname = \"x\"\n");
        assert_eq!(r.sha256.len(), 64);
        assert_eq!(r.updated_at_sort, "2026-05-01T00:00:00Z");
    }

    #[test]
    fn parse_template_row_missing_body_errors() {
        let raw = json!({
            "id": "c8e7e6d4-2c4f-4ff2-9f8d-9b7e6f5a4c3b",
            "name": "x",
            "updatedAtSort": ""
        });
        assert!(matches!(
            parse_template_row(&raw).unwrap_err(),
            RemoteError::Malformed(_)
        ));
    }

    #[test]
    fn parse_brush_row_full() {
        let raw = json!({
            "id": "c8e7e6d4-2c4f-4ff2-9f8d-9b7e6f5a4c3b",
            "name": "Pen",
            "bodyToml": "[brush]\nstyle = \"pen\"\n",
            "updatedAtSort": ""
        });
        let r = parse_brush_row(&raw).unwrap();
        assert_eq!(r.name, "Pen");
        assert!(!r.sha256.is_empty());
    }

    #[test]
    fn parse_page_template_row_with_assets_array() {
        let raw = json!({
            "id": "c8e7e6d4-2c4f-4ff2-9f8d-9b7e6f5a4c3b",
            "name": "Image bg",
            "bodyToml": "[meta]\nname = \"x\"\n",
            "updatedAtSort": "",
            "assets": [
                { "name": "bg.png", "mime": "image/png", "sha256": "0".repeat(64), "size": 1024 }
            ]
        });
        let (row, assets) = parse_page_template_row_with_assets(&raw).unwrap();
        assert_eq!(row.name, "Image bg");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, "bg.png");
        assert_eq!(assets[0].mime, "image/png");
        assert_eq!(assets[0].size, 1024);
    }

    #[test]
    fn parse_page_template_row_with_assets_string_encoded_array() {
        let raw = json!({
            "id": "c8e7e6d4-2c4f-4ff2-9f8d-9b7e6f5a4c3b",
            "name": "Image bg",
            "bodyToml": "x",
            "updatedAtSort": "",
            // AppSync sometimes returns a.json() as a JSON string.
            "assets": "[{\"name\":\"x.pdf\",\"mime\":\"application/pdf\",\"sha256\":\"abc\",\"size\":99}]"
        });
        let (_row, assets) = parse_page_template_row_with_assets(&raw).unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].mime, "application/pdf");
        assert_eq!(assets[0].size, 99);
    }

    #[test]
    fn parse_page_template_row_with_assets_null_is_empty() {
        let raw = json!({
            "id": "c8e7e6d4-2c4f-4ff2-9f8d-9b7e6f5a4c3b",
            "name": "Plain",
            "bodyToml": "x",
            "updatedAtSort": "",
            "assets": null
        });
        let (_row, assets) = parse_page_template_row_with_assets(&raw).unwrap();
        assert!(assets.is_empty());
    }
}
