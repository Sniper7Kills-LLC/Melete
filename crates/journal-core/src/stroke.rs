use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pen::PenSettings;
use crate::Rect;

/// A single point in a stroke, capturing stylus input data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokePoint {
    pub x: f64,
    pub y: f64,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub timestamp_ms: u64,
}

/// A stroke drawn on a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub id: Uuid,
    pub points: Vec<StrokePoint>,
    pub pen: PenSettings,
    pub zoom_at_creation: f64,
    pub bounding_box: Rect,
}
