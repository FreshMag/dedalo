//! Stage 2 of the pipeline: the document model and everything that reads or
//! writes it (spec sections 5, 6 and 10).

pub mod check;
pub mod diag;
pub mod document;
pub mod lower;
pub mod ops;
pub mod suggest;
pub mod value;
pub mod write;

pub use diag::{Code, Diagnostic, Related, Report, Severity, SourceMap, Span, Status};
pub use document::*;
pub use value::{Num, Value};

/// The schemas this crate publishes (spec 6.4).
pub const SCHEMA_NAMES: &[&str] = &["document", "ops", "diagnostics"];

/// A published JSON Schema, generated from the Rust types.
pub fn schema(name: &str) -> Option<serde_json::Value> {
    let schema = match name {
        "document" => schemars::schema_for!(Document),
        "ops" => schemars::schema_for!(ops::Batch),
        "diagnostics" => schemars::schema_for!(Report),
        _ => return None,
    };
    serde_json::to_value(schema).ok()
}
