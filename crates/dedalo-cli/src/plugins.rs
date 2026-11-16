//! Plugins in the binary (spec 11.6): discovery, the lockfile and the
//! `dedalo plugin` commands.
//!
//! Plugins are searched in `--plugin` paths, then `.dedalo/plugins/` beside
//! the document and in the working directory, then the per-user
//! configuration directory. Rendering never downloads anything.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dedalo_core::Registry;
use dedalo_plugin::{LoadedPlugin, MANIFEST, wasmi_factory};
use serde::{Deserialize, Serialize};

/// The lockfile beside a document.
pub const LOCKFILE: &str = "dedalo.lock";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lock {
    #[serde(default)]
    pub plugin: Vec<LockEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockEntry {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

impl Lock {
    pub fn read(dir: &Path) -> Result<Option<Lock>, String> {
        let path = dir.join(LOCKFILE);
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
        toml::from_str(&text)
            .map(Some)
            .map_err(|e| format!("`{}` is not valid: {e}", path.display()))
    }

    pub fn write(&self, dir: &Path) -> Result<(), String> {
        let body = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let text = format!(
            "# Written by `dedalo plugin add`; checked on every render.\n# Change it with `dedalo plugin add` or `--update-lock`, not by hand.\n\n{body}"
        );
        std::fs::write(dir.join(LOCKFILE), text)
            .map_err(|e| format!("cannot write `{}`: {e}", dir.join(LOCKFILE).display()))
    }

    pub fn set(&mut self, plugin: &LoadedPlugin) {
        self.plugin.retain(|e| e.id != plugin.id());
        self.plugin.push(LockEntry {
            id: plugin.id().to_string(),
            version: plugin.manifest.plugin.version.clone(),
            sha256: plugin.sha256.clone(),
        });
        self.plugin.sort_by(|a, b| a.id.cmp(&b.id));
    }
}

/// The per-user plugin directory.
pub fn user_dir() -> Option<PathBuf> {
    let base = std::env::var_os("DEDALO_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|d| PathBuf::from(d).join("dedalo")))
        .or_else(|| std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("dedalo")))
        .or_else(|| {
            std::env::var_os("HOME").map(|d| PathBuf::from(d).join(".config").join("dedalo"))
        })?;
    Some(base.join("plugins"))
}

/// Where plugins come from, in search order.
pub fn search_dirs(explicit: &[PathBuf], document_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    // `--plugin` names a package or a folder of packages.
    for path in explicit {
        dirs.push(path.clone());
    }
    if let Some(dir) = document_dir {
        dirs.push(dir.join(".dedalo").join("plugins"));
    }
    dirs.push(PathBuf::from(".dedalo").join("plugins"));
    dirs.extend(user_dir());
    dirs
}

/// Package directories under a search path: the path itself when it holds
/// a manifest, else its subdirectories that do.
fn packages(path: &Path) -> Vec<PathBuf> {
    if path.join(MANIFEST).exists() {
        return vec![path.to_path_buf()];
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(MANIFEST).exists())
        .collect();
    found.sort();
    found
}

pub fn load(dir: &Path) -> Result<LoadedPlugin, String> {
    let files = dedalo_plugin::read_dir(dir)?;
    dedalo_plugin::load_files(files, Some(&wasmi_factory))
        .map_err(|e| format!("the plugin at `{}` cannot be loaded: {e}", dir.display()))
}

/// A registry with the built-ins and every plugin found. The first package
/// with a given ID wins. Packages that fail to load are reported on
/// standard error and skipped.
pub fn registry(
    explicit: &[PathBuf],
    document_dir: Option<&Path>,
) -> (Registry, BTreeMap<String, PathBuf>) {
    let mut reg = Registry::builtin();
    let mut origins = BTreeMap::new();
    for dir in search_dirs(explicit, document_dir) {
        for package in packages(&dir) {
            match load(&package) {
                Ok(plugin) => {
                    let id = plugin.id().to_string();
                    if reg.plugin(&id).is_some() {
                        continue;
                    }
                    if let Err(e) = reg.add_plugin(plugin) {
                        eprintln!("warning: {e}");
                    } else {
                        origins.insert(id, package);
                    }
                }
                Err(e) => eprintln!("warning: {e}"),
            }
        }
        if explicit.iter().any(|p| p == &dir) && !dir.exists() {
            eprintln!(
                "warning: the plugin path `{}` does not exist",
                dir.display()
            );
        }
    }
    (reg, origins)
}

/// Checks the active plugins of a document against its lockfile. Returns
/// the problems; with `update`, rewrites the lockfile instead.
pub fn check_lock(
    document_dir: &Path,
    required: &[String],
    reg: &Registry,
    update: bool,
) -> Result<Vec<String>, String> {
    let lock = Lock::read(document_dir)?;
    if update {
        let mut lock = lock.unwrap_or_default();
        for id in required {
            if let Some(p) = reg.plugin(id) {
                lock.set(p);
            }
        }
        lock.write(document_dir)?;
        return Ok(Vec::new());
    }
    let Some(lock) = lock else {
        return Ok(Vec::new());
    };
    let mut problems = Vec::new();
    for id in required {
        let (Some(p), Some(entry)) = (reg.plugin(id), lock.plugin.iter().find(|e| &e.id == id))
        else {
            continue;
        };
        if entry.sha256 != p.sha256 {
            problems.push(format!(
                "The plugin `{id}` differs from the one recorded in {LOCKFILE}: version {} with hash {}, recorded {} with hash {}. Pass `--update-lock` if the change is intended.",
                p.manifest.plugin.version, p.sha256, entry.version, entry.sha256
            ));
        }
    }
    Ok(problems)
}

/// Copies a package into `./.dedalo/plugins/<id>` and records it in the
/// lockfile of the working directory.
pub fn add(source: &Path) -> Result<LoadedPlugin, String> {
    let text = source.display().to_string();
    if text.starts_with("http://") || text.starts_with("https://") {
        return Err(
            "installing from a URL is not supported yet; download the package and pass its folder"
                .to_string(),
        );
    }
    let plugin = load(source)?;
    let target = PathBuf::from(".dedalo").join("plugins").join(plugin.id());
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|e| format!("cannot replace `{}`: {e}", target.display()))?;
    }
    for (path, bytes) in &plugin.files {
        let dest = target.join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create `{}`: {e}", parent.display()))?;
        }
        std::fs::write(&dest, bytes)
            .map_err(|e| format!("cannot write `{}`: {e}", dest.display()))?;
    }
    let mut lock = Lock::read(Path::new("."))?.unwrap_or_default();
    lock.set(&plugin);
    lock.write(Path::new("."))?;
    Ok(plugin)
}

