//! Per-user entitlement query + sync-budget gate.
//!
//! Two responsibilities:
//!
//! 1. Fetch + cache the caller's `UserEntitlement` row from AppSync so
//!    feature gates (live sync, notebook count, daily writes, etc.)
//!    have authoritative cap numbers.
//! 2. Parse paywall errors (`QuotaExceeded`, `SubscriptionInactive`,
//!    `NOTEBOOK_LIMIT`, ...) emitted by the Lambda + JS pipeline
//!    resolvers, and persist a [`SyncBudget`] so the desktop pauses
//!    auto-sync until `resets_at` (or restart past that time).
//!
//! `Entitlement` + `SyncBudget` are usable in non-remote builds (just
//! defaults). Network fetch is gated behind the `remote` feature.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entitlement {
    pub id: String,
    pub tier: String,
    pub status: String,
    #[serde(default)]
    pub period_end: Option<String>,
    #[serde(default)]
    pub trial_ends_at: Option<String>,
    #[serde(default)]
    pub education_verified: bool,
    pub notebook_cap: u64,
    pub strokes_per_page_cap: u64,
    pub strokes_per_notebook_cap: u64,
    pub daily_write_cap: u64,
    pub s3_bytes_cap: u64,
    /// `-1` means unlimited.
    pub template_publish_cap: i64,
    #[serde(default)]
    pub history_days: u64,
    #[serde(default)]
    pub live_sync_enabled: bool,
}

impl Entitlement {
    /// Free-tier defaults used when no row exists yet (new user
    /// pre-checkout) or when the network is unavailable.
    pub fn free_default(sub: String) -> Self {
        Self {
            id: sub,
            tier: "free".into(),
            status: "active".into(),
            period_end: None,
            trial_ends_at: None,
            education_verified: false,
            notebook_cap: 1,
            strokes_per_page_cap: 10_000,
            strokes_per_notebook_cap: 50_000,
            daily_write_cap: 1_000,
            s3_bytes_cap: 50 * 1024 * 1024,
            template_publish_cap: 3,
            history_days: 0,
            live_sync_enabled: false,
        }
    }

    pub fn is_unlimited_publishes(&self) -> bool {
        self.template_publish_cap < 0
    }
}

/// Persistent per-user sync gate. Set by [`EntitlementService::record_block`]
/// on a paywall error; cleared automatically once `disabled_until`
/// passes (UTC midnight for daily-write blocks) or via
/// [`EntitlementService::clear_block`] on an admin override.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBudget {
    pub disabled_until: Option<DateTime<Utc>>,
    pub reason_code: Option<String>,
    pub limit: Option<u64>,
    pub current: Option<u64>,
    pub upgrade_url: Option<String>,
}

impl SyncBudget {
    pub fn is_blocked(&self) -> bool {
        match self.disabled_until {
            Some(until) => Utc::now() < until,
            None => self.reason_code.is_some(),
        }
    }

    /// Wipes the block if the reset time has passed. Returns `true`
    /// if cleared (useful for telling the UI to drop the banner).
    pub fn clear_if_expired(&mut self) -> bool {
        if let Some(until) = self.disabled_until {
            if Utc::now() >= until {
                *self = Self::default();
                return true;
            }
        }
        false
    }

    /// Default on-disk location for the budget snapshot. Lives next
    /// to other config under `~/.config/journal/sync_budget.json`.
    /// Returns `None` if no config dir is available.
    pub fn default_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("journal").join("sync_budget.json"))
    }

    /// Persist this budget to the default config path, creating
    /// parent directories as needed. Errors are logged via `tracing`
    /// but not returned — sync workers shouldn't fail their primary
    /// task because the budget side-channel couldn't write.
    pub fn save_to_default(&self) {
        let Some(path) = Self::default_path() else {
            tracing::warn!("sync budget: no config dir, skipping persist");
            return;
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("sync budget mkdir: {e}");
                return;
            }
        }
        match serde_json::to_vec_pretty(self) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(&path, bytes) {
                    tracing::warn!("sync budget write: {e}");
                }
            }
            Err(e) => tracing::warn!("sync budget serialize: {e}"),
        }
    }

    /// Load the most recent persisted budget, returning the default
    /// (no block) on any error. Expired blocks are auto-cleared as a
    /// side effect of [`Self::clear_if_expired`].
    pub fn load_from_default() -> Self {
        let Some(path) = Self::default_path() else {
            return Self::default();
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        let mut b: Self = serde_json::from_slice(&bytes).unwrap_or_default();
        b.clear_if_expired();
        b
    }
}

