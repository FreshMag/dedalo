//! Stages 4 and 5 of the pipeline: the audience-filtered view, and painting
//! it into a scene (spec section 9).
//!
//! Redaction happens before rendering: a player map is painted from a
//! filtered model, so secret doors, traps and DM notes never reach the scene.

mod paint;
mod render;
mod theme;
mod view;

pub use paint::{Cx, Item, Painter, Painters};
pub use render::render_scene;
pub use theme::{CLASSIC_TOML, Symbol, Theme};
pub use view::{Audience, ExportSpec, View, build_view};
