pub mod built_in_brushes;
pub mod renderer;
pub mod viewport_transform;
pub mod stroke_renderer;
pub mod grid_renderer;
pub mod background_renderer;
pub mod widget_renderer;

#[cfg(feature = "vello")]
pub mod vello_renderer;

pub use renderer::{
    draw_lasso_overlay, draw_selection_handles, hit_test_handle, paint, paint_with_widgets,
    paint_with_widgets_ctx, selection_combined_bbox,
};
pub use viewport_transform::ViewportTransform;
pub use grid_renderer::GridSettings;
pub use background_renderer::{scale_background, BackgroundConfig, draw_page_bounds_outline};
pub use widget_renderer::{draw_widgets, draw_widgets_with_context, WidgetRenderContext};
