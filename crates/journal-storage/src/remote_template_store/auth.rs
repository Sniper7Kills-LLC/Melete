//! Cognito User Pool sign-in + refresh + token persistence.
//!
//! Cognito InitiateAuth + RefreshAuth are unsigned `POST`s with a
//! JSON body and an `X-Amz-Target` header — no AWS SDK needed.
//! Talking to the service over raw `reqwest` keeps the dependency
//! footprint small and avoids dragging in a tokio runtime for what
//! is fundamentally a 1-shot HTTPS call.
//!
//! Token storage uses the OS keyring (Secret Service via dbus on
//! Linux) by default and falls back to `~/.config/journal/auth.toml`
//! mode `0600` when the keyring is unavailable (e.g. headless box,
//! locked session at boot). Both backends are tried on `load_tokens`
//! so a user who once saved to the file path doesn't get signed
//! out after enabling a keyring.
//!
//! The caller decides when to refresh: [`Tokens::needs_refresh`]
//! returns `true` once `access_expires_at` is within a 60s buffer of
//! `now`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("HTTP transport: {0}")]
    Transport(String),
    #[error("auth response was not valid JSON: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("Cognito returned {status}: {message}")]
    Service { status: u16, message: String },
    #[error("auth response missing field: {0}")]
    MissingField(&'static str),
    #[error("config dir unavailable")]
    NoConfigDir,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("toml-ser: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("keyring: {0}")]
    Keyring(String),
}

/// Persisted token bundle. `*_token` are opaque JWT strings; we
/// don't decode the body. `access_expires_at` is the wall-clock
/// epoch second when the access token stops being valid (server
/// `expires_in` + the time we received the response). Refresh
/// tokens themselves don't carry a server-side expiry exposed in
/// `InitiateAuth` — we treat them as long-lived and let the next
/// service-side rejection drive a re-login.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tokens {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: u64,
}

impl Tokens {
    /// `true` when `access_expires_at` is within `buffer_secs` of
    /// `now_secs`. The default callsite uses `60` so a refresh
    /// fires before the next request rather than after.
    pub fn needs_refresh(&self, now_secs: u64, buffer_secs: u64) -> bool {
        self.access_expires_at <= now_secs.saturating_add(buffer_secs)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Resolve `~/.config/journal/auth.toml` (XDG-aware via `dirs`).
/// Public so the file-backed fallback path is testable.
pub fn token_path() -> Result<PathBuf, AuthError> {
    let base = dirs::config_dir().ok_or(AuthError::NoConfigDir)?;
    Ok(base.join("journal").join("auth.toml"))
}

const KEYRING_SERVICE: &str = "journal-app";
const KEYRING_USER: &str = "tokens";

/// Load tokens with keyring-first / file-fallback semantics. Order:
///
///   1. Keyring `journal-app/tokens` — when the entry is present
///      and decodable, return it.
///   2. File `~/.config/journal/auth.toml` — covers the legacy
///      pre-keyring layout and headless / no-dbus environments.
///   3. `Ok(None)` — neither backend has tokens; user is signed out.
///
/// Keyring errors are downgraded to a tracing warning + fall-through
/// to the file backend, so a misconfigured Secret Service doesn't
/// strand the user.
pub fn load_tokens() -> Result<Option<Tokens>, AuthError> {
    match keyring_load() {
        Ok(Some(t)) => return Ok(Some(t)),
        Ok(None) => {}
        Err(e) => tracing::warn!("keyring load failed, trying file fallback: {e}"),
    }
    file_load()
}

/// Save tokens to the keyring. If the keyring isn't available
/// (no dbus, locked session at boot, …) write the toml fallback
/// instead. The toml fallback is also chmodded `0600` on Unix.
pub fn save_tokens(t: &Tokens) -> Result<(), AuthError> {
    match keyring_save(t) {
        Ok(()) => {
            // If a stale file copy exists from a previous version,
            // clear it so the keyring is the only authority.
            let _ = file_clear();
            Ok(())
        }
        Err(e) => {
            tracing::warn!("keyring save failed, falling back to file: {e}");
            file_save(t)
        }
    }
}

/// Best-effort delete from both backends. Errors are logged, not
/// returned, since a sign-out where one backend was already empty
/// shouldn't surface as a user-visible error.
pub fn clear_tokens() -> Result<(), AuthError> {
    if let Err(e) = keyring_clear() {
        tracing::warn!("keyring clear failed: {e}");
    }
    if let Err(e) = file_clear() {
        tracing::warn!("file clear failed: {e}");
    }
    Ok(())
}

// ── file backend ────────────────────────────────────────────────────

/// Path-based file save — the unit-testable seam.
pub(crate) fn save_to_path(path: &Path, t: &Tokens) -> Result<(), AuthError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string(t)?;
    fs::write(path, raw)?;
    set_mode_0600(path)?;
    Ok(())
}

pub(crate) fn load_from_path(path: &Path) -> Result<Option<Tokens>, AuthError> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let t: Tokens = toml::from_str(&raw)?;
    Ok(Some(t))
}

