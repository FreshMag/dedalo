//! The registry: the one place built-ins, packs and plugins enter through
//! (spec section 11).
//!
//! Built-ins register through the same calls third parties get: the
//! catalog and the `classic` theme are pack files read by the ordinary
//! loaders, and the built-in painters go through the painter registry.
//! A plugin is a directory holding `dedalo-plugin.toml`: packs contribute
//! themes and catalog types as data, and WebAssembly plugins contribute
//! painters, scene passes, a renderer, exporters, validators and shapes.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use dedalo_catalog::Catalog;
use dedalo_render::{Painters, Theme};

pub mod manifest;
pub mod package;
pub mod points;
pub mod runtime;
pub mod version;

pub use manifest::{API_VERSION, Manifest};
pub use runtime::{CallError, PluginRuntime};

/// The theme used when a document names none.
pub const DEFAULT_THEME: &str = "classic";
/// The manifest file name.
pub const MANIFEST: &str = "dedalo-plugin.toml";
/// Largest package, all files together.
pub const MAX_PACKAGE: usize = 32 * 1024 * 1024;

/// A plugin that has been read and checked.
pub struct LoadedPlugin {
    pub manifest: Manifest,
    /// Every file of the package, by path relative to its root.
    pub files: BTreeMap<String, Vec<u8>>,
    /// SHA-256 of the package, as the lockfile records it.
    pub sha256: String,
    pub runtime: Option<Arc<dyn PluginRuntime>>,
}

impl std::fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("id", &self.manifest.plugin.id)
            .field("version", &self.manifest.plugin.version)
            .finish()
    }
}

impl LoadedPlugin {
    pub fn id(&self) -> &str {
        &self.manifest.plugin.id
    }
}

/// Builds the code runtime of a plugin from its module bytes and assets.
pub type RuntimeFactory<'a> =
    &'a dyn Fn(&[u8], BTreeMap<String, Vec<u8>>) -> Result<Arc<dyn PluginRuntime>, String>;

/// The wasmi runtime, for the binary.
#[cfg(feature = "wasmi")]
pub fn wasmi_factory(
    bytes: &[u8],
    assets: BTreeMap<String, Vec<u8>>,
) -> Result<Arc<dyn PluginRuntime>, String> {
    Ok(Arc::new(runtime::WasmiRuntime::load(bytes, assets)?))
}

/// The SHA-256 of a package: every file's path and bytes, in path order.
pub fn package_hash(files: &BTreeMap<String, Vec<u8>>) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for (path, bytes) in files {
        h.update((path.len() as u64).to_le_bytes());
        h.update(path.as_bytes());
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// Reads every file of a package directory.
pub fn read_dir(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    fn walk(
        root: &Path,
        dir: &Path,
        out: &mut BTreeMap<String, Vec<u8>>,
        total: &mut usize,
    ) -> Result<(), String> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| format!("cannot read `{}`: {e}", dir.display()))?
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.')
                || name == "target"
                || name == "src"
                || name == "Cargo.toml"
                || name == "Cargo.lock"
            {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out, total)?;
            } else {
                let bytes = std::fs::read(&path)
                    .map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
                *total += bytes.len();
                if *total > MAX_PACKAGE {
                    return Err(format!(
                        "the package is larger than {} MB",
                        MAX_PACKAGE / 1024 / 1024
                    ));
                }
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, bytes);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out, &mut 0)?;
    Ok(out)
}

/// Builds a plugin from its files. `factory` makes the runtime of a code
/// plugin; without one, a code plugin cannot be loaded.
pub fn load_files(
    files: BTreeMap<String, Vec<u8>>,
    factory: Option<RuntimeFactory>,
) -> Result<LoadedPlugin, String> {
    let text = files
        .get(MANIFEST)
        .ok_or_else(|| format!("the package has no `{MANIFEST}`"))?;
    let manifest = Manifest::parse(&String::from_utf8_lossy(text))?;
    let runtime = match &manifest.wasm {
        None => None,
        Some(wasm) => {
            let bytes = files.get(&wasm.module).ok_or_else(|| {
                format!(
                    "the manifest names `{}`, which the package does not contain",
                    wasm.module
                )
            })?;
            let factory = factory.ok_or("this build cannot run WebAssembly plugins")?;
            Some(factory(bytes, files.clone())?)
        }
    };
    for decl in manifest
        .theme
        .values()
        .map(|t| &t.file)
        .chain(manifest.catalog.iter().map(|c| &c.file))
    {
        if !files.contains_key(decl) {
            return Err(format!(
                "the manifest names `{decl}`, which the package does not contain"
            ));
        }
    }
    let sha256 = package_hash(&files);
    Ok(LoadedPlugin {
        manifest,
        files,
        sha256,
        runtime,
    })
}

/// The catalog, themes, painters and plugins in effect.
pub struct Registry {
    pub catalog: Catalog,
    pub themes: BTreeMap<String, Theme>,
    pub painters: Painters,
    pub plugins: Vec<Arc<LoadedPlugin>>,
    /// Templates from packs: the name, `pluginid.name`, and the text of
    /// the `template` block.
    pub templates: Vec<(String, String)>,
}

