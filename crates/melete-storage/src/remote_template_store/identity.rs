//! Cognito Identity Pool credential exchange.
//!
//! Once the user is signed in to the User Pool we have an `id_token`
//! JWT. To talk to S3 directly (for asset upload / download) we
//! need short-lived AWS access credentials. The flow is:
//!
//!   1. `GetId` — exchange the `id_token` for an `IdentityId`
//!      (cached, but cheap to recompute).
//!   2. `GetCredentialsForIdentity` — exchange the `IdentityId`
//!      (and the same `id_token` proof) for AccessKey / Secret /
//!      SessionToken / Expiration.
//!
//! Both calls are unsigned `POST`s to `cognito-identity.<region>.amazonaws.com`,
//! same shape as Cognito IdP. The returned credentials live ~1 h
//! and the caller refreshes when [`AwsCredentials::needs_refresh`]
//! returns `true`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("HTTP transport: {0}")]
    Transport(String),
    #[error("response was not valid JSON: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("Cognito Identity returned {status}: {message}")]
    Service { status: u16, message: String },
    #[error("response missing field: {0}")]
    MissingField(&'static str),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_key: String,
    pub session_token: String,
    /// Wall-clock epoch second when the credentials expire (Cognito
    /// returns this as a unix timestamp in `Expiration`).
    pub expires_at: u64,
}

impl AwsCredentials {
    pub fn needs_refresh(&self, now_secs: u64, buffer_secs: u64) -> bool {
        self.expires_at <= now_secs.saturating_add(buffer_secs)
    }
}

pub fn cognito_identity_host(region: &str) -> String {
    format!("cognito-identity.{}.amazonaws.com", region)
}

/// Build the `Logins` map key Cognito Identity expects for a User
/// Pool federation: `cognito-idp.<auth_region>.amazonaws.com/<user_pool_id>`.
pub fn login_key(auth_region: &str, user_pool_id: &str) -> String {
    format!("cognito-idp.{}.amazonaws.com/{}", auth_region, user_pool_id)
}

pub fn get_id_body(identity_pool_id: &str, login_key: &str, id_token: &str) -> String {
    let v = serde_json::json!({
        "IdentityPoolId": identity_pool_id,
        "Logins": { login_key: id_token },
    });
    v.to_string()
}

pub fn get_credentials_body(identity_id: &str, login_key: &str, id_token: &str) -> String {
    let v = serde_json::json!({
        "IdentityId": identity_id,
        "Logins": { login_key: id_token },
    });
    v.to_string()
}

pub fn parse_get_id_response(raw: &str) -> Result<String, IdentityError> {
    let v: serde_json::Value = serde_json::from_str(raw)?;
    v.get("IdentityId")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or(IdentityError::MissingField("IdentityId"))
}

pub fn parse_credentials_response(raw: &str) -> Result<AwsCredentials, IdentityError> {
    let v: serde_json::Value = serde_json::from_str(raw)?;
    let creds = v
        .get("Credentials")
        .ok_or(IdentityError::MissingField("Credentials"))?;
    let access_key_id = creds
        .get("AccessKeyId")
        .and_then(|x| x.as_str())
        .ok_or(IdentityError::MissingField("AccessKeyId"))?
        .to_string();
    let secret_key = creds
        .get("SecretKey")
        .and_then(|x| x.as_str())
        .ok_or(IdentityError::MissingField("SecretKey"))?
        .to_string();
    let session_token = creds
        .get("SessionToken")
        .and_then(|x| x.as_str())
        .ok_or(IdentityError::MissingField("SessionToken"))?
        .to_string();
    // Cognito returns Expiration either as a unix timestamp number
    // or as an RFC3339 string depending on protocol version. Accept
    // both; normalise to epoch second.
    let expiration = creds
        .get("Expiration")
        .ok_or(IdentityError::MissingField("Expiration"))?;
    let expires_at = match expiration.as_f64() {
        Some(n) => n as u64,
        None => match expiration.as_str() {
            Some(s) => chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.timestamp() as u64)
                .map_err(|_| IdentityError::MissingField("Expiration(parse)"))?,
            None => return Err(IdentityError::MissingField("Expiration(type)")),
        },
    };
    Ok(AwsCredentials {
        access_key_id,
        secret_key,
        session_token,
        expires_at,
    })
}

