//! ID-addressed edit operations (spec 13.3).
//!
//! An agent or the editor changes one element without regenerating the
//! file: operations patch the syntax tree, so hand-written layout and
//! comments stay where they were.

mod diff;
mod engine;
mod value;

use sha2::{Digest, Sha256};

pub use dedalo_model::ops::{Batch, Op};
pub use diff::unified;
pub use engine::{Applied, ELEMENT_KEYWORDS, apply_ops};
pub use value::to_dsl;

/// The content hash of a source, as `base_hash` carries it.
pub fn content_hash(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}
