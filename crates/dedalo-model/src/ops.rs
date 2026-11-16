//! Edit operations addressed by element ID (spec 13.3).
//!
//! These are the data types only: they are part of the published `ops`
//! schema and are what a diagnostic's `fix` carries. The engine that applies
//! them to a syntax tree lives in `dedalo-ops`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::value::Value;

/// A batch of operations, applied in order and atomically.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    /// Content hash of the source the batch was computed against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_hash: Option<String>,
    pub ops: Vec<Op>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    /// Inserts an element given as a DSL fragment (`source`) or as `kind`,
    /// `id` and `set`. `parent` nests it in a space.
    Add {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        set: BTreeMap<String, Value>,
    },
    /// Sets or unsets attributes by dotted path, with values in document
    /// JSON form.
    Update {
        id: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        set: BTreeMap<String, Value>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        unset: Vec<String>,
    },
    /// Deletes an element. Fails with `REFERENCED_ELEMENT` unless `cascade`.
    Remove {
        id: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cascade: bool,
    },
    /// Rewrites a room's layout, or an element's space and position.
    Move {
        id: String,
        to: BTreeMap<String, Value>,
    },
    /// Shorthand for adding a connection.
    Connect {
        id: String,
        from: String,
        to: String,
        #[serde(rename = "type")]
        ty: String,
    },
}

impl Op {
    /// An `update` that sets one attribute.
    pub fn set(id: &str, path: &str, value: Value) -> Op {
        Op::Update {
            id: id.to_string(),
            set: BTreeMap::from([(path.to_string(), value)]),
            unset: Vec::new(),
        }
    }
}
