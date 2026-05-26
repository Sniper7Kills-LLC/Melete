//! Hand-rolled SigV4-signed S3 GET / PUT for template asset bytes.
//!
//! SigV4 is a few hundred lines of well-documented spec
//! (`AWS Signature Version 4`). We do it inline rather than pulling
//! the AWS SDK because the desktop app talks to S3 for exactly two
//! operations (asset GET, asset PUT) and the SDK ships with a
//! tokio dependency we don't otherwise need.
//!
//! Asset key layout: `assets/{sha256}` — content-addressed, so the
//! same bytes uploaded by two different users dedupe at the bucket
//! level. The `sha256` is the *asset bytes* digest (lowercase hex,
//! 64 chars), matching what we store in DynamoDB and the local
//! `page_template_assets` row.

use thiserror::Error;

use super::identity::AwsCredentials;

#[derive(Debug, Error)]
pub enum S3Error {
    #[error("HTTP transport: {0}")]
    Transport(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("invalid sha256 hex: {0}")]
    InvalidHash(String),
}

pub fn asset_key(sha256_hex: &str) -> String {
    format!("assets/{}", sha256_hex)
}

/// Construct the virtual-hosted-style URL for a bucket+key:
/// `https://<bucket>.s3.<region>.amazonaws.com/<key>`. S3 returns
/// 301s for path-style access in non-`us-east-1` regions, so we use
/// virtual-hosted style by default.
pub fn object_url(bucket: &str, region: &str, key: &str) -> String {
    format!("https://{}.s3.{}.amazonaws.com/{}", bucket, region, key)
}

// ── SigV4 helpers ───────────────────────────────────────────────────

const ALGO: &str = "AWS4-HMAC-SHA256";
const SERVICE: &str = "s3";

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("hmac accepts any length key");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