pub(crate) fn clear_path(path: &Path) -> Result<(), AuthError> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn file_save(t: &Tokens) -> Result<(), AuthError> {
    save_to_path(&token_path()?, t)
}

fn file_load() -> Result<Option<Tokens>, AuthError> {
    load_from_path(&token_path()?)
}

fn file_clear() -> Result<(), AuthError> {
    clear_path(&token_path()?)
}

// ── keyring backend ─────────────────────────────────────────────────

fn keyring_entry() -> Result<keyring::Entry, AuthError> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| AuthError::Keyring(e.to_string()))
}

fn keyring_save(t: &Tokens) -> Result<(), AuthError> {
    let entry = keyring_entry()?;
    let raw = toml::to_string(t)?;
    entry
        .set_password(&raw)
        .map_err(|e| AuthError::Keyring(e.to_string()))?;
    Ok(())
}

fn keyring_load() -> Result<Option<Tokens>, AuthError> {
    let entry = keyring_entry()?;
    match entry.get_password() {
        Ok(raw) => {
            let t: Tokens = toml::from_str(&raw)?;
            Ok(Some(t))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AuthError::Keyring(e.to_string())),
    }
}

fn keyring_clear() -> Result<(), AuthError> {
    let entry = keyring_entry()?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::Keyring(e.to_string())),
    }
}


#[cfg(unix)]
fn set_mode_0600(path: &std::path::Path) -> Result<(), AuthError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(path)?.permissions();
    perm.set_mode(0o600);
    fs::set_permissions(path, perm)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode_0600(_path: &std::path::Path) -> Result<(), AuthError> {
    Ok(())
}

/// Cognito IdP endpoint host for `region` (`cognito-idp.<region>.amazonaws.com`).
pub fn cognito_idp_host(region: &str) -> String {
    format!("cognito-idp.{}.amazonaws.com", region)
}

/// Build the request body for an `InitiateAuth` call with the
/// `USER_PASSWORD_AUTH` flow. `client_id` is the User Pool's app
/// client id from `amplify_outputs.json`. Intentionally a pure
/// function so unit tests can pin the JSON shape without touching
/// the network.
pub fn user_password_auth_body(client_id: &str, username: &str, password: &str) -> String {
    let v = serde_json::json!({
        "AuthFlow": "USER_PASSWORD_AUTH",
        "ClientId": client_id,
        "AuthParameters": {
            "USERNAME": username,
            "PASSWORD": password,
        },
    });
    v.to_string()
}

pub fn refresh_token_auth_body(client_id: &str, refresh_token: &str) -> String {
    let v = serde_json::json!({
        "AuthFlow": "REFRESH_TOKEN_AUTH",
        "ClientId": client_id,
        "AuthParameters": {
            "REFRESH_TOKEN": refresh_token,
        },
    });
    v.to_string()
}

