//! The plugin manifest, `dedalo-plugin.toml` (spec 11.4).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The plugin API version this host implements. A host supports the current
/// version and the previous one (spec 11.8).
pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub plugin: Meta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm: Option<Wasm>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scene_pass: Vec<Named>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub painter: Vec<PainterDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer: Option<Named>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exporter: Vec<ExporterDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validator: Vec<Named>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shape: Vec<Named>,
    /// Themes by name; each is a theme file in the package.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub theme: BTreeMap<String, FileRef>,
    /// Catalog entries in the format of spec 8.1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<FileRef>,
    /// Template files of `template` blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template: Vec<FileRef>,
    /// Options a document may set in `render { plugin "id" { ... } }`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, OptionDecl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub version: String,
    /// The plugin API version.
    pub api: u32,
    /// The Dedalo versions the plugin works with, such as `>=0.2`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedalo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wasm {
    pub module: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Named {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRef {
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Match {
    /// `space`, `wall`, `opening`, `element` or `note`.
    pub kind: String,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PainterDecl {
    #[serde(rename = "match")]
    pub matches: Match,
    /// The scene layer the nodes go to.
    pub layer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExporterDecl {
    /// The format name; documents write it as `pluginid.format`.
    pub format: String,
    pub extension: String,
    pub media_type: String,
    /// Host renderings the exporter receives: `svg`, `png`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionDecl {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<toml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let manifest: Manifest =
            toml::from_str(text).map_err(|e| format!("dedalo-plugin.toml is not valid: {e}"))?;
        manifest.check()?;
        Ok(manifest)
    }

    fn check(&self) -> Result<(), String> {
        let id = &self.plugin.id;
        let valid = id.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !valid || id.len() > 64 {
            return Err(format!(
                "`{id}` is not a valid plugin ID: lowercase letters, digits and underscores, starting with a letter"
            ));
        }
        if self.plugin.api != API_VERSION && self.plugin.api + 1 != API_VERSION {
            return Err(format!(
                "the plugin uses API version {}, but this Dedalo supports {} and {}",
                self.plugin.api,
                API_VERSION.saturating_sub(1),
                API_VERSION
            ));
        }
        crate::version::Version::parse(&self.plugin.version)
            .ok_or_else(|| format!("`{}` is not a version such as 1.2.0", self.plugin.version))?;
        let code = !self.scene_pass.is_empty()
            || !self.painter.is_empty()
            || self.renderer.is_some()
            || !self.exporter.is_empty()
            || !self.validator.is_empty()
            || !self.shape.is_empty();
        if code && self.wasm.is_none() {
            return Err(
                "the manifest declares code extension points but no `[wasm] module`".to_string(),
            );
        }
        Ok(())
    }

    /// The default options, as JSON.
    pub fn default_options(&self) -> serde_json::Map<String, serde_json::Value> {
        self.options
            .iter()
            .filter_map(|(k, o)| Some((k.clone(), serde_json::to_value(o.default.as_ref()?).ok()?)))
            .collect()
    }
}
