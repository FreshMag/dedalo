//! The element catalog: a registry of element types (spec section 8).
//!
//! Each entry fixes, for one `type`, its class, footprint, property schema,
//! default visibility and legend group. The catalog is data: validation,
//! completion, legend grouping and the reference pages are generated from it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The built-in catalog, in the pack format of spec 8.1.
pub const BUILTIN_TOML: &str = include_str!("../builtin.toml");

/// Which block keyword a type is written with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Feature,
    Entity,
    /// Valid under both keywords; only `custom` uses this.
    Either,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropType {
    String,
    Integer,
    Number,
    Boolean,
    Symbol,
    Reference,
    Shape,
    Point,
    List,
}

impl PropType {
    pub fn describe(self) -> &'static str {
        match self {
            PropType::String => "a string",
            PropType::Integer => "a whole number",
            PropType::Number => "a number",
            PropType::Boolean => "true or false",
            PropType::Symbol => "a symbol",
            PropType::Reference => "an element ID",
            PropType::Shape => "a shape such as circle(2)",
            PropType::Point => "a point such as [3, 2]",
            PropType::List => "a list",
        }
    }
}

/// The schema of one property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropSpec {
    #[serde(rename = "type")]
    pub ty: PropType,
    /// Element type of a list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<PropType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Allowed values of a symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

pub type PropSchema = BTreeMap<String, PropSpec>;

fn one_cell() -> [u32; 2] {
    [1, 1]
}
fn public() -> String {
    "public".to_string()
}
fn features_layer() -> String {
    "features".to_string()
}
fn question_mark() -> String {
    "?".to_string()
}

/// One element type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeEntry {
    pub class: Class,
    /// Legend group and export visibility toggle.
    pub category: String,
    /// Legend label of the type.
    pub label: String,
    /// The allowed subtypes. Empty means the subtype is free text.
    #[serde(default)]
    pub subtypes: Vec<String>,
    /// Default footprint in cells, `[width, height]`.
    #[serde(default = "one_cell")]
    pub footprint: [u32; 2],
    /// Footprints that differ from the default, by subtype.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub footprints: BTreeMap<String, [u32; 2]>,
    /// Whether the element blocks its cells for other blocking elements.
    #[serde(default)]
    pub blocking: bool,
    #[serde(default = "public")]
    pub default_visibility: String,
    /// Scene layer the element is drawn on.
    #[serde(default = "features_layer")]
    pub layer: String,
    /// Character drawn on the text map.
    #[serde(default = "question_mark")]
    pub glyph: String,
    #[serde(default)]
    pub properties: PropSchema,
}

impl TypeEntry {
    pub fn footprint_of(&self, subtype: Option<&str>) -> [u32; 2] {
        subtype
            .and_then(|s| self.footprints.get(s))
            .copied()
            .unwrap_or(self.footprint)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropGroup {
    #[serde(default)]
    pub properties: PropSchema,
}

/// The catalog in effect: built-in entries plus those added by packs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    /// Properties accepted on every element and space.
    #[serde(default)]
    pub common: PropGroup,
    /// Property schemas of the structural kinds: `room`, `corridor`, `connection`.
    #[serde(default)]
    pub structure: BTreeMap<String, PropGroup>,
    #[serde(default, rename = "type")]
    pub types: BTreeMap<String, TypeEntry>,
}

/// The fixed order of legend groups and visibility toggles (spec 9.1).
pub const CATEGORIES: &[&str] = &[
    "features",
    "mechanisms",
    "traps",
    "lighting",
    "terrain",
    "creatures",
    "treasure",
];

impl Catalog {
    /// Parses a catalog in the pack format.
    pub fn from_toml(text: &str) -> Result<Catalog, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// The catalog shipped with the tool.
    pub fn builtin() -> Catalog {
        Catalog::from_toml(BUILTIN_TOML).expect("the built-in catalog is valid")
    }

    pub fn get(&self, ty: &str) -> Option<&TypeEntry> {
        self.types.get(ty)
    }

    /// The schema of one property of an element type, falling back to the
    /// common properties.
    pub fn property(&self, ty: &str, name: &str) -> Option<&PropSpec> {
        self.types
            .get(ty)
            .and_then(|t| t.properties.get(name))
            .or_else(|| self.common.properties.get(name))
    }

    /// The schema of one property of a structural kind.
    pub fn structure_property(&self, kind: &str, name: &str) -> Option<&PropSpec> {
        self.structure
            .get(kind)
            .and_then(|g| g.properties.get(name))
            .or_else(|| self.common.properties.get(name))
    }

    /// Every property name valid for an element type.
    pub fn property_names(&self, ty: &str) -> Vec<&str> {
        let own = self
            .types
            .get(ty)
            .into_iter()
            .flat_map(|t| t.properties.keys());
        own.chain(self.common.properties.keys())
            .map(String::as_str)
            .collect()
    }

    /// Every property name valid for a structural kind.
    pub fn structure_property_names(&self, kind: &str) -> Vec<&str> {
        let own = self
            .structure
            .get(kind)
            .into_iter()
            .flat_map(|g| g.properties.keys());
        own.chain(self.common.properties.keys())
            .map(String::as_str)
            .collect()
    }
}

/// `pressure_plate` to `Pressure plate`.
pub fn title_case(name: &str) -> String {
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_loads_and_covers_release_0_1() {
        let cat = Catalog::builtin();
        for ty in [
            "architecture",
            "furniture",
            "mechanism",
            "trap",
            "lighting",
            "terrain",
            "container",
            "creature",
            "npc",
            "item",
            "custom",
        ] {
            let entry = cat.get(ty).unwrap_or_else(|| panic!("missing type {ty}"));
            assert!(
                CATEGORIES.contains(&entry.category.as_str()),
                "{ty} has an unknown category"
            );
        }
        assert_eq!(cat.get("trap").unwrap().default_visibility, "dm");
        assert_eq!(
            cat.get("furniture").unwrap().footprint_of(Some("table")),
            [2, 1]
        );
        assert!(cat.property("trap", "description").is_some());
    }
}