/// Parse a Cognito `InitiateAuth` JSON response into a [`Tokens`].
/// `received_at_secs` is the wall-clock epoch second when the
/// response was received; the function adds `expires_in` to it for
/// the `access_expires_at` field. `existing_refresh` is used as a
/// fallback when the response omits `RefreshToken` (refresh-flow
/// responses do not include one).
pub fn parse_initiate_auth_response(
    raw: &str,
    received_at_secs: u64,
    existing_refresh: Option<&str>,
) -> Result<Tokens, AuthError> {
    let v: serde_json::Value = serde_json::from_str(raw)?;
    let result = v
        .get("AuthenticationResult")
        .ok_or(AuthError::MissingField("AuthenticationResult"))?;
    let id_token = result
        .get("IdToken")
        .and_then(|x| x.as_str())
        .ok_or(AuthError::MissingField("IdToken"))?
        .to_string();
    let access_token = result
        .get("AccessToken")
        .and_then(|x| x.as_str())
        .ok_or(AuthError::MissingField("AccessToken"))?
        .to_string();
    let refresh_token = match result.get("RefreshToken").and_then(|x| x.as_str()) {
        Some(s) => s.to_string(),
        None => existing_refresh
            .ok_or(AuthError::MissingField("RefreshToken"))?
            .to_string(),
    };
    let expires_in = result
        .get("ExpiresIn")
        .and_then(|x| x.as_u64())
        .unwrap_or(3600);
    Ok(Tokens {
        id_token,
        access_token,
        refresh_token,
        access_expires_at: received_at_secs.saturating_add(expires_in),
    })
}

#[cfg(feature = "remote")]
mod net {
    use super::*;

    fn idp_url(region: &str) -> String {
        format!("https://{}/", cognito_idp_host(region))
    }

    fn post_cognito(
        region: &str,
        target: &str,
        body: String,
    ) -> Result<(u16, String), AuthError> {
        let client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|e| AuthError::Transport(e.to_string()))?;
        let resp = client
            .post(idp_url(region))
            .header("X-Amz-Target", target)
            .header("Content-Type", "application/x-amz-json-1.1")
            .body(body)
            .send()
            .map_err(|e| AuthError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let text = resp
            .text()
            .map_err(|e| AuthError::Transport(e.to_string()))?;
        Ok((status, text))
    }

    /// Sign in with email + password. On success persists the
    /// returned tokens to `~/.config/journal/auth.toml` and
    /// returns the bundle.
    pub fn sign_in(
        region: &str,
        client_id: &str,
        username: &str,
        password: &str,
    ) -> Result<Tokens, AuthError> {
        let body = user_password_auth_body(client_id, username, password);
        let (status, text) = post_cognito(
            region,
            "AWSCognitoIdentityProviderService.InitiateAuth",
            body,
        )?;
        if status >= 400 {
            return Err(AuthError::Service {
                status,
                message: text,
            });
        }
        let tokens = parse_initiate_auth_response(&text, now_secs(), None)?;
        save_tokens(&tokens)?;
        Ok(tokens)
    }

    /// Refresh the access + id tokens using the current refresh
    /// token. Re-persists the bundle on success. Caller invokes
    /// when `Tokens::needs_refresh` is `true`.
    pub fn refresh(region: &str, client_id: &str, current: &Tokens) -> Result<Tokens, AuthError> {
        let body = refresh_token_auth_body(client_id, &current.refresh_token);
        let (status, text) = post_cognito(
            region,
            "AWSCognitoIdentityProviderService.InitiateAuth",
            body,
        )?;
        if status >= 400 {
            return Err(AuthError::Service {
                status,
                message: text,
            });
        }
        let tokens =
            parse_initiate_auth_response(&text, now_secs(), Some(&current.refresh_token))?;
        save_tokens(&tokens)?;
        Ok(tokens)
    }
}

