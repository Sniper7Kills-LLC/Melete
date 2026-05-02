use std::cell::RefCell;
use std::rc::Rc;

use journal_canvas::{BackgroundConfig, GridSettings, ViewportTransform};
use journal_core::{Color, PenSettings, Rect, Stroke, Viewport};

pub struct CanvasState {
    pub transform: ViewportTransform,
    pub strokes: Vec<Stroke>,
    pub current_stroke: Option<Stroke>,
    pub pen: PenSettings,
    pub background: BackgroundConfig,
    pub page_rect: Rect,
}

pub type SharedState = Rc<RefCell<CanvasState>>;

pub fn new_shared_state() -> SharedState {
    let viewport = Viewport {
        center: journal_core::Point { x: 408.0, y: 528.0 },
        zoom: 1.0,
        rotation: 0.0,
    };
    let transform = ViewportTransform::new(viewport, 1280.0, 800.0);

    let pen = PenSettings {
        color: Color { r: 20, g: 20, b: 20, a: 255 },
        base_width: 2.0,
        opacity: 1.0,
        blend_mode: journal_core::BlendMode::Normal,
    };

    let background = BackgroundConfig::Grid(GridSettings {
        base_spacing: 20.0,
        subdivisions: 4,
        color: Color { r: 200, g: 200, b: 220, a: 255 },
    });

    let page_rect = Rect { x: 0.0, y: 0.0, width: 816.0, height: 1056.0 };

    Rc::new(RefCell::new(CanvasState {
        transform,
        strokes: Vec::new(),
        current_stroke: None,
        pen,
        background,
        page_rect,
    }))
}
