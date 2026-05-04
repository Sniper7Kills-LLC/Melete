//! Grid-pattern page templates — dotted, square, isometric, hex,
//! engineering graph.

pub mod dotted;
pub mod engineering_graph;
pub mod grid;
pub mod hexagonal;
pub mod isometric;

pub use dotted::{builtin_dotted, BUILTIN_DOTTED_ID};
pub use engineering_graph::{builtin_engineering_graph, BUILTIN_ENGINEERING_GRAPH_ID};
pub use grid::{builtin_grid, BUILTIN_GRID_ID};
pub use hexagonal::{builtin_hexagonal, BUILTIN_HEX_ID};
pub use isometric::{builtin_isometric, BUILTIN_ISOMETRIC_ID};