#[cfg(feature = "remote")]
pub use net::{refresh, sign_in};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_password_auth_body_shape() {
        let body = user_password_auth_body("clientid", "alice@example.com", "hunter2");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["AuthFlow"], "USER_PASSWORD_AUTH");
        assert_eq!(v["ClientId"], "clientid");
        assert_eq!(v["AuthParameters"]["USERNAME"], "alice@example.com");
        assert_eq!(v["AuthParameters"]["PASSWORD"], "hunter2");
    }

    #[test]
    fn refresh_token_auth_body_shape() {
        let body = refresh_token_auth_body("clientid", "rt");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["AuthFlow"], "REFRESH_TOKEN_AUTH");
        assert_eq!(v["AuthParameters"]["REFRESH_TOKEN"], "rt");
    }

    #[test]
    fn parses_full_initiate_auth_response() {
        let raw = r#"{
            "AuthenticationResult": {
                "IdToken": "id.jwt.value",
                "AccessToken": "access.jwt.value",
                "RefreshToken": "refresh.jwt.value",
                "ExpiresIn": 3600,
                "TokenType": "Bearer"
            }
        }"#;
        let t = parse_initiate_auth_response(raw, 1_700_000_000, None).unwrap();
        assert_eq!(t.id_token, "id.jwt.value");
        assert_eq!(t.access_token, "access.jwt.value");
        assert_eq!(t.refresh_token, "refresh.jwt.value");
        assert_eq!(t.access_expires_at, 1_700_000_000 + 3600);
    }

    #[test]
    fn refresh_response_falls_back_to_existing_refresh_token() {
        // Real Cognito refresh-flow responses omit RefreshToken; we
        // must reuse the one the caller already had.
        let raw = r#"{
            "AuthenticationResult": {
                "IdToken": "new.id",
                "AccessToken": "new.access",
                "ExpiresIn": 3600
            }
        }"#;
        let t = parse_initiate_auth_response(raw, 100, Some("kept.refresh")).unwrap();
        assert_eq!(t.refresh_token, "kept.refresh");
    }

    #[test]
    fn refresh_response_without_existing_token_errors() {
        let raw = r#"{ "AuthenticationResult": { "IdToken": "i", "AccessToken": "a" } }"#;
        let err = parse_initiate_auth_response(raw, 0, None).unwrap_err();
        match err {
            AuthError::MissingField("RefreshToken") => {}
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn missing_authentication_result_errors() {
        let raw = r#"{ "ChallengeName": "NEW_PASSWORD_REQUIRED" }"#;
        match parse_initiate_auth_response(raw, 0, None).unwrap_err() {
            AuthError::MissingField("AuthenticationResult") => {}
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn needs_refresh_buffer() {
        let t = Tokens {
            id_token: String::new(),
            access_token: String::new(),
            refresh_token: String::new(),
            access_expires_at: 1000,
        };
        assert!(!t.needs_refresh(800, 60));
        assert!(t.needs_refresh(950, 60)); // within 60s buffer
        assert!(t.needs_refresh(1500, 60));
    }

    #[test]
    fn cognito_idp_host_format() {
        assert_eq!(cognito_idp_host("us-west-2"), "cognito-idp.us-west-2.amazonaws.com");
    }

    fn fixture_tokens() -> Tokens {
        Tokens {
            id_token: "id".into(),
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            access_expires_at: 1_700_000_000,
        }
    }

    #[test]
    fn file_round_trip_writes_then_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.toml");
        let t = fixture_tokens();
        save_to_path(&path, &t).unwrap();
        let loaded = load_from_path(&path).unwrap().expect("tokens present");
        assert_eq!(loaded, t);
    }

    #[test]
    fn file_load_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.toml");
        assert!(load_from_path(&path).unwrap().is_none());
    }

    #[test]
    fn file_clear_removes_file_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.toml");
        save_to_path(&path, &fixture_tokens()).unwrap();
        clear_path(&path).unwrap();
        assert!(!path.exists());
        // Second clear is a no-op.
        clear_path(&path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn file_save_chmod_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.toml");
        save_to_path(&path, &fixture_tokens()).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected mode 0600, got {:o}", mode);
    }
}