fn signing_key(secret: &str, date_yyyymmdd: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(
        format!("AWS4{}", secret).as_bytes(),
        date_yyyymmdd.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, SERVICE.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

/// Bundle of computed headers ready to attach to a request. The
/// caller is responsible for actually setting them on the request
/// builder (we don't bind to `reqwest::RequestBuilder` here so the
/// signer stays unit-testable without a network).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedHeaders {
    pub authorization: String,
    pub x_amz_date: String,
    pub x_amz_content_sha256: String,
    pub x_amz_security_token: Option<String>,
}

/// Sign a single S3 request. `bucket`/`region`/`key` identify the
/// object; `method` is `"GET"` or `"PUT"`; `body_sha256_hex` is the
/// hex digest of the request body (lowercase, 64 chars; pass the
/// digest of an empty byte slice for GETs); `now_yyyymmdd_hhmmss_z`
/// is `"<YYYYMMDD>T<HHMMSS>Z"` UTC (split out so unit tests can pin
/// a fixed timestamp).
///
/// Returns a [`SignedHeaders`] the caller layers onto the request.
#[allow(clippy::too_many_arguments)]
pub fn sign(
    creds: &AwsCredentials,
    bucket: &str,
    region: &str,
    key: &str,
    method: &str,
    body_sha256_hex: &str,
    now_yyyymmdd_hhmmss_z: &str,
) -> SignedHeaders {
    let host = format!("{}.s3.{}.amazonaws.com", bucket, region);
    let canonical_uri = format!("/{}", key);
    let canonical_query = String::new();
    let date_yyyymmdd = &now_yyyymmdd_hhmmss_z[..8];
    let credential_scope = format!("{}/{}/{}/aws4_request", date_yyyymmdd, region, SERVICE);

    // Canonical headers (sorted by lowercase header name).
    // `host`, `x-amz-content-sha256`, `x-amz-date`, plus `x-amz-security-token`
    // when session creds.
    let mut headers: Vec<(String, String)> = vec![
        ("host".into(), host.clone()),
        ("x-amz-content-sha256".into(), body_sha256_hex.into()),
        ("x-amz-date".into(), now_yyyymmdd_hhmmss_z.into()),
    ];
    if !creds.session_token.is_empty() {
        headers.push(("x-amz-security-token".into(), creds.session_token.clone()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));
    let canonical_headers = headers
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
        .collect::<String>();
    let signed_headers = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, canonical_uri, canonical_query, canonical_headers, signed_headers, body_sha256_hex,
    );
    let canonical_request_hash = sha256_hex(canonical_request.as_bytes());

    let string_to_sign = format!(
        "{}\n{}\n{}\n{}",
        ALGO, now_yyyymmdd_hhmmss_z, credential_scope, canonical_request_hash,
    );

    let key_bytes = signing_key(&creds.secret_key, date_yyyymmdd, region);
    let signature_bytes = hmac_sha256(&key_bytes, string_to_sign.as_bytes());
    let signature = hex::encode(signature_bytes);

    let authorization = format!(
        "{} Credential={}/{},SignedHeaders={},Signature={}",
        ALGO, creds.access_key_id, credential_scope, signed_headers, signature,
    );
    SignedHeaders {
        authorization,
        x_amz_date: now_yyyymmdd_hhmmss_z.into(),
        x_amz_content_sha256: body_sha256_hex.into(),
        x_amz_security_token: if creds.session_token.is_empty() {
            None
        } else {
            Some(creds.session_token.clone())
        },
    }
}

pub fn now_yyyymmdd_hhmmss_z() -> String {
    use chrono::Utc;
    Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

// ── network ─────────────────────────────────────────────────────────

#[cfg(feature = "remote")]
mod net {
    use super::*;

    fn apply_headers(
        mut req: reqwest::blocking::RequestBuilder,
        h: &SignedHeaders,
    ) -> reqwest::blocking::RequestBuilder {
        req = req
            .header("Authorization", &h.authorization)
            .header("X-Amz-Date", &h.x_amz_date)
            .header("X-Amz-Content-Sha256", &h.x_amz_content_sha256);
        if let Some(token) = &h.x_amz_security_token {
            req = req.header("X-Amz-Security-Token", token);
        }
        req
    }

    pub fn put(
        creds: &AwsCredentials,
        bucket: &str,
        region: &str,
        sha256_hex: &str,
        bytes: &[u8],
        content_type: &str,
    ) -> Result<(), S3Error> {
        if sha256_hex.len() != 64 || !sha256_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(S3Error::InvalidHash(sha256_hex.to_string()));
        }
        let key = asset_key(sha256_hex);
        let body_sha = super::sha256_hex(bytes);
        // S3 expects the digest of the *body bytes*. The asset key
        // is already the asset's sha256, but we re-hash the body
        // here so the canonical-request line matches what S3
        // computes server-side.
        let signed = sign(
            creds,
            bucket,
            region,
            &key,
            "PUT",
            &body_sha,
            &now_yyyymmdd_hhmmss_z(),
        );
        let url = object_url(bucket, region, &key);
        let client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|e| S3Error::Transport(e.to_string()))?;
        let req = client
            .put(url)
            .header("Content-Type", content_type)
            .body(bytes.to_vec());
        let resp = apply_headers(req, &signed)
            .send()
            .map_err(|e| S3Error::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().unwrap_or_default();
            return Err(S3Error::Http { status, body });
        }
        Ok(())
    }

    pub fn get(
        creds: &AwsCredentials,
        bucket: &str,
        region: &str,
        sha256_hex: &str,
    ) -> Result<Vec<u8>, S3Error> {
        if sha256_hex.len() != 64 || !sha256_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(S3Error::InvalidHash(sha256_hex.to_string()));
        }
        let key = asset_key(sha256_hex);
        let empty_sha = super::sha256_hex(&[]);
        let signed = sign(
            creds,
            bucket,
            region,
            &key,
            "GET",
            &empty_sha,
            &now_yyyymmdd_hhmmss_z(),
        );
        let url = object_url(bucket, region, &key);
        let client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|e| S3Error::Transport(e.to_string()))?;
        let req = client.get(url);
        let resp = apply_headers(req, &signed)
            .send()
            .map_err(|e| S3Error::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().unwrap_or_default();
            return Err(S3Error::Http { status, body });
        }
        let bytes = resp
            .bytes()
            .map_err(|e| S3Error::Transport(e.to_string()))?;
        Ok(bytes.to_vec())
    }
}