pub fn remove(id: &str) -> Result<(), String> {
    let target = PathBuf::from(".dedalo").join("plugins").join(id);
    if !target.exists() {
        return Err(format!(
            "no plugin `{id}` is installed in `.dedalo/plugins`"
        ));
    }
    std::fs::remove_dir_all(&target)
        .map_err(|e| format!("cannot remove `{}`: {e}", target.display()))?;
    if let Some(mut lock) = Lock::read(Path::new("."))? {
        lock.plugin.retain(|e| e.id != id);
        lock.write(Path::new("."))?;
    }
    Ok(())
}

/// Writes a starter package.
pub fn scaffold(id: &str, kind: &str, dir: &Path) -> Result<Vec<String>, String> {
    if dir.exists()
        && std::fs::read_dir(dir)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        return Err(format!("`{}` is not empty", dir.display()));
    }
    let mut files: Vec<(String, String)> = Vec::new();
    let meta = format!(
        "[plugin]\nid = \"{id}\"\nversion = \"0.1.0\"\napi = 1\ndedalo = \">=0.2\"\nlicense = \"MIT\"\n"
    );
    match kind {
        "theme" => {
            files.push((
                MANIFEST.into(),
                format!("{meta}\n[theme.default]\nfile = \"themes/default.toml\"\n"),
            ));
            files.push((
                "themes/default.toml".into(),
                dedalo_core::CLASSIC_THEME_TOML.replace("id = \"classic\"", "id = \"default\""),
            ));
        }
        "catalog" => {
            files.push((
                MANIFEST.into(),
                format!("{meta}\n[catalog]\nfile = \"catalog.toml\"\n"),
            ));
            files.push((
                "catalog.toml".into(),
                "# Types are used as `type = PLUGIN.NAME`.\n\n[type.console]\nclass = \"feature\"\ncategory = \"features\"\nlabel = \"Console\"\nblocking = true\nglyph = \"c\"\n\n[type.console.properties]\npowered = { type = \"boolean\" }\n".replace("PLUGIN", id),
            ));
        }
        "wasm" => {
            files.push((
                MANIFEST.into(),
                format!(
                    "{meta}\n[wasm]\nmodule = \"{id}.wasm\"\n\n[[scene_pass]]\nname = \"pass\"\n"
                ),
            ));
            files.push((
                "Cargo.toml".into(),
                format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\ndedalo-pdk = \"0.2\"\n\n[profile.release]\nopt-level = \"s\"\nlto = true\npanic = \"abort\"\n", id.replace('_', "-")),
            ));
            files.push((
                "src/lib.rs".into(),
                "use dedalo_pdk::{Request, Response, json, plugin};\n\n/// Handles every request from the host.\nfn handle(request: Request) -> Response {\n    match (request.point.as_str(), request.name.as_str()) {\n        // A scene pass receives the scene and returns it, changed or not.\n        (\"scene_pass\", \"pass\") => Ok(json!({ \"scene\": request.input[\"scene\"] })),\n        (point, name) => Err(format!(\"no {point} named `{name}`\").into()),\n    }\n}\n\nplugin!(handle);\n".into(),
            ));
            files.push(("README.md".into(), format!("# {id}\n\nBuild with:\n\n    cargo build --release --target wasm32-unknown-unknown\n    cp target/wasm32-unknown-unknown/release/{}.wasm {id}.wasm\n\nThen check it with `dedalo plugin check .`.\n", id.replace('-', "_"))));
        }
        other => {
            return Err(format!(
                "`{other}` is not a kind of plugin; use theme, catalog or wasm"
            ));
        }
    }
    for (path, text) in &files {
        let dest = dir.join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create `{}`: {e}", parent.display()))?;
        }
        std::fs::write(&dest, text)
            .map_err(|e| format!("cannot write `{}`: {e}", dest.display()))?;
    }
    Ok(files.into_iter().map(|(p, _)| p).collect())
}

