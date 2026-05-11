//! Pluggable asset lookup for templates that reference image / PDF
//! bytes by short name (`asset:foo.png`).
//!
//! In Phase 6.3 the template body TOML stops carrying filesystem paths
//! and instead embeds `asset:<name>` URIs that resolve against an
//! `AssetResolver`. The desktop app will hand the renderer a resolver
//! that pulls bytes from `page_template_assets` in the local SQLite
//! catalog; a future web viewer can plug in a resolver backed by S3 or
//! an in-memory bundle.
//!
//! The trait lives in `journal-core` (web-importable, no GTK / SQLite /
//! poppler in its dependency closure) so both the desktop renderer and
//! a future WASM viewer can share the same call site.

use std::sync::Arc;

/// Resolves a template-asset name (e.g. `"foo.png"`) to its raw bytes.
///
/// The contract is intentionally narrow: implementations are pure
/// lookups; they do not mutate state, perform network IO, or surface
/// errors. A failed resolve returns `None` and the caller decides what
/// to do (typically draw a placeholder).
///
/// Not `Send + Sync`: the desktop holds its backend in
/// `Rc<RefCell<dyn NotebookBackend>>` (single-threaded GTK main loop)
/// and the web viewer is single-threaded wasm. A future caller that
/// needs cross-thread sharing can introduce a `SyncAssetResolver`
/// marker without disturbing existing impls.
pub trait AssetResolver {
    /// Look up the bytes for `name`. Returns `None` if no asset by
    /// that name is registered, or if the asset is registered but
    /// its bytes are unavailable in this resolver (e.g. a remote
    /// resolver that hasn't fetched the blob yet).
    fn resolve(&self, name: &str) -> Option<Arc<[u8]>>;
}

impl<F> AssetResolver for F
where
    F: Fn(&str) -> Option<Arc<[u8]>>,
{
    fn resolve(&self, name: &str) -> Option<Arc<[u8]>> {
        self(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closure_impl_resolves() {
        let bytes: Arc<[u8]> = Arc::from(vec![1, 2, 3].into_boxed_slice());
        let bytes_clone = bytes.clone();
        let resolver = move |name: &str| {
            if name == "foo.png" {
                Some(bytes_clone.clone())
            } else {
                None
            }
        };
        assert_eq!(resolver.resolve("foo.png").as_deref(), Some(&[1, 2, 3][..]));
        assert!(resolver.resolve("missing").is_none());
    }

    #[test]
    fn dyn_dispatch_compiles() {
        let r: Box<dyn AssetResolver> = Box::new(|_: &str| None);
        assert!(r.resolve("anything").is_none());
    }
}