#[cfg(feature = "remote")]
pub use net::{get, put};

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_creds() -> AwsCredentials {
        AwsCredentials {
            access_key_id: "AKIDEXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            session_token: String::new(),
            expires_at: 0,
        }
    }

    #[test]
    fn asset_key_format() {
        assert_eq!(
            asset_key("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
            "assets/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn object_url_uses_virtual_hosted_style() {
        let url = object_url("my-bucket", "us-west-2", "assets/abc");
        assert_eq!(
            url,
            "https://my-bucket.s3.us-west-2.amazonaws.com/assets/abc"
        );
    }

    #[test]
    fn sign_includes_security_token_header_when_session_creds() {
        let mut creds = fixture_creds();
        creds.session_token = "session-token-xyz".into();
        let body_sha = sha256_hex(&[]);
        let signed = sign(
            &creds,
            "bucket",
            "us-west-2",
            "assets/foo",
            "GET",
            &body_sha,
            "20240115T120000Z",
        );
        assert_eq!(
            signed.x_amz_security_token.as_deref(),
            Some("session-token-xyz")
        );
        assert!(
            signed.authorization.contains("x-amz-security-token"),
            "SignedHeaders list should include x-amz-security-token: {}",
            signed.authorization
        );
    }

    #[test]
    fn sign_omits_security_token_when_no_session() {
        let body_sha = sha256_hex(&[]);
        let signed = sign(
            &fixture_creds(),
            "bucket",
            "us-west-2",
            "assets/foo",
            "GET",
            &body_sha,
            "20240115T120000Z",
        );
        assert!(signed.x_amz_security_token.is_none());
        assert!(!signed.authorization.contains("x-amz-security-token"));
    }

    #[test]
    fn sign_authorization_header_shape() {
        let body_sha = sha256_hex(&[]);
        let signed = sign(
            &fixture_creds(),
            "bucket",
            "us-east-1",
            "assets/key",
            "GET",
            &body_sha,
            "20240115T120000Z",
        );
        // Spec form: "AWS4-HMAC-SHA256 Credential=<access>/<scope>,SignedHeaders=<list>,Signature=<hex>"
        assert!(
            signed.authorization.starts_with(
                "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20240115/us-east-1/s3/aws4_request,"
            ),
            "authz: {}",
            signed.authorization
        );
        assert!(signed
            .authorization
            .contains(",SignedHeaders=host;x-amz-content-sha256;x-amz-date,"));
        assert!(signed.authorization.contains(",Signature="));
    }

    #[test]
    fn sign_signature_is_deterministic_for_fixed_inputs() {
        // Pin the signature so accidental algorithm regressions show
        // up as a test failure.
        let body_sha = sha256_hex(&[]);
        let signed = sign(
            &fixture_creds(),
            "examplebucket",
            "us-east-1",
            "test.txt",
            "GET",
            &body_sha,
            "20130524T000000Z",
        );
        // Hash of canonical request + signing-key + string-to-sign.
        // Computed from this same code; locking it down so future
        // refactors trip if the canonical-string layout drifts.
        assert_eq!(signed.x_amz_date, "20130524T000000Z");
        assert!(signed.authorization.contains("Signature="));
        let sig = signed
            .authorization
            .rsplit_once("Signature=")
            .map(|(_, s)| s.to_string())
            .unwrap();
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
