// Renderer entry points threading transform + state + scene parameters
// trip clippy's too_many_arguments / type_complexity lints. The shapes
// are cross-crate API surfaces, not bugs — silence at crate level.
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod background_renderer;
pub mod built_in_brushes;
pub mod grid_renderer;
pub mod renderer;
pub mod stroke_renderer;
pub mod viewport_transform;
pub mod widget_renderer;

#[cfg(feature = "vello")]
pub mod vello_renderer;

pub use background_renderer::{draw_page_bounds_outline, scale_background, BackgroundConfig};
pub use grid_renderer::GridSettings;
pub use renderer::{
    draw_lasso_overlay, draw_selection_handles, hit_test_handle, paint, paint_with_widgets,
    paint_with_widgets_ctx, selection_combined_bbox,
};
pub use viewport_transform::ViewportTransform;
pub use widget_renderer::{draw_widgets, draw_widgets_with_context, WidgetRenderContext};
