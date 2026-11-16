//! Stage 6 of the pipeline: output formats beyond SVG (spec 9.6).
//!
//! The text map is drawn from the view. PNG and PDF are derived from the
//! SVG the scene serialises to: PNG with resvg, PDF with krilla.

#[cfg(feature = "raster")]
mod raster;
mod text_map;

#[cfg(feature = "raster")]
pub use raster::{MAX_PNG_SIDE, pdf, png, svg_size};
pub use text_map::text_map;

/// Longest side of a PNG, in pixels.
#[cfg(not(feature = "raster"))]
pub const MAX_PNG_SIDE: u32 = 16_384;
