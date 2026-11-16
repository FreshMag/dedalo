//! ID-addressed edit operations (spec 13.3). Milestone M5.
//!
//! The operation data types already exist in `dedalo_model::ops`, because
//! diagnostics carry them as fixes and they are part of the published `ops`
//! schema. This crate will hold the engine that applies a batch to the
//! syntax tree: text, comments and formatting outside the touched nodes are
//! preserved byte for byte, and a batch is atomic.
//!
//! Not implemented yet. `dedalo_syntax::edit` provides the text-edit
//! primitive the engine will build on.

pub use dedalo_model::ops::{Batch, Op};
pub use dedalo_syntax::edit::{TextEdit, apply};
