//! Backend-backed `AssetResolver` for page-template image / PDF
//! backgrounds. The renderer asks for `asset:<name>` URI fragments,
//! we strip the prefix and look up bytes in `page_template_assets`
//! keyed by `(template_id, name)`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use journal_core::AssetResolver;
use journal_storage::JournalBackend;
use uuid::Uuid;

pub struct PageTemplateAssetResolver {
    backend: Rc<RefCell<dyn JournalBackend>>,
    template_id: Uuid,
}

impl PageTemplateAssetResolver {
    pub fn new(backend: Rc<RefCell<dyn JournalBackend>>, template_id: Uuid) -> Self {
        Self {
            backend,
            template_id,
        }
    }
}

impl AssetResolver for PageTemplateAssetResolver {
    fn resolve(&self, name: &str) -> Option<Arc<[u8]>> {
        let stripped = name.strip_prefix("asset:").unwrap_or(name);
        let mut b = self.backend.borrow_mut();
        b.get_page_template_asset(self.template_id, stripped)
            .ok()
            .flatten()
            .map(|a| Arc::from(a.bytes.into_boxed_slice()))
    }
}

/// No-op resolver for code paths that never reference image/PDF
/// backgrounds (e.g. tool-editor preview, which always renders on a
/// blank background).
pub fn null_resolver() -> impl AssetResolver {
    |_: &str| None
}
