use serde::{Deserialize, Serialize};

use crate::Point;

/// Describes the current view into a page (pan, zoom, rotation).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub center: Point,
    pub zoom: f64,
    pub rotation: f64,
}
