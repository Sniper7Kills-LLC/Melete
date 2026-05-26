//! Tiny JWT payload decoder. We only need claim extraction (not
//! signature verification) because Cognito already validated the
//! token when it issued + every refreshed; the desktop trusts its
//! own token bundle.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde_json::Value;

use super::store::RemoteError;

fn decode_payload(token: &str) -> Result<Value, RemoteError> {
    let mut parts = token.split('.');
    let _header = parts
        .next()
        .ok_or_else(|| RemoteError::Malformed("jwt: no header".into()))?;
    let payload_b64 = parts
        .next()
        .ok_or_else(|| RemoteError::Malformed("jwt: no payload".into()))?;
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| RemoteError::Malformed(format!("jwt b64: {e}")))?;
    serde_json::from_slice(&payload_bytes)
        .map_err(|e| RemoteError::Malformed(format!("jwt json: {e}")))
}

/// Decode the JWT's payload and extract the `sub` claim. Returns
/// `RemoteError::Malformed` if the token is not a 3-part JWT, if the
/// payload isn't valid base64url, or if `sub` is missing.
pub fn decode_sub(token: &str) -> Result<String, RemoteError> {
    let json = decode_payload(token)?;
    json.get("sub")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| RemoteError::Malformed("jwt: missing sub".into()))
}

/// Decode the JWT's `cognito:groups` claim. Returns an empty vec
/// if the claim isn't present (user belongs to no Cognito group).
pub fn decode_groups(token: &str) -> Result<Vec<String>, RemoteError> {
    let json = decode_payload(token)?;
    Ok(json
        .get("cognito:groups")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    fn encode_jwt(payload: &str) -> String {
        let h = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}");
        let p = URL_SAFE_NO_PAD.encode(payload);
        format!("{h}.{p}.sig")
    }

    #[test]
    fn decodes_sub_claim() {
        let jwt = encode_jwt(r#"{"sub":"abc-123","exp":1}"#);
        assert_eq!(decode_sub(&jwt).unwrap(), "abc-123");
    }

    #[test]
    fn rejects_missing_sub() {
        let jwt = encode_jwt(r#"{"exp":1}"#);
        assert!(decode_sub(&jwt).is_err());
    }

    #[test]
    fn rejects_malformed_token() {
        assert!(decode_sub("notajwt").is_err());
    }
}
