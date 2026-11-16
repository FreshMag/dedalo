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

/// The `authoring` profile of the document schema (spec 6.4 and 13.5): a
/// closed, non-recursive schema with every definition inlined, sized for
/// model structured output and function calling. Comments are left out,
/// and `properties` stay loosely typed; `validate` checks them against the
/// catalog.
pub fn authoring_schema() -> serde_json::Value {
    use serde_json::Value;
    let full = schema("document").expect("the document schema");
    let defs = full
        .get("$defs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    fn inline(v: &Value, defs: &serde_json::Map<String, Value>, depth: usize) -> Value {
        match v {
            Value::Object(map) => {
                if let Some(Value::String(r)) = map.get("$ref")
                    && let Some(def) = r.strip_prefix("#/$defs/").and_then(|n| defs.get(n))
                {
                    return if depth > 12 {
                        serde_json::json!({})
                    } else {
                        inline(def, defs, depth + 1)
                    };
                }
                let mut out = serde_json::Map::new();
                for (k, v) in map {
                    if k == "$defs" || k == "$schema" {
                        continue;
                    }
                    if k == "properties"
                        && let Value::Object(props) = v
                    {
                        let kept = props.iter().filter(|(name, _)| {
                            name.as_str() != "comments" && name.as_str() != "$schema"
                        });
                        out.insert(
                            k.clone(),
                            Value::Object(
                                kept.map(|(n, s)| (n.clone(), inline(s, defs, depth)))
                                    .collect(),
                            ),
                        );
                        continue;
                    }
                    out.insert(k.clone(), inline(v, defs, depth));
                }
                Value::Object(out)
            }
            Value::Array(items) => {
                Value::Array(items.iter().map(|i| inline(i, defs, depth)).collect())
            }
            other => other.clone(),
        }
    }
    let mut out = inline(&full, &defs, 0);
    if let Some(obj) = out.as_object_mut() {
        obj.insert(
            "$schema".to_string(),
            Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
        );
        obj.insert(
            "title".to_string(),
            Value::String("Dedalo document (authoring profile)".to_string()),
        );
    }
    out
}
