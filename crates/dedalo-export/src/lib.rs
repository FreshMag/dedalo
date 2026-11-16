//! Stage 6 of the pipeline: output formats beyond SVG (spec 9.6).
//!
//! This crate holds the text map. PNG (resvg) and PDF (krilla) arrive with
//! milestone M3; both are derived from the SVG the scene serialises to.

mod text_map;

pub use text_map::text_map;
