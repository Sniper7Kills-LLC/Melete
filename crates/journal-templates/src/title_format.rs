//! Re-exports of the title-format engine, which now lives in `journal_core`
//! so that `journal_canvas` can also use it for widget text substitution
//! without taking a dependency on this crate.

pub use journal_core::title_format::{render, TitleContext};
