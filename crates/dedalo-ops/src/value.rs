//! Values in document JSON form to DSL text.
//!
//! An edit operation carries values as the document model stores them:
//! symbols and references are both strings. The attribute path says which
//! spelling the DSL wants, as the schema does for the reader (spec 5.4).

use dedalo_catalog::{PropSpec, PropType};
use dedalo_model::Value;
use dedalo_syntax::ast::{is_symbol, quote};

/// Attributes whose value is written as a bare symbol.
const SYMBOLS: &[&str] = &[
    "type",
    "subtype",
    "visibility",
    "facing",
    "wall",
    "anchor",
    "direction",
    "align",
    "mode",
    "preset",
    "format",
    "resolution",
    "bounds",
    "page",
    "orientation",
    "scale",
    "theme",
    "default_wall",
    "default_floor",
];

/// How strings are spelt at one place in the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    Symbol,
    Quoted,
}

/// The spelling of the strings of an attribute, by its dotted path.
pub fn spelling(path: &str) -> Spelling {
    let leaf = path.rsplit('.').next().unwrap_or(path);
    let symbol = match leaf {
        // `text` is a string on a note and a symbol in a `render` block.
        "text" => path.ends_with("render.text"),
        // `legend.mode` and `layout.mode` are symbols; nothing else is called `mode`.
        _ => SYMBOLS.contains(&leaf),
    };
    if symbol {
        Spelling::Symbol
    } else {
        Spelling::Quoted
    }
}

fn string(text: &str, spelling: Spelling) -> String {
    if spelling == Spelling::Symbol && is_symbol(text) {
        text.to_string()
    } else {
        quote(text)
    }
}

fn spelling_of(ty: PropType) -> Spelling {
    if ty == PropType::Symbol {
        Spelling::Symbol
    } else {
        Spelling::Quoted
    }
}

/// Writes a value for the attribute at `path`. For a property, `prop` is its
/// catalog schema when the catalog defines it.
pub fn to_dsl(path: &str, value: &Value, prop: Option<&PropSpec>) -> String {
    match prop {
        Some(spec) => write(value, spelling_of(spec.ty), spec.of.map(spelling_of)),
        None if path.starts_with("properties.") => write(value, Spelling::Quoted, None),
        None => {
            let s = spelling(path);
            write(value, s, Some(s))
        }
    }
}

fn write(value: &Value, spelling: Spelling, items: Option<Spelling>) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => n.to_string(),
        Value::Str(s) => string(s, spelling),
        Value::List(list) => {
            let inner = items.unwrap_or(Spelling::Quoted);
            let parts: Vec<String> = list.iter().map(|v| write(v, inner, Some(inner))).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Map(map) if value.is_call() => {
            let name = map.get("fn").and_then(Value::as_str).unwrap_or_default();
            let args = match map.get("args") {
                Some(Value::List(args)) => args
                    .iter()
                    .map(|a| write(a, Spelling::Quoted, None))
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => String::new(),
            };
            format!("{name}({args})")
        }
        Value::Map(map) if map.is_empty() => "{}".to_string(),
        Value::Map(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{} = {}",
                        if is_symbol(k) { k.clone() } else { quote(k) },
                        write(v, Spelling::Quoted, None)
                    )
                })
                .collect();
            format!("{{ {} }}", entries.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dedalo_model::Num;

    use super::*;

    #[test]
    fn spelling_follows_the_path() {
        let s = |t: &str| Value::Str(t.to_string());
        assert_eq!(to_dsl("type", &s("door"), None), "door");
        assert_eq!(to_dsl("placement.wall", &s("north"), None), "north");
        assert_eq!(to_dsl("from", &s("entrance"), None), "\"entrance\"");
        assert_eq!(to_dsl("text", &s("Hi"), None), "\"Hi\"");
        assert_eq!(to_dsl("render.text", &s("live"), None), "live");
        assert_eq!(
            to_dsl(
                "position",
                &Value::List(vec![Value::Num(Num::int(2)), Value::Num(Num(1500))]),
                None
            ),
            "[2, 1.5]"
        );
        let shape = Value::Map(BTreeMap::from([
            ("fn".to_string(), s("rectangle")),
            (
                "args".to_string(),
                Value::List(vec![Value::Num(Num::int(6)), Value::Num(Num::int(5))]),
            ),
        ]));
        assert_eq!(to_dsl("shape", &shape, None), "rectangle(6, 5)");
        assert_eq!(
            to_dsl("connects", &Value::List(vec![s("a"), s("b")]), None),
            "[\"a\", \"b\"]"
        );
    }
}
