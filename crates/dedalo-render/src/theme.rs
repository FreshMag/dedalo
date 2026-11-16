//! Themes: palette, stroke widths, text sizes and marker symbols (spec 9.4).
//! A theme is data. The built-in `classic` theme is a pack file read by the
//! same loader a third-party theme pack will use.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const CLASSIC_TOML: &str = include_str!("../classic.toml");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeMeta {
    pub id: String,
}

/// A marker drawn as a short text in a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Symbol {
    pub glyph: String,
    /// `diamond`, `circle`, `square` or `none`.
    #[serde(default)]
    pub frame: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub theme: ThemeMeta,
    #[serde(default)]
    pub palette: BTreeMap<String, String>,
    #[serde(default)]
    pub stroke: BTreeMap<String, f64>,
    #[serde(default)]
    pub text: BTreeMap<String, f64>,
    #[serde(default)]
    pub symbols: BTreeMap<String, Symbol>,
}

impl Theme {
    pub fn from_toml(text: &str) -> Result<Theme, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn classic() -> Theme {
        Theme::from_toml(CLASSIC_TOML).expect("the built-in theme is valid")
    }

    pub fn id(&self) -> &str {
        &self.theme.id
    }

    /// A palette colour; black when the theme does not define it.
    pub fn color(&self, name: &str) -> &str {
        self.palette.get(name).map_or("#000000", String::as_str)
    }

    /// A stroke width in cells.
    pub fn stroke(&self, name: &str) -> f64 {
        self.stroke.get(name).copied().unwrap_or(0.05)
    }

    /// A text size in cells.
    pub fn size(&self, name: &str) -> f64 {
        self.text.get(name).copied().unwrap_or(0.4)
    }

    /// The marker for a type and subtype: the most specific entry wins.
    pub fn symbol(&self, ty: &str, subtype: Option<&str>) -> Option<&Symbol> {
        subtype
            .and_then(|s| self.symbols.get(&format!("{ty}.{s}")))
            .or_else(|| self.symbols.get(ty))
    }
}