/// Structured paywall error body. AppSync's direct-Lambda integration
/// flattens errors to `{ errorType, message }`; Lambdas put the body
/// in `message` as JSON. JS resolver pipeline steps emit the same
/// shape but via the native `errors[].errorInfo` field — we read both.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaywallError {
    #[serde(default)]
    pub error: String,
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub current: Option<u64>,
    #[serde(default)]
    pub resets_at: Option<String>,
    #[serde(default)]
    pub upgrade_url: Option<String>,
}

impl PaywallError {
    /// Try every item in a serialized GraphQL `errors` array. Used
    /// to peel a paywall body off `GraphQlError::Service`, whose
    /// payload is the JSON-encoded errors array as a string.
    pub fn from_graphql_service_message(msg: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(msg).ok()?;
        let arr = value.as_array()?;
        arr.iter().find_map(Self::from_error_item)
    }

    /// Extract a paywall body from one item of a GraphQL `errors`
    /// array. Tries the Lambda convention (JSON-encoded `message`)
    /// first, then the JS resolver convention (`errorInfo` map).
    /// Returns `None` if the error isn't one of our paywall types.
    pub fn from_error_item(err: &serde_json::Value) -> Option<Self> {
        let error_type = err.get("errorType").and_then(|v| v.as_str()).unwrap_or("");
        // JS resolver convention wins when present: a structured
        // `errorInfo` map carries the body, the `message` is free
        // text. Lambda convention puts the JSON body in `message`
        // directly. Both can share an `errorType` so we discriminate
        // by which payload field exists.
        if let Some(info) = err.get("errorInfo") {
            if let Some(code) = info.get("code").and_then(|v| v.as_str()) {
                return Some(PaywallError {
                    error: error_type.to_string(),
                    code: code.to_string(),
                    message: err
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    limit: info.get("limit").and_then(|v| v.as_u64()),
                    current: info.get("current").and_then(|v| v.as_u64()),
                    resets_at: info
                        .get("resetsAt")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    upgrade_url: info
                        .get("upgradeUrl")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                });
            }
        }
        let lambda_types = [
            "QuotaExceeded",
            "SubscriptionInactive",
            "Lambda:QuotaExceeded",
            "Lambda:SubscriptionInactive",
        ];
        if lambda_types.contains(&error_type) {
            let msg = err.get("message").and_then(|v| v.as_str())?;
            return serde_json::from_str(msg).ok();
        }
        None
    }
}

#[cfg(feature = "remote")]
pub use service::EntitlementService;

#[cfg(feature = "remote")]
mod service {
    use super::{Entitlement, PaywallError, SyncBudget};
    use crate::remote_template_store::graphql::{post, GraphQlError};
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const ENTITLEMENT_QUERY: &str = r#"
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

    const CACHE_TTL: Duration = Duration::from_secs(300);

    pub struct EntitlementService {
        data_url: String,
        cache_path: PathBuf,
        state: Mutex<State>,
    }

    #[derive(Default)]
    struct State {
        entitlement: Option<Entitlement>,
        fetched_at: Option<Instant>,
        budget: SyncBudget,
    }

    #[derive(Serialize, Deserialize, Default)]
    struct Persisted {
        budget: SyncBudget,
    }

    impl EntitlementService {
        /// `data_url` = AppSync GraphQL endpoint (from
        /// `amplify_outputs.json::data_url`). `config_dir` = where to
        /// persist the `SyncBudget` between restarts (typically
        /// `~/.config/journal/`).
        pub fn new(data_url: String, config_dir: PathBuf) -> Self {
            let cache_path = config_dir.join("entitlement.json");
            let mut state = State::default();
            if let Ok(bytes) = std::fs::read(&cache_path) {
                if let Ok(persisted) = serde_json::from_slice::<Persisted>(&bytes) {
                    state.budget = persisted.budget;
                }
            }
            state.budget.clear_if_expired();
            Self {
                data_url,
                cache_path,
                state: Mutex::new(state),
            }
        }

        /// Returns the current sync budget. Clears expired blocks
        /// inline so callers always see a fresh value.
        pub fn budget(&self) -> SyncBudget {
            let mut state = self.state.lock().expect("entitlement state mutex");
            if state.budget.clear_if_expired() {
                self.persist(&state);
            }
            state.budget.clone()
        }

        /// Last-known entitlement without a network call. Returns
        /// `None` if no fetch has succeeded yet — callers should use
        /// [`Entitlement::free_default`] in that case.
        pub fn cached(&self) -> Option<Entitlement> {
            self.state
                .lock()
                .expect("entitlement state mutex")
                .entitlement
                .clone()
        }

