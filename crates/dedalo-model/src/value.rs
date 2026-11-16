//! Numbers and free-form property values.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A number in thousandths, the only numeric type of the model.
///
/// Geometry is integer arithmetic on thousandths of a cell (spec 7.1), which
/// keeps results identical on every platform. The DSL accepts at most three
/// decimals, so every written number is represented exactly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Num(pub i64);

impl Num {
    pub const ZERO: Num = Num(0);

    /// A whole number of units.
    pub const fn int(n: i64) -> Num {
        Num(n * 1000)
    }

    pub fn is_int(self) -> bool {
        self.0 % 1000 == 0
    }

    /// The value when it is whole.
    pub fn as_int(self) -> Option<i64> {
        self.is_int().then_some(self.0 / 1000)
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 1000.0
    }

    /// Parses a decimal literal with at most three decimals.
    pub fn parse(text: &str) -> Result<Num, String> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !frac.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(format!("`{text}` is not a decimal number."));
        }
        if frac.len() > 3 {
            return Err(format!(
                "`{text}` has more than three decimals; Dedalo numbers carry at most three."
            ));
        }
        let whole: i64 = whole
            .parse()
            .map_err(|_| format!("`{text}` is too large."))?;
        if whole > 1_000_000_000 {
            return Err(format!(
                "`{text}` is too large; the largest number is 1000000000."
            ));
        }
        let frac_value: i64 = format!("{frac:0<3}").parse().unwrap_or(0);
        let value = whole * 1000 + frac_value;
        Ok(Num(if negative { -value } else { value }))
    }
}

impl fmt::Display for Num {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let (whole, frac) = (abs / 1000, abs % 1000);
        if frac == 0 {
            write!(f, "{sign}{whole}")
        } else {
            let digits = format!("{frac:03}");
            write!(f, "{sign}{whole}.{}", digits.trim_end_matches('0'))
        }
    }
}

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.as_int() {
            Some(n) => s.serialize_i64(n),
            None => s.serialize_f64(self.to_f64()),
        }
    }
}

struct NumVisitor;

impl Visitor<'_> for NumVisitor {
    type Value = Num;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number with at most three decimals")
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Num, E> {
        if v.abs() > 1_000_000_000 {
            return Err(E::custom("number out of range"));
        }
        Ok(Num::int(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Num, E> {
        if v > 1_000_000_000 {
            return Err(E::custom("number out of range"));
        }
        Ok(Num::int(v as i64))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Num, E> {
        if !v.is_finite() || v.abs() > 1e9 {
            return Err(E::custom("number out of range"));
        }
        Ok(Num((v * 1000.0).round() as i64))
    }
}

impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Num, D::Error> {
        d.deserialize_any(NumVisitor)
    }
}

impl JsonSchema for Num {
    fn schema_name() -> Cow<'static, str> {
        "Number".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "number", "description": "A number with at most three decimals." })
    }
}

/// A free-form value: the content of `properties` and the arguments of shapes.
///
/// Symbols and references are strings here; the catalog says which is which.
/// A shape is a map with `fn` and `args`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Num(Num),
    Str(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_num(&self) -> Option<Num> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// True for a map shaped like a shape call: `{ "fn": ..., "args": [...] }`.
    pub fn is_call(&self) -> bool {
        match self {
            Value::Map(m) => {
                m.len() == 2
                    && matches!(m.get("fn"), Some(Value::Str(_)))
                    && matches!(m.get("args"), Some(Value::List(_)))
            }
            _ => false,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(b) => format!("`{b}`"),
            Value::Num(n) => format!("the number {n}"),
            Value::Str(s) => format!("\"{s}\""),
            Value::List(_) => "a list".to_string(),
            Value::Map(_) if self.is_call() => "a shape".to_string(),
            Value::Map(_) => "an object".to_string(),
        }
    }
}

impl JsonSchema for Value {
    fn schema_name() -> Cow<'static, str> {
        "Value".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "description": "Any JSON value. Numbers carry at most three decimals." })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_parse_print_and_round_trip() {
        for (text, raw) in [
            ("0", 0),
            ("8", 8000),
            ("-3", -3000),
            ("2.5", 2500),
            ("0.001", 1),
            ("-0.25", -250),
            ("1.50", 1500),
        ] {
            assert_eq!(Num::parse(text).unwrap(), Num(raw), "{text}");
        }
        assert_eq!(Num(1500).to_string(), "1.5");
        assert_eq!(Num(-250).to_string(), "-0.25");
        assert_eq!(Num(8000).to_string(), "8");
        assert!(Num::parse("1.2345").is_err());
        for raw in [0, 1, -1, 999, 2500, -12345, 8000] {
            let json = serde_json::to_string(&Num(raw)).unwrap();
            assert_eq!(
                serde_json::from_str::<Num>(&json).unwrap(),
                Num(raw),
                "{json}"
            );
            assert_eq!(json, Num(raw).to_string());
        }
    }

    #[test]
    fn values_round_trip_through_json() {
        let json =
            r#"{"a":null,"b":true,"c":2.5,"d":"x","e":[1,"y"],"f":{"args":[2],"fn":"circle"}}"#;
        let value: Value = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
    }
}
