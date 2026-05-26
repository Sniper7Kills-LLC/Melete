//! Parses the `amplify_outputs.json` produced by Amplify Gen 2
//! (`npx ampx sandbox` / `npx ampx pipeline-deploy`) into a flat
//! [`AmplifyOutputs`] struct the future `RemoteTemplateStore` consumes.
//!
//! Source precedence at runtime:
//!   1. `MELETE_AMPLIFY_OUTPUTS` env var → file path.
//!   2. The build-time embedded copy (resolved by `build.rs`,
//!      surfaced via the `AMPLIFY_OUTPUTS_JSON` rustc env var).
//!
//! If any required field is empty, [`load`] returns
//! [`ConfigError::NotConfigured`] — caller treats this as "remote
//! disabled, fall back to local-only mode".

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmplifyOutputs {
    pub auth_region: String,
    pub user_pool_id: String,
    pub user_pool_client_id: String,
    pub identity_pool_id: String,
    pub data_url: String,
    pub data_region: String,
    pub data_api_key: String,
    pub storage_bucket: String,
    pub storage_region: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("backend not configured (run `npx ampx sandbox` and rebuild)")]
    NotConfigured,
    #[error("amplify_outputs.json malformed: {0}")]
    Malformed(#[from] serde_json::Error),
}

/// Embedded copy. `build.rs` either points this at the resolved
/// repo-root / override path, or at a stub file in `OUT_DIR` with all
/// fields blank.
const EMBEDDED_OUTPUTS: &str = include_str!(env!("AMPLIFY_OUTPUTS_JSON"));

pub fn load() -> Result<AmplifyOutputs, ConfigError> {
    let raw = std::env::var("MELETE_AMPLIFY_OUTPUTS")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| EMBEDDED_OUTPUTS.to_string());
    parse(&raw)
}

fn parse(raw: &str) -> Result<AmplifyOutputs, ConfigError> {
    let v: Value = serde_json::from_str(raw)?;

    let auth = v.get("auth");
    let data = v.get("data");
    let storage = v.get("storage");

    let outputs = AmplifyOutputs {
        auth_region: str_field(auth, "aws_region"),
        user_pool_id: str_field(auth, "user_pool_id"),
        user_pool_client_id: str_field(auth, "user_pool_client_id"),
        identity_pool_id: str_field(auth, "identity_pool_id"),
        data_url: str_field(data, "url"),
        data_region: str_field(data, "aws_region"),
        data_api_key: str_field(data, "api_key"),
        storage_bucket: str_field(storage, "bucket_name"),
        storage_region: str_field(storage, "aws_region"),
    };

    // Required fields. `data_api_key` is optional (Cognito user-pool
    // auth doesn't need one) and is intentionally omitted from this
    // list.
    let required = [
        &outputs.auth_region,
        &outputs.user_pool_id,
        &outputs.user_pool_client_id,
        &outputs.identity_pool_id,
        &outputs.data_url,
        &outputs.data_region,
        &outputs.storage_bucket,
        &outputs.storage_region,
    ];
    if required.iter().any(|s| s.is_empty()) {
        return Err(ConfigError::NotConfigured);
    }
    Ok(outputs)
}

fn str_field(parent: Option<&Value>, key: &str) -> String {
    parent
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_json() -> &'static str {
        r#"{
            "version": "1",
            "auth": {
                "aws_region": "us-west-2",
                "user_pool_id": "us-west-2_abc123",
                "user_pool_client_id": "client123",
                "identity_pool_id": "us-west-2:guid"
            },
            "data": {
                "url": "https://example.appsync-api.us-west-2.amazonaws.com/graphql",
                "aws_region": "us-west-2",
                "api_key": "da2-key",
                "default_authorization_type": "AMAZON_COGNITO_USER_POOLS"
            },
            "storage": {
                "aws_region": "us-west-2",
                "bucket_name": "amplify-bucket-xyz"
            }
        }"#
    }

    #[test]
    fn parses_full_outputs() {
        let parsed = parse(full_json()).unwrap();
        assert_eq!(parsed.auth_region, "us-west-2");
        assert_eq!(parsed.user_pool_id, "us-west-2_abc123");
        assert_eq!(
            parsed.data_url,
            "https://example.appsync-api.us-west-2.amazonaws.com/graphql"
        );
        assert_eq!(parsed.storage_bucket, "amplify-bucket-xyz");
        assert_eq!(parsed.data_api_key, "da2-key");
    }

    #[test]
    fn missing_api_key_is_ok() {
        let raw = r#"{
            "auth": {
                "aws_region": "us-west-2",
                "user_pool_id": "p", "user_pool_client_id": "c", "identity_pool_id": "i"
            },
            "data": { "url": "u", "aws_region": "us-west-2" },
            "storage": { "bucket_name": "b", "aws_region": "us-west-2" }
        }"#;
        let parsed = parse(raw).unwrap();
        assert_eq!(parsed.data_api_key, "");
    }

    #[test]
    fn empty_required_field_returns_not_configured() {
        let raw = r#"{
            "auth": { "aws_region": "", "user_pool_id": "p", "user_pool_client_id": "c", "identity_pool_id": "i" },
            "data": { "url": "u", "aws_region": "r" },
            "storage": { "bucket_name": "b", "aws_region": "r" }
        }"#;
        assert!(matches!(parse(raw), Err(ConfigError::NotConfigured)));
    }

    #[test]
    fn malformed_json() {
        assert!(matches!(
            parse("{ not json"),
            Err(ConfigError::Malformed(_))
        ));
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    #[test]
    fn embedded_outputs_are_non_empty_when_sandbox_deployed() {
        // Sanity check: when amplify_outputs.json is present at the
        // repo root (post `npx ampx sandbox`), `load` returns real
        // endpoints. Skipped silently when build.rs fell back to the
        // empty stub — that's the legitimate offline state.
        match load() {
            Ok(o) => {
                assert!(!o.user_pool_id.is_empty());
                assert!(!o.data_url.is_empty());
                eprintln!(
                    "embedded outputs OK: user_pool={}, data_url={}",
                    o.user_pool_id, o.data_url
                );
            }
            Err(ConfigError::NotConfigured) => eprintln!("(stub) skipped — no live sandbox"),
            Err(e) => panic!("unexpected: {e}"),
        }
    }
}