        /// Refresh from AppSync. Honors a 5-minute in-memory TTL —
        /// repeated calls inside the window return the cached row.
        pub fn fetch(&self, id_token: &str, sub: &str) -> Result<Entitlement, GraphQlError> {
            {
                let state = self.state.lock().expect("entitlement state mutex");
                if let (Some(ent), Some(at)) =
                    (state.entitlement.as_ref(), state.fetched_at)
                {
                    if at.elapsed() < CACHE_TTL {
                        return Ok(ent.clone());
                    }
                }
            }

            let data = post(
                &self.data_url,
                id_token,
                ENTITLEMENT_QUERY,
                Some("GetMyEntitlement"),
                json!({ "id": sub }),
            )?;

            let row = data
                .get("getUserEntitlement")
                .cloned()
                .unwrap_or(Value::Null);
            let ent: Entitlement = if row.is_null() {
                Entitlement::free_default(sub.to_string())
            } else {
                serde_json::from_value(row).map_err(GraphQlError::from)?
            };

            let mut state = self.state.lock().expect("entitlement state mutex");
            state.entitlement = Some(ent.clone());
            state.fetched_at = Some(Instant::now());
            Ok(ent)
        }

        /// Record a paywall error → set the disable window + persist.
        /// Subsequent `budget()` reads see the block until
        /// `disabled_until` passes.
        pub fn record_block(&self, err: &PaywallError) {
            let disabled_until: Option<DateTime<Utc>> = err
                .resets_at
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc));
            let mut state = self.state.lock().expect("entitlement state mutex");
            state.budget = SyncBudget {
                disabled_until,
                reason_code: Some(err.code.clone()),
                limit: err.limit,
                current: err.current,
                upgrade_url: err.upgrade_url.clone(),
            };
            self.persist(&state);
        }

        /// Drop any current block (admin override, manual retry, etc).
        pub fn clear_block(&self) {
            let mut state = self.state.lock().expect("entitlement state mutex");
            state.budget = SyncBudget::default();
            self.persist(&state);
        }

        fn persist(&self, state: &State) {
            let payload = Persisted {
                budget: state.budget.clone(),
            };
            if let Some(parent) = self.cache_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(bytes) = serde_json::to_vec_pretty(&payload) {
                let _ = std::fs::write(&self.cache_path, bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn free_default_sane() {
        let f = Entitlement::free_default("abc".into());
        assert_eq!(f.tier, "free");
        assert_eq!(f.notebook_cap, 1);
        assert!(!f.live_sync_enabled);
    }

    #[test]
    fn paywall_parses_lambda_message_json() {
        let err = json!({
            "errorType": "QuotaExceeded",
            "message": "{\"error\":\"QuotaExceeded\",\"code\":\"DAILY_WRITE_LIMIT\",\"message\":\"...\",\"limit\":1000,\"current\":1000,\"resetsAt\":\"2026-05-11T00:00:00Z\",\"upgradeUrl\":\"https://journal.app/settings/billing\"}",
        });
        let parsed = PaywallError::from_error_item(&err).expect("parses");
        assert_eq!(parsed.code, "DAILY_WRITE_LIMIT");
        assert_eq!(parsed.limit, Some(1000));
        assert_eq!(parsed.current, Some(1000));
        assert_eq!(parsed.resets_at.as_deref(), Some("2026-05-11T00:00:00Z"));
    }

    #[test]
    fn paywall_parses_js_error_info() {
        let err = json!({
            "errorType": "QuotaExceeded",
            "message": "Daily write limit reached.",
            "errorInfo": {
                "code": "DAILY_WRITE_LIMIT",
                "limit": 1000,
                "resetsAt": "2026-05-11T00:00:00Z",
            },
        });
        let parsed = PaywallError::from_error_item(&err).expect("parses");
        assert_eq!(parsed.code, "DAILY_WRITE_LIMIT");
        assert_eq!(parsed.limit, Some(1000));
    }

    #[test]
    fn paywall_ignores_unrelated_errors() {
        let err = json!({
            "errorType": "Unauthorized",
            "message": "no perms",
        });
        assert!(PaywallError::from_error_item(&err).is_none());
    }

    #[test]
    fn sync_budget_blocked_only_when_disabled_in_future() {
        let mut b = SyncBudget::default();
        assert!(!b.is_blocked());

        b.disabled_until = Some(Utc::now() + chrono::Duration::hours(1));
        b.reason_code = Some("DAILY_WRITE_LIMIT".into());
        assert!(b.is_blocked());

        b.disabled_until = Some(Utc::now() - chrono::Duration::hours(1));
        assert!(b.clear_if_expired());
        assert!(!b.is_blocked());
    }
}
