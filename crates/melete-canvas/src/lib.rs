// Renderer entry points threading transform + state + scene parameters
// trip clippy's too_many_arguments / type_complexity lints. The shapes
// are cross-crate API surfaces, not bugs — silence at crate level.
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod background_renderer;
pub mod built_in_brushes;
pub mod grid_renderer;
pub mod viewport_transform;

#[cfg(feature = "desktop")]
pub mod renderer;
#[cfg(feature = "desktop")]
pub mod stroke_renderer;
#[cfg(feature = "desktop")]
pub mod widget_renderer;

#[cfg(feature = "vello")]
pub mod vello_renderer;

pub use background_renderer::{scale_background, BackgroundConfig};
#[cfg(feature = "desktop")]
pub use background_renderer::draw_page_bounds_outline;
pub use grid_renderer::GridSettings;
#[cfg(feature = "desktop")]
pub use renderer::{
    draw_lasso_overlay, draw_selection_handles, hit_test_handle, paint, paint_with_widgets,
    paint_with_widgets_ctx, selection_combined_bbox,
};
pub use viewport_transform::ViewportTransform;
#[cfg(feature = "desktop")]
pub use widget_renderer::{draw_widgets, draw_widgets_with_context, WidgetRenderContext};