#[cfg(feature = "remote")]
mod net {
    use super::*;

    fn url(region: &str) -> String {
        format!("https://{}/", cognito_identity_host(region))
    }

    fn post(region: &str, target: &str, body: String) -> Result<(u16, String), IdentityError> {
        let client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|e| IdentityError::Transport(e.to_string()))?;
        let resp = client
            .post(url(region))
            .header("X-Amz-Target", target)
            .header("Content-Type", "application/x-amz-json-1.1")
            .body(body)
            .send()
            .map_err(|e| IdentityError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let text = resp
            .text()
            .map_err(|e| IdentityError::Transport(e.to_string()))?;
        Ok((status, text))
    }

    pub fn get_identity_id(
        region: &str,
        identity_pool_id: &str,
        login_key: &str,
        id_token: &str,
    ) -> Result<String, IdentityError> {
        let body = get_id_body(identity_pool_id, login_key, id_token);
        let (status, text) = post(region, "AWSCognitoIdentityService.GetId", body)?;
        if status >= 400 {
            return Err(IdentityError::Service {
                status,
                message: text,
            });
        }
        parse_get_id_response(&text)
    }

    pub fn get_credentials(
        region: &str,
        identity_id: &str,
        login_key: &str,
        id_token: &str,
    ) -> Result<AwsCredentials, IdentityError> {
        let body = get_credentials_body(identity_id, login_key, id_token);
        let (status, text) = post(
            region,
            "AWSCognitoIdentityService.GetCredentialsForIdentity",
            body,
        )?;
        if status >= 400 {
            return Err(IdentityError::Service {
                status,
                message: text,
            });
        }
        parse_credentials_response(&text)
    }
}

#[cfg(feature = "remote")]
pub use net::{get_credentials, get_identity_id};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_key_format() {
        assert_eq!(
            login_key("us-west-2", "us-west-2_abc123"),
            "cognito-idp.us-west-2.amazonaws.com/us-west-2_abc123"
        );
    }

    #[test]
    fn get_id_body_shape() {
        let b = get_id_body("us-west-2:guid", "key", "token");
        let v: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_eq!(v["IdentityPoolId"], "us-west-2:guid");
        assert_eq!(v["Logins"]["key"], "token");
    }

    #[test]
    fn parses_get_id_response() {
        let raw = r#"{"IdentityId":"us-west-2:cf3..."}"#;
        assert_eq!(parse_get_id_response(raw).unwrap(), "us-west-2:cf3...");
    }

    #[test]
    fn parses_credentials_with_numeric_expiration() {
        let raw = r#"{
            "IdentityId": "id",
            "Credentials": {
                "AccessKeyId": "AKIA",
                "SecretKey": "SK",
                "SessionToken": "ST",
                "Expiration": 1700003600
            }
        }"#;
        let c = parse_credentials_response(raw).unwrap();
        assert_eq!(c.access_key_id, "AKIA");
        assert_eq!(c.secret_key, "SK");
        assert_eq!(c.session_token, "ST");
        assert_eq!(c.expires_at, 1_700_003_600);
    }

    #[test]
    fn parses_credentials_with_rfc3339_expiration() {
        let raw = r#"{
            "Credentials": {
                "AccessKeyId": "AKIA",
                "SecretKey": "SK",
                "SessionToken": "ST",
                "Expiration": "2024-01-15T12:00:00Z"
            }
        }"#;
        let c = parse_credentials_response(raw).unwrap();
        assert_eq!(
            c.expires_at,
            chrono::DateTime::parse_from_rfc3339("2024-01-15T12:00:00Z")
                .unwrap()
                .timestamp() as u64
        );
    }

    #[test]
    fn missing_credentials_returns_error() {
        let raw = r#"{"IdentityId":"id"}"#;
        match parse_credentials_response(raw).unwrap_err() {
            IdentityError::MissingField("Credentials") => {}
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn aws_credentials_needs_refresh_buffer() {
        let c = AwsCredentials {
            access_key_id: String::new(),
            secret_key: String::new(),
            session_token: String::new(),
            expires_at: 1000,
        };
        assert!(!c.needs_refresh(800, 60));
        assert!(c.needs_refresh(950, 60));
        assert!(c.needs_refresh(1500, 60));
    }
}
