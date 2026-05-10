//! Build script: resolve `amplify_outputs.json` and expose its absolute
//! path to the crate via the `AMPLIFY_OUTPUTS_JSON` rustc env var, which
//! `remote_template_store::config` consumes through
//! `include_str!(env!("AMPLIFY_OUTPUTS_JSON"))`.
//!
//! Resolution precedence:
//!   1. `JOURNAL_AMPLIFY_OUTPUTS` env var (CI / explicit override).
//!   2. `<repo_root>/amplify_outputs.json` (sandbox dev — repo root
//!      walked up from `CARGO_MANIFEST_DIR` until we see `Cargo.lock`).
//!
//! If neither is found AND the `remote` feature is on, we emit a
//! `cargo:warning=` and write a stub file into `OUT_DIR` so the build
//! still succeeds. Runtime config probe will see empty endpoints and
//! report "backend not configured".
//!
//! When the `remote` feature is OFF this script does effectively
//! nothing — the config module isn't compiled, so the rustc env var is
//! never consumed.

use std::path::{Path, PathBuf};

const STUB: &str = r#"{
  "version": "1",
  "auth": { "aws_region": "", "user_pool_id": "", "user_pool_client_id": "", "identity_pool_id": "" },
  "data": { "url": "", "aws_region": "", "api_key": "", "default_authorization_type": "AMAZON_COGNITO_USER_POOLS" },
  "storage": { "aws_region": "", "bucket_name": "" }
}
"#;

fn main() {
    // Re-run when the override env var changes or when a sibling
    // `amplify_outputs.json` appears / disappears at the repo root.
    println!("cargo:rerun-if-env-changed=JOURNAL_AMPLIFY_OUTPUTS");

    if !cfg!(feature = "remote") {
        // Without the `remote` feature the config module isn't built,
        // so we don't need to expose the path. Bail early.
        return;
    }

    if let Some(path) = resolve_source() {
        println!("cargo:rerun-if-changed={}", path.display());
        println!("cargo:rustc-env=AMPLIFY_OUTPUTS_JSON={}", path.display());
        return;
    }

    // Stub fallback: emit an empty-shape file inside OUT_DIR so
    // `include_str!` has something to read. The runtime config loader
    // detects the empty fields and returns `ConfigError::NotConfigured`.
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR set by cargo"));
    let stub_path = out_dir.join("amplify_outputs.json");
    std::fs::write(&stub_path, STUB).expect("write stub amplify_outputs.json into OUT_DIR");
    println!(
        "cargo:warning=amplify_outputs.json not found — built with empty stub. \
         Run `npx ampx sandbox` and rebuild to embed real endpoints, or set \
         JOURNAL_AMPLIFY_OUTPUTS=<path> for an explicit override."
    );
    println!(
        "cargo:rustc-env=AMPLIFY_OUTPUTS_JSON={}",
        stub_path.display()
    );
}

fn resolve_source() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("JOURNAL_AMPLIFY_OUTPUTS") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    let repo_root = walk_up_to_repo_root(&manifest_dir)?;
    let candidate = repo_root.join("amplify_outputs.json");
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Walk up from `start` looking for the workspace `Cargo.lock`. The
/// directory containing it is the repo root.
fn walk_up_to_repo_root(start: &Path) -> Option<PathBuf> {
    let mut cur: &Path = start;
    loop {
        if cur.join("Cargo.lock").is_file() {
            return Some(cur.to_path_buf());
        }
        cur = cur.parent()?;
    }
}
