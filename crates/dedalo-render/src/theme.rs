//! Themes: palette, stroke widths, text sizes and marker symbols (spec 9.4).
//! A theme is data. The built-in `classic` theme is a pack file read by the
//! same loader a third-party theme pack will use.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::symbol::{self, Art};

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

/// An entry of the symbol map: a marker, or the path of an SVG file in the
/// theme's package, such as `symbols/torch.svg`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SymbolRef {
    File(String),
    Mark(Symbol),
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
    pub symbols: BTreeMap<String, SymbolRef>,
    /// The symbol files, sanitised and converted, by the same keys.
    #[serde(skip)]
    pub art: BTreeMap<String, Art>,
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

    /// The entry for a type and subtype: the most specific one wins.
    fn entry(&self, ty: &str, subtype: Option<&str>) -> Option<(String, &SymbolRef)> {
        let specific = subtype.map(|s| format!("{ty}.{s}"));
        [specific, Some(ty.to_string())]
            .into_iter()
            .flatten()
            .find_map(|key| self.symbols.get(&key).map(|entry| (key, entry)))
    }

    /// The marker for a type and subtype.
    pub fn symbol(&self, ty: &str, subtype: Option<&str>) -> Option<&Symbol> {
        match self.entry(ty, subtype)?.1 {
            SymbolRef::Mark(symbol) => Some(symbol),
            SymbolRef::File(_) => None,
        }
    }

    /// The drawn symbol for a type and subtype, when the theme maps the
    /// most specific entry to a file.
    pub fn art(&self, ty: &str, subtype: Option<&str>) -> Option<&Art> {
        match self.entry(ty, subtype)? {
            (key, SymbolRef::File(_)) => self.art.get(&key),
            _ => None,
        }
    }

    /// Reads the symbol files the theme names. `read` returns a file of the
    /// theme's package by its path. Every file passes the sanitiser.
    pub fn load_symbols(&mut self, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<(), String> {
        for (key, entry) in &self.symbols {
            let SymbolRef::File(path) = entry else {
                continue;
            };
            let fail = |why: String| format!("the symbol `{key}` ({path}): {why}");
            if !path.ends_with(".svg") {
                return Err(fail("a symbol file is an `.svg` file".to_string()));
            }
            let bytes =
                read(path).ok_or_else(|| fail("the package has no such file".to_string()))?;
            if bytes.len() > symbol::MAX_BYTES {
                return Err(fail(format!(
                    "the file is {} bytes; a symbol is at most {}",
                    bytes.len(),
                    symbol::MAX_BYTES
                )));
            }
            let text = String::from_utf8(bytes).map_err(|_| fail("it is not UTF-8".to_string()))?;
            let art = symbol::parse(&text).map_err(fail)?;
            self.art.insert(key.clone(), art);
        }
        Ok(())
    }
}