impl Registry {
    /// The registry of a tool with no third-party extensions.
    pub fn builtin() -> Registry {
        let mut registry = Registry {
            catalog: Catalog::builtin(),
            themes: BTreeMap::new(),
            painters: Painters::builtin(),
            plugins: Vec::new(),
            templates: Vec::new(),
        };
        registry.add_theme(Theme::classic());
        registry
    }

    pub fn add_theme(&mut self, theme: Theme) {
        self.themes.insert(theme.id().to_string(), theme);
    }

    /// The theme with this name, or the default theme when none is asked for.
    pub fn theme(&self, name: Option<&str>) -> Option<&Theme> {
        self.themes.get(name.unwrap_or(DEFAULT_THEME))
    }

    pub fn plugin(&self, id: &str) -> Option<&Arc<LoadedPlugin>> {
        self.plugins.iter().find(|p| p.id() == id)
    }

    /// Adds a plugin: its themes and catalog types join the registry under
    /// its namespace, so two plugins cannot collide on a name (spec 11.3).
    pub fn add_plugin(&mut self, plugin: LoadedPlugin) -> Result<(), String> {
        let id = plugin.id().to_string();
        if self.plugin(&id).is_some() {
            return Err(format!("the plugin `{id}` is loaded twice"));
        }
        for (name, file) in &plugin.manifest.theme {
            let text = String::from_utf8_lossy(&plugin.files[&file.file]).into_owned();
            let mut theme = Theme::from_toml(&text)
                .map_err(|e| format!("the theme `{id}.{name}` is not valid: {e}"))?;
            theme.theme.id = format!("{id}.{name}");
            // Symbol files are named from the theme file's folder, or from
            // the root of the package.
            let folder = file.file.rsplit_once('/').map(|(dir, _)| dir);
            theme
                .load_symbols(&|path| {
                    folder
                        .and_then(|dir| plugin.files.get(&format!("{dir}/{path}")))
                        .or_else(|| plugin.files.get(path))
                        .cloned()
                })
                .map_err(|e| format!("the theme `{id}.{name}` is not valid: {e}"))?;
            // What a theme leaves out comes from `classic`.
            let base = Theme::classic();
            for (k, v) in base.palette {
                theme.palette.entry(k).or_insert(v);
            }
            for (k, v) in base.stroke {
                theme.stroke.entry(k).or_insert(v);
            }
            for (k, v) in base.text {
                theme.text.entry(k).or_insert(v);
            }
            for (k, v) in base.symbols {
                theme.symbols.entry(k).or_insert(v);
            }
            self.add_theme(theme);
        }
        if let Some(file) = &plugin.manifest.catalog {
            let text = String::from_utf8_lossy(&plugin.files[&file.file]).into_owned();
            let catalog = Catalog::from_toml(&text)
                .map_err(|e| format!("the catalog of `{id}` is not valid: {e}"))?;
            for (name, entry) in catalog.types {
                self.catalog.types.insert(format!("{id}.{name}"), entry);
            }
        }
        for file in &plugin.manifest.template {
            let text = String::from_utf8_lossy(plugin.files.get(&file.file).ok_or_else(|| {
                format!(
                    "the manifest names `{}`, which the package does not contain",
                    file.file
                )
            })?)
            .into_owned();
            let parsed = dedalo_syntax::parse(&text);
            if let Some(e) = parsed.errors.first() {
                return Err(format!(
                    "the template file `{}` of `{id}` is not valid: {}",
                    file.file, e.message
                ));
            }
            for node in parsed
                .syntax()
                .children()
                .filter(|n| n.kind() == dedalo_syntax::SyntaxKind::Block)
            {
                let block = dedalo_syntax::ast::Block(node.clone());
                if block.keyword_text() != "template" {
                    return Err(format!(
                        "the template file `{}` of `{id}` holds a `{}` block; it may hold `template` blocks only",
                        file.file,
                        block.keyword_text()
                    ));
                }
                let Some(label) = block.labels().first().map(dedalo_syntax::ast::label_text) else {
                    continue;
                };
                self.templates
                    .push((format!("{id}.{label}"), node.text().to_string()));
            }
        }
        self.plugins.push(Arc::new(plugin));
        Ok(())
    }
}

impl Default for Registry {
    fn default() -> Self {
        Registry::builtin()
    }
}

/// Base64 for binary payloads inside JSON messages.
pub mod base64 {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = (chunk[0] as u32) << 16
                | (*chunk.get(1).unwrap_or(&0) as u32) << 8
                | *chunk.get(2).unwrap_or(&0) as u32;
            for k in 0..4 {
                out.push(if k <= chunk.len() {
                    B64[(n >> (18 - 6 * k) & 63) as usize] as char
                } else {
                    '='
                });
            }
        }
        out
    }

    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() / 4 * 3);
        let mut acc = 0u32;
        let mut bits = 0;
        for c in text.bytes().filter(|c| !c.is_ascii_whitespace()) {
            if c == b'=' {
                break;
            }
            let v = B64.iter().position(|x| *x == c)? as u32;
            acc = acc << 6 | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
                acc &= (1 << bits) - 1;
            }
        }
        Some(out)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn round_trip() {
            for text in [&b""[..], b"f", b"fo", b"foo", b"foobar", &[0, 255, 7, 128]] {
                assert_eq!(super::decode(&super::encode(text)).unwrap(), text);
            }
            assert_eq!(super::encode(b"foobar"), "Zm9vYmFy");
        }
    }
}
