//! Hand-rolled AppSync GraphQL client. Cognito User-Pool auth mode
//! is the only path supported (matches `amplify_outputs.json`'s
//! `default_authorization_type: AMAZON_COGNITO_USER_POOLS`).
//!
//! Decision (Phase 6.3 #12): no `cynic` / no codegen — every query
//! is just a string constant in the call site. Saves a few minutes
//! of compile time and a few hundred KB of generated trait surface
//! for an app that ships ~6 GraphQL operations total.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphQlError {
    #[error("HTTP transport: {0}")]
    Transport(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("response was not valid JSON: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("GraphQL errors: {0}")]
    Service(String),
    #[error("response missing `data` field")]
    NoData,
}

/// Marshalled request body. Public so callers can construct the
/// payload offline (e.g. for test fixtures or for queueing while
/// offline) and ship it later.
#[derive(Debug, Serialize)]
pub struct Request<'a> {
    pub query: &'a str,
    #[serde(rename = "operationName", skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<&'a str>,
    pub variables: Value,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    errors: Option<Value>,
}

/// Parse an AppSync response envelope into the inner `data` value.
/// `errors` (when present) is rendered as a string and wrapped in
/// [`GraphQlError::Service`] regardless of HTTP status — AppSync
/// often returns 200 with an `errors` array.
pub fn parse_response(body: &str) -> Result<Value, GraphQlError> {
    let env: Envelope = serde_json::from_str(body)?;
    if let Some(errors) = env.errors {
        if !errors.as_array().map(|a| a.is_empty()).unwrap_or(true) {
            return Err(GraphQlError::Service(errors.to_string()));
        }
    }
    env.data.ok_or(GraphQlError::NoData)
}

/// Build the canonical AppSync POST body. Pure function so the wire
/// shape is testable without a network.
pub fn body(query: &str, operation_name: Option<&str>, variables: Value) -> String {
    let req = Request {
        query,
        operation_name,
        variables,
    };
    serde_json::to_string(&req).expect("serialize gql request")
}

#[cfg(feature = "remote")]
fn shared_client() -> &'static reqwest::blocking::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // Connection pool + HTTP/2 keep-alive cuts the per-request TLS
        // handshake cost from ~150 ms to ~0. Without this, every
        // mutation rebuilds the TLS connection — visible as 1–2 s
        // per-stroke latency during eraser bursts.
        reqwest::blocking::Client::builder()
            .pool_max_idle_per_host(16)
            .pool_idle_timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("build shared graphql client")
    })
}

#[cfg(feature = "remote")]
pub fn post(
    data_url: &str,
    id_token: &str,
    query: &str,
    operation_name: Option<&str>,
    variables: Value,
) -> Result<Value, GraphQlError> {
    let payload = body(query, operation_name, variables);
    let started = std::time::Instant::now();
    let op = operation_name
        .or_else(|| query.split_whitespace().nth(1))
        .unwrap_or("?");
    tracing::trace!("graphql post {} ({} bytes)", op, payload.len());
    let resp = shared_client()
        .post(data_url)
        .header("Content-Type", "application/json")
        .header("Authorization", id_token)
        .body(payload)
        .send()
        .map_err(|e| {
            tracing::warn!("graphql {} transport: {}", op, e);
            GraphQlError::Transport(e.to_string())
        })?;
    let status = resp.status().as_u16();
    let text = resp
        .text()
        .map_err(|e| GraphQlError::Transport(e.to_string()))?;
    let elapsed_ms = started.elapsed().as_millis();
    tracing::debug!("graphql {} -> {} in {} ms", op, status, elapsed_ms);
    if status >= 500 {
        return Err(GraphQlError::Http { status, body: text });
    }
    // 4xx may still carry a useful `errors` payload — try to parse
    // first, fall back to an Http error if the body wasn't JSON.
    match parse_response(&text) {
        Ok(v) => Ok(v),
        Err(GraphQlError::Decode(_)) if status >= 400 => {
            Err(GraphQlError::Http { status, body: text })
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn body_serializes_with_operation_name() {
        let b = body("query Foo { foo }", Some("Foo"), json!({ "id": "abc" }));
        let v: Value = serde_json::from_str(&b).unwrap();
        assert_eq!(v["query"], "query Foo { foo }");
        assert_eq!(v["operationName"], "Foo");
        assert_eq!(v["variables"]["id"], "abc");
    }

    #[test]
    fn body_omits_operation_name_when_absent() {
        let b = body("query { foo }", None, json!({}));
        let v: Value = serde_json::from_str(&b).unwrap();
        assert!(v.get("operationName").is_none());
    }

    #[test]
    fn parse_response_unwraps_data() {
        let raw = r#"{"data":{"foo":42}}"#;
        let v = parse_response(raw).unwrap();
        assert_eq!(v["foo"], 42);
    }

    #[test]
    fn parse_response_surfaces_errors_array() {
        let raw = r#"{"errors":[{"message":"unauthorized"}]}"#;
        match parse_response(raw).unwrap_err() {
            GraphQlError::Service(msg) => assert!(msg.contains("unauthorized")),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_response_no_data_no_errors_is_no_data() {
        let raw = r#"{}"#;
        assert!(matches!(
            parse_response(raw).unwrap_err(),
            GraphQlError::NoData
        ));
    }

    #[test]
    fn parse_response_empty_errors_array_treats_as_success() {
        let raw = r#"{"data":{"x":1},"errors":[]}"#;
        let v = parse_response(raw).unwrap();
        assert_eq!(v["x"], 1);
    }
}
