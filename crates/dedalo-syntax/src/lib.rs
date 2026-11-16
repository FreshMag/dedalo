//! Stage 1 of the pipeline: text to a lossless syntax tree.
//!
//! The DSL is a strict subset of HCL native syntax (spec section 5). The tree
//! keeps every byte of the source, including whitespace and comments, so the
//! formatter and the edit engine can rewrite a file without disturbing the
//! parts they do not touch. The parser recovers from errors and reports all of
//! them at once.

pub mod ast;
pub mod edit;
pub mod fmt;
mod kind;
mod lexer;
mod parser;
pub mod text;

pub use kind::{Lang, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};
pub use parser::{Parse, SyntaxError, parse};
pub use rowan::{NodeOrToken, TextRange, TextSize};

/// Deepest nesting of blocks and values the parser accepts (spec 10.3 bounds
/// documents at 16; the parser allows more so the limit is reported as a
/// diagnostic by the lower stage instead of as a syntax error).
pub const MAX_PARSE_DEPTH: usize = 64;
