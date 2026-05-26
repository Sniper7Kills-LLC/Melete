//! Remote notebook (user document) store. Mirrors the local SQLite
//! tables row-by-row to AppSync/DynamoDB:
//!
//!   `notebooks`     ↔ `Notebook`
//!   `sections`      ↔ `RemoteSection`
//!   `pages`         ↔ `RemotePage`
//!   `strokes`       ↔ `RemoteStroke`
//!
//! Sync is just upsert-each-row. Live sync = create one `RemoteStroke`
//! row per local insert, which AppSync fans out to subscribers via
//! the auto-generated `onCreateRemoteStroke` subscription.
//!
//! No S3 binary blobs in this design — every byte that lands in the
//! cloud is a discrete row the user can inspect / delete via the
//! AppSync console.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use melete_core::{Notebook, NotebookId, Page, PageId, Section, SectionId, Stroke};

use super::remote_template_store::{auth, config, graphql, identity};

#[derive(Debug, Error)]
pub enum NotebookSyncError {
    #[error("auth: {0}")]
    Auth(#[from] auth::AuthError),
    #[error("identity: {0}")]
    Identity(#[from] identity::IdentityError),
    #[error("graphql: {0}")]
    GraphQl(#[from] graphql::GraphQlError),
    #[error("config: {0}")]
    Config(#[from] config::ConfigError),
    #[error("not signed in")]
    NotSignedIn,
    #[error("encode: {0}")]
    Encode(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotebookVisibility {
    Private,
    Unlisted,
    Public,
}

impl NotebookVisibility {
    pub fn as_gql(&self) -> &'static str {
        match self {
            Self::Private => "PRIVATE",
            Self::Unlisted => "UNLISTED",
            Self::Public => "PUBLIC",
        }
    }
}

/// Statistics from a sync run — surfaced to the user in the success
/// dialog so they know how much got pushed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncReport {
    pub sections_upserted: usize,
    pub pages_upserted: usize,
    pub strokes_upserted: usize,
    pub visibility: Option<String>,
    /// Stroke IDs whose cloud-delete succeeded (or returned
    /// "already gone"). Caller should `purge_deleted_stroke` for
    /// each so the local tombstone is freed.
    #[serde(default)]
    pub strokes_purged: Vec<Uuid>,
}

/// Progress event the sync engine fires at each stage / per-row.
/// Worker thread pushes these into a shared queue; main-thread poller
/// drains them to drive the progress dialog.
#[derive(Debug, Clone)]
pub enum SyncProgress {
    /// New phase — `label` is what to show in the dialog ("Uploading
    /// strokes", "Uploading pages", …). `total` is the number of
    /// items in this phase (0 for one-shot stages).
    Phase { label: String, total: usize },
    /// One item in the current phase finished. `done` is cumulative
    /// inside this phase.
    Step { done: usize, total: usize },
}

/// No-op callback for callers that don't care about progress (tests,
/// CLI). Use [`Self::sync_notebook`] with this when there's no UI to
/// drive.
pub fn no_progress(_p: SyncProgress) {}

/// Single upsert item for [`RemoteNotebookStore::upsert_strokes_batch`].
/// Cloud row is the LWW record: every mutation sets `updated_at`;
/// soft-delete sets `deleted_at`. Empty payload + `deleted_at` set =
/// pure tombstone push (cloud row keeps prior body).
#[derive(Debug, Clone)]
pub struct UpsertStrokeItem {
    pub id: Uuid,
    pub page_id: PageId,
    pub payload: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

// ── GraphQL operations ──────────────────────────────────────────────

const Q_GET_NOTEBOOK: &str = r#"
query GetNotebook($id: ID!) {
  getNotebook(id: $id) { id name visibility updatedAtSort kindJson assignedTemplatesJson }
}
"#;

const Q_LIST_SECTIONS: &str = r#"
query ListSections($notebookId: ID!) {
  listRemoteSectionsByNotebook(notebookId: $notebookId, limit: 1000) {
    items { id notebookId parentSectionId name position allowedTemplatesJson }
  }
}
"#;

const Q_LIST_PAGES: &str = r#"
query ListPages($notebookId: ID!) {
  listRemotePagesByNotebook(notebookId: $notebookId, limit: 1000) {
    items {
      id notebookId sectionId templateId position name
      plannerAddressJson widgetOverridesJson widgetDataJson flagged
      createdAtIso modifiedAtIso
    }
  }
}
"#;

const Q_LIST_STROKES: &str = r#"
query ListStrokes($notebookId: ID!, $nextToken: String) {
  listRemoteStrokesByNotebook(notebookId: $notebookId, limit: 1000, nextToken: $nextToken) {
    items { id notebookId pageId strokeJson createdAt updatedAtIso deletedAtIso }
    nextToken
  }
}
"#;

const M_CREATE_NOTEBOOK: &str = r#"
mutation CreateNotebook($input: CreateNotebookInput!) {
  createNotebook(input: $input) { id name visibility updatedAtSort }
}
"#;

const M_UPDATE_NOTEBOOK: &str = r#"
mutation UpdateNotebook($input: UpdateNotebookInput!) {
  updateNotebook(input: $input) { id name visibility updatedAtSort }
}
"#;

const M_CREATE_SECTION: &str = r#"
mutation CreateRemoteSection($input: CreateRemoteSectionInput!) {
  createRemoteSection(input: $input) { id name }
}
"#;

const M_UPDATE_SECTION: &str = r#"
mutation UpdateRemoteSection($input: UpdateRemoteSectionInput!) {
  updateRemoteSection(input: $input) { id name }
}
"#;

const M_CREATE_PAGE: &str = r#"
mutation CreateRemotePage($input: CreateRemotePageInput!) {
  createRemotePage(input: $input) { id name }
}
"#;

const M_UPDATE_PAGE: &str = r#"
mutation UpdateRemotePage($input: UpdateRemotePageInput!) {
  updateRemotePage(input: $input) { id name }
}
"#;

const M_CREATE_STROKE: &str = r#"
mutation CreateRemoteStroke($input: CreateRemoteStrokeInput!) {
  createRemoteStroke(input: $input) { id }
}
"#;

const M_DELETE_STROKE: &str = r#"
mutation DeleteRemoteStroke($input: DeleteRemoteStrokeInput!) {
  deleteRemoteStroke(input: $input) { id }
}
"#;

const M_UPDATE_STROKE: &str = r#"
mutation UpdateRemoteStroke($input: UpdateRemoteStrokeInput!) {
  updateRemoteStroke(input: $input) { id }
}
"#;

const M_UPSERT_STROKES_BATCH: &str = r#"
mutation UpsertStrokesBatch($notebookId: ID!, $items: AWSJSON!, $kind: String) {
  upsertStrokesBatch(notebookId: $notebookId, items: $items, kind: $kind) {
    upserted unprocessed
  }
}
"#;

const M_DELETE_PAGE: &str = r#"
mutation DeleteRemotePage($input: DeleteRemotePageInput!) {
  deleteRemotePage(input: $input) { id }
}
"#;

const M_DELETE_SECTION: &str = r#"
mutation DeleteRemoteSection($input: DeleteRemoteSectionInput!) {
  deleteRemoteSection(input: $input) { id }
}
"#;

// ── Client struct ───────────────────────────────────────────────────

#[cfg(feature = "remote")]
pub struct RemoteNotebookStore {
    config: config::AmplifyOutputs,
    tokens: Option<auth::Tokens>,
}

#[cfg(feature = "remote")]
impl RemoteNotebookStore {
    pub fn connect() -> Result<Self, NotebookSyncError> {
        let config = config::load()?;
        let tokens = auth::load_tokens()?;
        Ok(Self { config, tokens })
    }

    pub fn is_signed_in(&self) -> bool {
        self.tokens.is_some()
    }

    fn now() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn ensure_tokens(&mut self) -> Result<&auth::Tokens, NotebookSyncError> {
        let needs_refresh = match &self.tokens {
            Some(t) => t.needs_refresh(Self::now(), 60),
            None => return Err(NotebookSyncError::NotSignedIn),
        };
        if needs_refresh {
            let current = self.tokens.as_ref().expect("checked above").clone();
            let refreshed = auth::refresh(
                &self.config.auth_region,
                &self.config.user_pool_client_id,
                &current,
            )?;
            self.tokens = Some(refreshed);
        }
        Ok(self.tokens.as_ref().expect("set above"))
    }

    fn gql(
        &mut self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, NotebookSyncError> {
        let id_token = self.ensure_tokens()?.id_token.clone();
        let data_url = self.config.data_url.clone();
        Ok(graphql::post(&data_url, &id_token, query, None, variables)?)
    }

    /// Try createX; on conflict (row already exists) fall back to
    /// updateX. Both shapes always include `id` so the AppSync
    /// resolver routes the right row.
    fn upsert(
        &mut self,
        create_query: &str,
        update_query: &str,
        input: serde_json::Value,
    ) -> Result<(), NotebookSyncError> {
        let create_res = self.gql(create_query, serde_json::json!({ "input": input.clone() }));
        match create_res {
            Ok(_) => Ok(()),
            Err(NotebookSyncError::GraphQl(graphql::GraphQlError::Service(msg)))
                if msg.contains("ConditionalCheckFailedException")
                    || msg.contains("already exists")
                    || msg.contains("ConditionalCheckFailed") =>
            {
                self.gql(update_query, serde_json::json!({ "input": input }))?;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Look up a remote notebook header. Returns the current visibility
    /// (so callers can reuse it when re-syncing without prompting the
    /// user again). `Ok(None)` means the row doesn't exist yet.
    pub fn get_notebook_visibility(
        &mut self,
        id: NotebookId,
    ) -> Result<Option<NotebookVisibility>, NotebookSyncError> {
        let v = self.gql(
            Q_GET_NOTEBOOK,
            serde_json::json!({ "id": id.0.to_string() }),
        )?;
        let item = v.get("getNotebook");
        let Some(item) = item else { return Ok(None) };
        if item.is_null() {
            return Ok(None);
        }
        let vis = item
            .get("visibility")
            .and_then(|x| x.as_str())
            .ok_or_else(|| NotebookSyncError::Encode("getNotebook missing visibility".into()))?;
        Ok(Some(match vis {
            "PRIVATE" => NotebookVisibility::Private,
            "UNLISTED" => NotebookVisibility::Unlisted,
            "PUBLIC" => NotebookVisibility::Public,
            other => {
                return Err(NotebookSyncError::Encode(format!(
                    "unknown visibility {other}"
                )))
            }
        }))
    }

    /// Push the notebook header + every section/page/stroke to DDB.
    /// Idempotent — re-runs upsert each row. `on_progress` fires per
    /// phase + per-row so a UI can render a progress bar.
    /// `deleted_stroke_ids` is the local soft-delete tombstone list;
    /// each id is sent as a cloud-delete and added to the
    /// `successfully_deleted` field of the report so the caller can
    /// `purge_deleted_stroke` locally.
    // Each argument carries a distinct cloud-sync concern (notebook
    // header / per-table rows / tombstones / visibility / progress).
    // Bundling them into a context struct would force the caller to
    // build that struct only to immediately destructure it inside —
    // not worth it. Revisit if the count grows past 10.
    #[allow(clippy::too_many_arguments)]
    pub fn sync_notebook(
        &mut self,
        notebook: &Notebook,
        sections: &[Section],
        pages: &[Page],
        strokes_per_page: &[(PageId, Vec<Stroke>)],
        deleted_stroke_ids: &[Uuid],
        visibility: NotebookVisibility,
        on_progress: &mut dyn FnMut(SyncProgress),
    ) -> Result<SyncReport, NotebookSyncError> {
        let total_strokes: usize = strokes_per_page.iter().map(|(_, s)| s.len()).sum();
        let now = Utc::now().to_rfc3339();
        let kind_json = serde_json::to_string(&notebook.kind)
            .map_err(|e| NotebookSyncError::Encode(format!("notebook.kind: {e}")))?;
        let assigned_json = serde_json::to_string(&notebook.assigned_templates)
            .map_err(|e| NotebookSyncError::Encode(format!("assigned_templates: {e}")))?;

        // Pull the remote state up-front so every subsequent phase can
        // diff before upserting. Skipping unchanged sections / pages /
        // strokes turns a no-op resync into ~zero AppSync writes.
        on_progress(SyncProgress::Phase {
            label: "Diffing remote state".into(),
            total: 0,
        });
        let pre_pull = self.pull_notebook(notebook.id)?;
        let remote_sections: std::collections::HashMap<String, serde_json::Value> = pre_pull
            .as_ref()
            .map(|p| {
                p.sections
                    .iter()
                    .filter_map(|v| {
                        v.get("id")
                            .and_then(|x| x.as_str())
                            .map(|id| (id.to_string(), v.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let remote_pages: std::collections::HashMap<String, serde_json::Value> = pre_pull
            .as_ref()
            .map(|p| {
                p.pages
                    .iter()
                    .filter_map(|v| {
                        v.get("id")
                            .and_then(|x| x.as_str())
                            .map(|id| (id.to_string(), v.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let remote_stroke_ids: std::collections::HashSet<Uuid> = pre_pull
            .as_ref()
            .map(|p| p.strokes.iter().map(|s| s.id).collect())
            .unwrap_or_default();

        // Notebook header.
        on_progress(SyncProgress::Phase {
            label: "Uploading notebook header".into(),
            total: 1,
        });
        let nb_input = serde_json::json!({
            "id": notebook.id.0.to_string(),
            "name": notebook.name,
            "description": "",
            "visibility": visibility.as_gql(),
            "kindJson": kind_json,
            "assignedTemplatesJson": assigned_json,
            "updatedAtSort": now,
        });
        self.upsert(M_CREATE_NOTEBOOK, M_UPDATE_NOTEBOOK, nb_input)?;
        on_progress(SyncProgress::Step { done: 1, total: 1 });

        // Sections — skip upsert when local matches remote on the
        // fields that drive ordering / display.
        on_progress(SyncProgress::Phase {
            label: "Uploading sections".into(),
            total: sections.len(),
        });
        let mut sections_n = 0usize;
        for s in sections {
            let allowed_json = serde_json::to_string(&s.allowed_templates)
                .map_err(|e| NotebookSyncError::Encode(format!("section.allowed: {e}")))?;
            let parent_str = s.parent_section_id.map(|p| p.0.to_string());
            let id_str = s.id.0.to_string();
            let needs_upsert = match remote_sections.get(&id_str) {
                None => true,
                Some(rv) => {
                    let r_name = rv.get("name").and_then(|x| x.as_str()).unwrap_or("");
                    let r_pos = rv.get("position").and_then(|x| x.as_i64()).unwrap_or(-1);
                    let r_parent = rv
                        .get("parentSectionId")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string());
                    let r_allowed = rv
                        .get("allowedTemplatesJson")
                        .and_then(|x| x.as_str())
                        .unwrap_or("");
                    r_name != s.name
                        || r_pos != s.position as i64
                        || r_parent != parent_str
                        || r_allowed != allowed_json
                }
            };
            if !needs_upsert {
                sections_n += 1;
                on_progress(SyncProgress::Step {
                    done: sections_n,
                    total: sections.len(),
                });
                continue;
            }
            let input = serde_json::json!({
                "id": id_str,
                "notebookId": s.notebook_id.0.to_string(),
                "parentSectionId": parent_str,
                "name": s.name,
                "position": s.position,
                "allowedTemplatesJson": allowed_json,
            });
            self.upsert(M_CREATE_SECTION, M_UPDATE_SECTION, input)?;
            sections_n += 1;
            on_progress(SyncProgress::Step {
                done: sections_n,
                total: sections.len(),
            });
        }

        // Pages — same diff strategy.
        on_progress(SyncProgress::Phase {
            label: "Uploading pages".into(),
            total: pages.len(),
        });
        let mut pages_n = 0usize;
        for p in pages {
            let planner_json = serde_json::to_string(&p.planner_address)
                .map_err(|e| NotebookSyncError::Encode(format!("page.planner: {e}")))?;
            let widget_overrides_json = serde_json::to_string(&p.widget_overrides)
                .map_err(|e| NotebookSyncError::Encode(format!("page.widget_overrides: {e}")))?;
            let widget_data_json = serde_json::to_string(&p.widget_data)
                .map_err(|e| NotebookSyncError::Encode(format!("page.widget_data: {e}")))?;
            let id_str = p.id.0.to_string();
            let template_str = p.template_id.map(|t| t.0.to_string());
            let needs_upsert = match remote_pages.get(&id_str) {
                None => true,
                Some(rv) => {
                    let r_name = rv.get("name").and_then(|x| x.as_str()).unwrap_or("");
                    let r_pos = rv.get("position").and_then(|x| x.as_i64()).unwrap_or(-1);
                    let r_section = rv.get("sectionId").and_then(|x| x.as_str()).unwrap_or("");
                    let r_template = rv
                        .get("templateId")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string());
                    let r_planner = rv
                        .get("plannerAddressJson")
                        .and_then(|x| x.as_str())
                        .unwrap_or("");
                    let r_overrides = rv
                        .get("widgetOverridesJson")
                        .and_then(|x| x.as_str())
                        .unwrap_or("");
                    let r_data = rv
                        .get("widgetDataJson")
                        .and_then(|x| x.as_str())
                        .unwrap_or("");
                    let r_flagged = rv.get("flagged").and_then(|x| x.as_bool()).unwrap_or(false);
                    r_name != p.name
                        || r_pos != p.position as i64
                        || r_section != p.section_id.0.to_string()
                        || r_template != template_str
                        || r_planner != planner_json
                        || r_overrides != widget_overrides_json
                        || r_data != widget_data_json
                        || r_flagged != p.flagged
                }
            };
            if !needs_upsert {
                pages_n += 1;
                on_progress(SyncProgress::Step {
                    done: pages_n,
                    total: pages.len(),
                });
                continue;
            }
            let notebook_id = sections
                .iter()
                .find(|s| s.id == p.section_id)
                .map(|s| s.notebook_id.0.to_string())
                .unwrap_or_else(|| notebook.id.0.to_string());
            let input = serde_json::json!({
                "id": id_str,
                "notebookId": notebook_id,
                "sectionId": p.section_id.0.to_string(),
                "templateId": template_str,
                "position": p.position,
                "name": p.name,
                "plannerAddressJson": planner_json,
                "widgetOverridesJson": widget_overrides_json,
                "widgetDataJson": widget_data_json,
                "flagged": p.flagged,
                "createdAtIso": p.created_at.to_rfc3339(),
                "modifiedAtIso": p.modified_at.to_rfc3339(),
            });
            self.upsert(M_CREATE_PAGE, M_UPDATE_PAGE, input)?;
            pages_n += 1;
            on_progress(SyncProgress::Step {
                done: pages_n,
                total: pages.len(),
            });
        }

        let net_new_strokes: usize = strokes_per_page
            .iter()
            .map(|(_, ss)| {
                ss.iter()
                    .filter(|s| !remote_stroke_ids.contains(&s.id))
                    .count()
            })
            .sum();
        on_progress(SyncProgress::Phase {
            label: format!(
                "Uploading strokes ({} new, {} already in cloud)",
                net_new_strokes,
                total_strokes - net_new_strokes
            ),
            total: net_new_strokes,
        });
        // Route new strokes through the Lambda-backed batch upsert
        // (same code path that powers live sync) so the daily-usage
        // counter actually sees them. The previous M_CREATE_STROKE
        // auto-CRUD mutation bypassed the Lambda entirely, which left
        // the billing page reporting zero strokes regardless of how
        // many a free user pushed in a save. `kind: "snapshot"` keeps
        // the cap-rejection off for manual saves.
        let mut strokes_n = 0usize;
        let mut strokes_done = 0usize;
        const CREATE_BATCH: usize = 25;
        let mut new_items: Vec<UpsertStrokeItem> = Vec::new();
        for (page_id, strokes) in strokes_per_page {
            for st in strokes {
                if remote_stroke_ids.contains(&st.id) {
                    continue;
                }
                let body_json = serde_json::to_string(st)
                    .map_err(|e| NotebookSyncError::Encode(format!("stroke: {e}")))?;
                new_items.push(UpsertStrokeItem {
                    id: st.id,
                    page_id: *page_id,
                    payload: body_json,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    deleted_at: None,
                });
            }
        }
        for chunk in new_items.chunks(CREATE_BATCH) {
            match self.upsert_strokes_batch_with_kind(notebook.id, chunk, "snapshot") {
                Ok((upserted, _unprocessed, _failed_ids)) => {
                    strokes_n += upserted;
                }
                Err(e) => {
                    tracing::warn!(
                        "sync_notebook batch creates failed for chunk of {}: {}",
                        chunk.len(),
                        e
                    );
                }
            }
            strokes_done += chunk.len();
            on_progress(SyncProgress::Step {
                done: strokes_done,
                total: net_new_strokes,
            });
        }

        // Cleanup: delete any remote rows that don't have a local
        // counterpart any more. Without this, deleting a page or
        // stroke locally leaves orphans in DDB that the web viewer
        // would still render.
        let local_section_ids: std::collections::HashSet<String> =
            sections.iter().map(|s| s.id.0.to_string()).collect();
        let local_page_ids: std::collections::HashSet<String> =
            pages.iter().map(|p| p.id.0.to_string()).collect();
        let local_stroke_ids: std::collections::HashSet<String> = strokes_per_page
            .iter()
            .flat_map(|(_, ss)| ss.iter().map(|s| s.id.to_string()))
            .collect();

        // Phase: push every locally-tombstoned stroke as a cloud
        // delete via the Lambda-backed `syncStrokesBatch` mutation.
        // Batches of 25 (DDB BatchWriteItem cap) → N/25 round-trips
        // instead of N. The Lambda treats already-gone rows as
        // success, so we mark every id as purgeable on a clean
        // batch return.
        //
        // Filter the tombstones against the pre-pull's `remote_stroke_ids`
        // so we don't repeatedly push deletes the cloud already
        // applied. Without this, a save with no real changes still
        // re-stamps every prior delete with `now_iso` — every row
        // succeeds (newer timestamp) and the daily-usage counter
        // bumps for work the user didn't redo.
        let mut strokes_purged: Vec<Uuid> = Vec::new();
        let deletes_to_push: Vec<Uuid> = deleted_stroke_ids
            .iter()
            .copied()
            .filter(|id| remote_stroke_ids.contains(id))
            .collect();
        on_progress(SyncProgress::Phase {
            label: format!("Pushing {} local deletes (batched)", deletes_to_push.len()),
            total: deletes_to_push.len(),
        });
        const BATCH: usize = 25;
        let now_iso = chrono::Utc::now().to_rfc3339();
        let mut deletes_done = 0usize;
        for chunk in deletes_to_push.chunks(BATCH) {
            // Soft-delete = upsert with deleted_at set. Empty
            // payload is fine — the row already exists with the
            // last known body. The cloud just bumps deleted_at.
            let items: Vec<UpsertStrokeItem> = chunk
                .iter()
                .map(|id| UpsertStrokeItem {
                    id: *id,
                    page_id: PageId(Uuid::nil()),
                    payload: String::new(),
                    created_at: now_iso.clone(),
                    updated_at: now_iso.clone(),
                    deleted_at: Some(now_iso.clone()),
                })
                .collect();
            match self.upsert_strokes_batch_with_kind(notebook.id, &items, "snapshot") {
                Ok((_upserted, unprocessed, _failed_ids)) => {
                    let processed = chunk.len().saturating_sub(unprocessed);
                    strokes_purged.extend(chunk[..processed].iter().copied());
                    if unprocessed > 0 {
                        tracing::warn!(
                            "sync_notebook batch deletes: {} unprocessed in chunk of {}",
                            unprocessed,
                            chunk.len()
                        );
                    }
                }
                Err(e) => tracing::warn!(
                    "sync_notebook batch deletes failed for chunk of {}: {}",
                    chunk.len(),
                    e
                ),
            }
            deletes_done += chunk.len();
            on_progress(SyncProgress::Step {
                done: deletes_done,
                total: deletes_to_push.len(),
            });
        }
        // Locally-tombstoned ids that were never in the cloud (or
        // were already deleted in a prior sync) can still be purged
        // locally — their tombstone has nothing to chase.
        strokes_purged.extend(
            deleted_stroke_ids
                .iter()
                .filter(|id| !remote_stroke_ids.contains(*id))
                .copied(),
        );

        on_progress(SyncProgress::Phase {
            label: "Cleaning up orphan rows".into(),
            total: 0,
        });
        // Reuse the pre-pull from the strokes phase rather than
        // round-tripping a second pull_notebook just for cleanup.
        // Local soft-deleted strokes don't appear in `local_stroke_ids`
        // (those count only live strokes), so this also catches any
        // stroke the user erased before we had the persistent queue.
        if let Some(pulled) = pre_pull {
            let remote_secs: Vec<String> = pulled
                .sections
                .iter()
                .filter_map(|v| v.get("id").and_then(|x| x.as_str()).map(String::from))
                .collect();
            let remote_pages: Vec<String> = pulled
                .pages
                .iter()
                .filter_map(|v| v.get("id").and_then(|x| x.as_str()).map(String::from))
                .collect();
            let remote_strokes: Vec<String> =
                pulled.strokes.iter().map(|s| s.id.to_string()).collect();

            // Batch the cloud-orphan stroke deletes through the
            // Lambda — same reasoning as the local-tombstone push
            // above, just for ids that exist remotely but not
            // locally.
            let orphan_ids: Vec<Uuid> = remote_strokes
                .iter()
                .filter(|id| !local_stroke_ids.contains(*id))
                .filter_map(|s| Uuid::parse_str(s).ok())
                .collect();
            for chunk in orphan_ids.chunks(BATCH) {
                let items: Vec<UpsertStrokeItem> = chunk
                    .iter()
                    .map(|id| UpsertStrokeItem {
                        id: *id,
                        page_id: PageId(Uuid::nil()),
                        payload: String::new(),
                        created_at: now_iso.clone(),
                        updated_at: now_iso.clone(),
                        deleted_at: Some(now_iso.clone()),
                    })
                    .collect();
                let _ = self.upsert_strokes_batch(notebook.id, &items);
            }
            for id in remote_pages
                .iter()
                .filter(|id| !local_page_ids.contains(*id))
            {
                let _ = self.gql(M_DELETE_PAGE, serde_json::json!({ "input": { "id": id } }));
            }
            for id in remote_secs
                .iter()
                .filter(|id| !local_section_ids.contains(*id))
            {
                let _ = self.gql(
                    M_DELETE_SECTION,
                    serde_json::json!({ "input": { "id": id } }),
                );
            }
        }

        Ok(SyncReport {
            sections_upserted: sections_n,
            pages_upserted: pages_n,
            strokes_upserted: strokes_n,
            visibility: Some(visibility.as_gql().to_string()),
            strokes_purged,
        })
    }

    /// Best-effort: fire one `RemoteStroke.create` per local stroke
    /// commit. Caller logs and continues on failure — drawing latency
    /// must not block on the network.
    pub fn publish_stroke_create(
        &mut self,
        notebook_id: NotebookId,
        page_id: PageId,
        stroke: &Stroke,
    ) -> Result<(), NotebookSyncError> {
        let body_json = serde_json::to_string(stroke)
            .map_err(|e| NotebookSyncError::Encode(format!("stroke: {e}")))?;
        let input = serde_json::json!({
            "id": stroke.id.to_string(),
            "notebookId": notebook_id.0.to_string(),
            "pageId": page_id.0.to_string(),
            "strokeJson": body_json,
            "createdAt": Utc::now().to_rfc3339(),
        });
        self.gql(M_CREATE_STROKE, serde_json::json!({ "input": input }))?;
        Ok(())
    }

    /// Update the visibility on the remote Notebook row only — no
    /// section / page / stroke writes. Cheap "change who can see this"
    /// path used by the notebook settings entry.
    pub fn set_visibility(
        &mut self,
        id: NotebookId,
        visibility: NotebookVisibility,
    ) -> Result<(), NotebookSyncError> {
        let now = Utc::now().to_rfc3339();
        let input = serde_json::json!({
            "id": id.0.to_string(),
            "visibility": visibility.as_gql(),
            "updatedAtSort": now,
        });
        self.gql(M_UPDATE_NOTEBOOK, serde_json::json!({ "input": input }))?;
        Ok(())
    }

    /// Last-writer-wins upsert. One mutation handles creates, updates,
    /// and tombstones — every item carries its own `updated_at`, so
    /// AppSync subscribers can ignore out-of-order events. No
    /// separate delete path. Returns (upserted, unprocessed).
    pub fn upsert_strokes_batch(
        &mut self,
        notebook_id: NotebookId,
        items: &[UpsertStrokeItem],
    ) -> Result<(usize, usize, Vec<Uuid>), NotebookSyncError> {
        self.upsert_strokes_batch_with_kind(notebook_id, items, "live")
    }

    /// Variant that lets the caller mark the batch as a `"snapshot"`
    /// (explicit manual save) instead of the default `"live"` (live
    /// sync). Snapshot batches still increment usage counters but
    /// bypass the daily-write cap rejection — used by the initial
    /// notebook push so a free user with a fresh notebook can save
    /// without slamming into the 1k/day cap.
    pub fn upsert_strokes_batch_with_kind(
        &mut self,
        notebook_id: NotebookId,
        items: &[UpsertStrokeItem],
        kind: &str,
    ) -> Result<(usize, usize, Vec<Uuid>), NotebookSyncError> {
        if items.is_empty() {
            return Ok((0, 0, Vec::new()));
        }
        let items_json: Vec<serde_json::Value> = items
            .iter()
            .map(|it| {
                serde_json::json!({
                    "id": it.id.to_string(),
                    "pageId": it.page_id.0.to_string(),
                    "strokeJson": it.payload,
                    "createdAt": it.created_at,
                    "updatedAtIso": it.updated_at,
                    "deletedAtIso": it.deleted_at,
                })
            })
            .collect();
        let payload = serde_json::to_string(&items_json)
            .map_err(|e| NotebookSyncError::Encode(format!("items: {e}")))?;
        let n = items.len();
        let payload_len = payload.len();
        tracing::info!(
            "STROKE_MOD http upsert_strokes_batch: -> AppSync notebook={:?} items={} payload_bytes={} kind={}",
            notebook_id,
            n,
            payload_len,
            kind,
        );
        let t0 = std::time::Instant::now();
        let v = self.gql(
            M_UPSERT_STROKES_BATCH,
            serde_json::json!({
                "notebookId": notebook_id.0.to_string(),
                "items": payload,
                "kind": kind,
            }),
        )?;
        let elapsed = t0.elapsed();
        let item = v
            .get("upsertStrokesBatch")
            .ok_or_else(|| {
                tracing::warn!(
                    "STROKE_MOD http upsert_strokes_batch: missing 'upsertStrokesBatch' in response: {}",
                    v
                );
                NotebookSyncError::Encode("missing upsertStrokesBatch".into())
            })?;
        let upserted = item.get("upserted").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let unprocessed = item
            .get("unprocessed")
            .and_then(|x| x.as_u64())
            .unwrap_or(0) as usize;
        let failed_ids: Vec<Uuid> = item
            .get("failedIds")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                    .collect()
            })
            .unwrap_or_default();
        tracing::info!(
            "STROKE_MOD http upsert_strokes_batch: <- AppSync upserted={} unprocessed={} failedIds={} elapsed={:?} (sent {} items)",
            upserted,
            unprocessed,
            failed_ids.len(),
            elapsed,
            n
        );
        if unprocessed > 0 {
            tracing::warn!(
                "STROKE_MOD http upsert_strokes_batch: {} of {} items UNPROCESSED for {:?}; failed_ids={:?}",
                unprocessed,
                n,
                notebook_id,
                failed_ids
            );
        }
        Ok((upserted, unprocessed, failed_ids))
    }

    /// Best-effort delete for an erased stroke.
    pub fn publish_stroke_delete(&mut self, stroke_id: Uuid) -> Result<(), NotebookSyncError> {
        let input = serde_json::json!({ "id": stroke_id.to_string() });
        self.gql(M_DELETE_STROKE, serde_json::json!({ "input": input }))?;
        Ok(())
    }

    /// Best-effort update — fires when a stroke's geometry mutates
    /// (move / scale / partial-erase replacement / lasso split).
    /// Sends only `id` + `strokeJson`; other fields stay as-is on
    /// the remote row.
    pub fn publish_stroke_update(&mut self, stroke: &Stroke) -> Result<(), NotebookSyncError> {
        let body_json = serde_json::to_string(stroke)
            .map_err(|e| NotebookSyncError::Encode(format!("stroke: {e}")))?;
        let input = serde_json::json!({
            "id": stroke.id.to_string(),
            "strokeJson": body_json,
        });
        self.gql(M_UPDATE_STROKE, serde_json::json!({ "input": input }))?;
        Ok(())
    }

    /// Pull every section / page / stroke for a notebook from the
    /// cloud. The caller diffs against local state and inserts any
    /// missing rows. Returns `Ok(None)` when the notebook isn't in
    /// the cloud at all (treat as "nothing to pull").
    pub fn pull_notebook(
        &mut self,
        id: NotebookId,
    ) -> Result<Option<PulledNotebook>, NotebookSyncError> {
        // Header first — bail out cheap if the notebook isn't
        // synced anywhere yet.
        let v = self.gql(
            Q_GET_NOTEBOOK,
            serde_json::json!({ "id": id.0.to_string() }),
        )?;
        let item = v.get("getNotebook");
        let Some(item) = item else { return Ok(None) };
        if item.is_null() {
            return Ok(None);
        }

        let id_str = id.0.to_string();
        let secs_v = self.gql(Q_LIST_SECTIONS, serde_json::json!({ "notebookId": id_str }))?;
        let pages_v = self.gql(Q_LIST_PAGES, serde_json::json!({ "notebookId": id_str }))?;

        // Strokes paginate; loop until nextToken null.
        let mut strokes: Vec<PulledStroke> = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let vars = match token.as_deref() {
                Some(t) => serde_json::json!({ "notebookId": id_str, "nextToken": t }),
                None => serde_json::json!({ "notebookId": id_str, "nextToken": null }),
            };
            let v = self.gql(Q_LIST_STROKES, vars)?;
            let conn = v
                .get("listRemoteStrokesByNotebook")
                .ok_or_else(|| NotebookSyncError::Encode("missing strokes connection".into()))?;
            let items = conn
                .get("items")
                .and_then(|x| x.as_array())
                .ok_or_else(|| NotebookSyncError::Encode("missing strokes items".into()))?;
            for it in items {
                let id_s = it.get("id").and_then(|x| x.as_str()).unwrap_or_default();
                let page_id = it
                    .get("pageId")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default();
                // Skip tombstones — `deletedAtIso` set means the row
                // is soft-deleted in cloud + payload may be empty.
                if it.get("deletedAtIso").and_then(|x| x.as_str()).is_some() {
                    continue;
                }
                let stroke_json = it
                    .get("strokeJson")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default();
                if stroke_json.is_empty() {
                    // Defensive: legacy empty bodies that aren't
                    // tagged as tombstones — drop instead of erroring.
                    tracing::debug!("pull: skipping stroke {} with empty body", id_s);
                    continue;
                }
                let stroke_id = Uuid::parse_str(id_s)
                    .map_err(|e| NotebookSyncError::Encode(format!("stroke id: {e}")))?;
                let pg_id = Uuid::parse_str(page_id)
                    .map_err(|e| NotebookSyncError::Encode(format!("page id: {e}")))?;
                let stroke: Stroke = serde_json::from_str(stroke_json)
                    .map_err(|e| NotebookSyncError::Encode(format!("stroke body: {e}")))?;
                strokes.push(PulledStroke {
                    id: stroke_id,
                    page_id: PageId(pg_id),
                    stroke,
                });
            }
            token = conn
                .get("nextToken")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            if token.is_none() {
                break;
            }
        }

        let sections = secs_v
            .pointer("/listRemoteSectionsByNotebook/items")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        let pages = pages_v
            .pointer("/listRemotePagesByNotebook/items")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(Some(PulledNotebook {
            sections,
            pages,
            strokes,
        }))
    }
}

/// Result of [`RemoteNotebookStore::pull_notebook`]. Sections / pages
/// stay as raw JSON values for now — the immediate caller only needs
/// the stroke deltas to warm up the canvas; richer page-level pulls
/// are a follow-up.
#[derive(Debug, Default)]
pub struct PulledNotebook {
    pub sections: Vec<serde_json::Value>,
    pub pages: Vec<serde_json::Value>,
    pub strokes: Vec<PulledStroke>,
}

#[derive(Debug)]
pub struct PulledStroke {
    pub id: Uuid,
    pub page_id: PageId,
    pub stroke: Stroke,
}

// `Section` / `SectionId` use only their public fields above; pull the
// SectionId import into scope so the `id` accessor compiles.
#[allow(dead_code)]
fn _force_use_section_id(s: &Section) -> SectionId {
    s.id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_gql_strings() {
        assert_eq!(NotebookVisibility::Private.as_gql(), "PRIVATE");
        assert_eq!(NotebookVisibility::Unlisted.as_gql(), "UNLISTED");
        assert_eq!(NotebookVisibility::Public.as_gql(), "PUBLIC");
    }
}
