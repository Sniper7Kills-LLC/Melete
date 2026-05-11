//! Integration tests for `remote_template_store::config::load`.
//!
//! These exercise the runtime override path (`MELETE_AMPLIFY_OUTPUTS`),
//! the embedded fallback, and the two error variants.
//!
//! Gated on the `remote` feature — without it the config module isn't
//! compiled and there's nothing to test.

#![cfg(feature = "remote")]

use std::io::Write;

use melete_storage::remote_template_store::config::{load, ConfigError};
use tempfile::NamedTempFile;

/// `cargo test` runs all integration tests in the same process; touching
/// a process-wide env var from one test races every other test that
/// reads the same var. Serialise via a global mutex so each test sees a
/// well-defined view of `MELETE_AMPLIFY_OUTPUTS`.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const FULL_OUTPUTS: &str = r#"{
    "version": "1",
    "auth": {
        "aws_region": "eu-west-1",
        "user_pool_id": "eu-west-1_pool",
        "user_pool_client_id": "client",
        "identity_pool_id": "eu-west-1:identity"
    },
    "data": {
        "url": "https://example.appsync-api.eu-west-1.amazonaws.com/graphql",
        "aws_region": "eu-west-1",
        "api_key": "da2-test",
        "default_authorization_type": "AMAZON_COGNITO_USER_POOLS"
    },
    "storage": {
        "aws_region": "eu-west-1",
        "bucket_name": "amplify-test-bucket"
    }
}"#;

const EMPTY_STUB: &str = r#"{
    "version": "1",
    "auth": { "aws_region": "", "user_pool_id": "", "user_pool_client_id": "", "identity_pool_id": "" },
    "data": { "url": "", "aws_region": "", "api_key": "", "default_authorization_type": "AMAZON_COGNITO_USER_POOLS" },
    "storage": { "aws_region": "", "bucket_name": "" }
}"#;

fn write_temp(body: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(body.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn load_via_env_var_override() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = write_temp(FULL_OUTPUTS);
    std::env::set_var("MELETE_AMPLIFY_OUTPUTS", f.path());
    let out = load().expect("env-var override should parse");
    std::env::remove_var("MELETE_AMPLIFY_OUTPUTS");
    assert_eq!(out.auth_region, "eu-west-1");
    assert_eq!(out.user_pool_id, "eu-west-1_pool");
    assert_eq!(out.storage_bucket, "amplify-test-bucket");
}

#[test]
fn load_falls_back_to_embedded_when_env_unset() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("MELETE_AMPLIFY_OUTPUTS");
    // Embedded copy is whatever build.rs found. In the dev sandbox
    // (no amplify_outputs.json) it's the empty stub → NotConfigured.
    // In CI w/ a sandbox up, it parses successfully. Either result is
    // acceptable — what we're asserting is that the call doesn't panic
    // (i.e. include_str! resolved a real file at build time) and that
    // a malformed-JSON error is impossible.
    match load() {
        Ok(_) => {}
        Err(ConfigError::NotConfigured) => {}
        Err(e @ ConfigError::Malformed(_)) => {
            panic!("embedded copy should never be malformed: {e}");
        }
    }
}

#[test]
fn load_returns_not_configured_when_required_fields_empty() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = write_temp(EMPTY_STUB);
    std::env::set_var("MELETE_AMPLIFY_OUTPUTS", f.path());
    let r = load();
    std::env::remove_var("MELETE_AMPLIFY_OUTPUTS");
    assert!(matches!(r, Err(ConfigError::NotConfigured)), "got {r:?}");
}

#[test]
fn load_returns_malformed_on_invalid_json() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = write_temp("not json {");
    std::env::set_var("MELETE_AMPLIFY_OUTPUTS", f.path());
    let r = load();
    std::env::remove_var("MELETE_AMPLIFY_OUTPUTS");
    assert!(matches!(r, Err(ConfigError::Malformed(_))), "got {r:?}");
}