/// The conformance suite (spec 11.8): manifest and ABI on load, then every
/// declared extension point exercised twice on an example dungeon, which
/// must give identical bytes.
pub fn check(dir: &Path) -> Result<Vec<String>, Vec<String>> {
    let mut passed = Vec::new();
    let plugin = load(dir).map_err(|e| vec![e])?;
    let id = plugin.id().to_string();
    passed.push(format!(
        "manifest: `{id}` version {}, API {}",
        plugin.manifest.plugin.version, plugin.manifest.plugin.api
    ));
    if plugin.runtime.is_some() {
        passed.push("module: ABI 1, imports only from `dedalo`".to_string());
    }
    let manifest = plugin.manifest.clone();
    let mut reg = Registry::builtin();
    reg.add_plugin(plugin).map_err(|e| vec![e])?;
    passed.push(format!(
        "registry: {} theme(s), {} catalog type(s)",
        manifest.theme.len(),
        reg.catalog
            .types
            .keys()
            .filter(|k| k.starts_with(&format!("{id}.")))
            .count()
    ));
    let example = crate::guide::EXAMPLES[0].2.replace("\r\n", "\n");
    let header = format!(
        "dedalo {{\n    version = 1\n\n    plugin \"{id}\" {{\n        version = \"{}\"\n    }}\n}}\n",
        manifest.plugin.version
    );
    let body = example
        .split_once("dungeon \"")
        .map(|(_, rest)| format!("dungeon \"{rest}"))
        .unwrap_or(example.clone());
    let source = format!("{header}\n{body}");
    let compiled = dedalo_core::compile(dedalo_core::Input::Dsl(&source), &reg);
    let mut failures: Vec<String> = compiled
        .diagnostics
        .iter()
        .filter(|d| d.severity == dedalo_core::Severity::Error)
        .map(|d| format!("{}: {}", d.code.as_str(), d.message))
        .collect();
    let mut runs: Vec<(String, dedalo_core::ExportSpec)> = Vec::new();
    for preset in [dedalo_core::Preset::Dm, dedalo_core::Preset::Player] {
        let mut spec = dedalo_core::export_spec(&compiled, None)
            .unwrap_or_else(|_| dedalo_core::ExportSpec::preset(preset));
        spec.preset = preset;
        spec.format = "svg".to_string();
        runs.push((format!("{} map", preset.as_str()), spec.clone()));
        for theme in manifest.theme.keys() {
            let mut themed = spec.clone();
            themed.theme = Some(format!("{id}.{theme}"));
            runs.push((
                format!("{} map with theme {id}.{theme}", preset.as_str()),
                themed,
            ));
        }
        for exporter in &manifest.exporter {
            let mut exported = spec.clone();
            exported.format = format!("{id}.{}", exporter.format);
            runs.push((
                format!("{} export {id}.{}", preset.as_str(), exporter.format),
                exported,
            ));
        }
    }
    for (what, spec) in runs {
        let first = dedalo_core::render(&compiled, &spec, &reg);
        let second = dedalo_core::render(&compiled, &spec, &reg);
        match (first, second) {
            (Ok(a), Ok(b)) if a.bytes == b.bytes => {
                if spec.preset != dedalo_core::Preset::Dm
                    && String::from_utf8_lossy(&a.bytes).contains("data-id")
                {
                    failures.push(format!("{what}: the player output carries element IDs"));
                } else {
                    passed.push(format!("{what}: {} bytes, deterministic", a.bytes.len()));
                }
            }
            (Ok(_), Ok(_)) => failures.push(format!("{what}: two runs gave different bytes")),
            (Err(d), _) | (_, Err(d)) => {
                failures.push(format!("{what}: {}: {}", d.code.as_str(), d.message))
            }
        }
    }
    if failures.is_empty() {
        Ok(passed)
    } else {
        Err(failures)
    }
}
