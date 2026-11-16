//! Stage 3 of the pipeline: geometry and layout (spec section 7).
//!
//! Every space is a closed polygon on a grid of cells, every wall is a
//! boundary edge of a space, and every door is an opening cut into a wall by
//! a connection. Coordinates are integers in thousandths of a cell, so the
//! results are identical on every platform.

mod model;
mod overlap;
mod prim;
mod resolve;
mod shape;
mod walls;

pub use model::{Circle, Opening, RElement, RNote, RSpace, Reach, Resolved, Wall};
pub use overlap::interiors_intersect;
pub use prim::{C, CELL, Loc, Pt, Rect, locate, locate_scaled};
pub use resolve::{ShapeHook, resolve, resolve_with};
pub use walls::derive_walls;
