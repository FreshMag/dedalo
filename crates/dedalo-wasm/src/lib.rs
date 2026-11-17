//! Browser bindings: the core compiled to WebAssembly (spec section 14).
//!
//! The page and the module exchange JSON messages through one synchronous
//! call, the same shape the plugin ABI uses (spec 11.5): the host allocates a
//! buffer, writes a request, calls `dedalo_call`, and reads the response.
//! This host holds no map logic; every message is answered by `dedalo-core`.
//!
//! Milestone M0 uses these raw exports with a few lines of hand-written
//! JavaScript, so the page needs no build tooling. The `wasm-bindgen`
//! bindings and TypeScript types named in spec section 4 come with the web
//! editor in milestone M4 and will wrap this same `handle` function.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use dedalo_core::model::{Diagnostic, Preset, Report};
use dedalo_core::{Compiled, Input, Registry};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Request {
    /// Version of the core in this module.
    Version,
    /// Run parse, lower and resolve.
    Compile { source: String },
    /// Compile and render one export.
    Render {
        source: String,
        #[serde(default)]
        export: Option<String>,
        #[serde(default)]
        preset: Option<String>,
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        rotation: Option<i64>,
        /// One level of a multi-level dungeon.
        #[serde(default)]
        level: Option<String>,
        /// The language of default labels, for a document that sets none.
        #[serde(default)]
        language: Option<String>,
        /// Temporary category toggles, such as `{ "grid": false }`.
        #[serde(default)]
        toggles: std::collections::BTreeMap<String, bool>,
        /// A PNG rendering from the export module, base64, for plugin
        /// exporters that need one.
        #[serde(default)]
        host_png: Option<String>,
        /// Render whatever resolved even when there are errors.
        #[serde(default)]
        force: bool,
    },
    /// Format DSL text canonically.
    Format { source: String },
    /// Switch between DSL and JSON.
    Convert { source: String, to: String },
    /// Every output the golden corpus records for this input.
    Golden { source: String },
    /// Completions at an offset, counted in UTF-16 code units as in JavaScript.
    Complete { source: String, offset: usize },
    /// Hover help at an offset in UTF-16 code units.
    Hover { source: String, offset: usize },
    /// The line and column of the block declaring an ID.
    Locate { source: String, id: String },
    /// Spaces, elements, connections and notes, with their lines.
    Outline { source: String },
    /// Load a plugin package: its files, base64-encoded, by path.
    LoadPlugin { files: BTreeMap<String, String> },
    /// Open a zipped package (`.dedalopkg`, base64) without loading it:
    /// its files, and the ID, version and hash to show before asking.
    UnpackPackage { package: String },
    /// The plugins loaded.
    Plugins,
    /// Apply edit operations, such as a diagnostic's fix.
    Edit {
        source: String,
        ops: Vec<dedalo_core::model::ops::Op>,
        #[serde(default)]
        force: bool,
    },
}

/// UTF-16 offset to byte offset.
fn to_byte(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// Byte offset to UTF-16 offset.
fn to_utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

thread_local! {
    /// The registry: the built-ins and the plugins the page has loaded.
    static REGISTRY: RefCell<Rc<Registry>> = RefCell::new(Rc::new(Registry::builtin()));
}

fn registry() -> Rc<Registry> {
    REGISTRY.with(|r| r.borrow().clone())
}

/// Runs a plugin through the page: the worker instantiates the plugin's
/// module with the browser's engine and answers synchronously.
struct BrowserRuntime {
    id: String,
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "dedalo_host")]
unsafe extern "C" {
    fn plugin_call(
        id_ptr: *const u8,
        id_len: usize,
        req_ptr: *const u8,
        req_len: usize,
        seed: i64,
    ) -> i64;
}

impl dedalo_plugin::PluginRuntime for BrowserRuntime {
    fn call(&self, request: &[u8], seed: u64) -> Result<Vec<u8>, dedalo_plugin::CallError> {
        #[cfg(target_arch = "wasm32")]
        {
            // SAFETY: both buffers are valid for the call; the host returns a
            // buffer it got from `dedalo_alloc`, packed, or 0 on failure.
            let packed = unsafe {
                plugin_call(
                    self.id.as_ptr(),
                    self.id.len(),
                    request.as_ptr(),
                    request.len(),
                    seed as i64,
                )
            } as u64;
            if packed == 0 {
                return Err(dedalo_plugin::CallError::Timeout(format!(
                    "the plugin `{}` did not answer",
                    self.id
                )));
            }
            let (ptr, len) = (
                (packed >> 32) as usize as *mut u8,
                (packed & 0xffff_ffff) as usize,
            );
            // SAFETY: the host allocated this buffer with `dedalo_alloc` and wrote `len` bytes.
            let bytes = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) };
            Ok(bytes.into_vec())
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (request, seed);
            Err(dedalo_plugin::CallError::Failed(format!(
                "the plugin `{}` needs a browser host",
                self.id
            )))
        }
    }
}

/// Opens a zipped package and describes it. Nothing is registered.
fn unpack_package(package: &str) -> Result<serde_json::Value, String> {
    let bytes = dedalo_plugin::base64::decode(package).ok_or("the package is not valid base64")?;
    let files = dedalo_plugin::package::unpack(&bytes)?;
    let manifest = files
        .get(dedalo_plugin::MANIFEST)
        .ok_or("the package has no dedalo-plugin.toml")?;
    let manifest = dedalo_plugin::Manifest::parse(&String::from_utf8_lossy(manifest))?;
    let encoded: BTreeMap<&String, String> = files
        .iter()
        .map(|(path, bytes)| (path, dedalo_plugin::base64::encode(bytes)))
        .collect();
    Ok(json!({
        "ok": true,
        "plugin": { "id": manifest.plugin.id, "version": manifest.plugin.version, "sha256": dedalo_plugin::package_hash(&files) },
        "files": encoded,
    }))
}

/// Loads a plugin package given as files, base64-encoded.
fn load_plugin(files: BTreeMap<String, String>) -> Result<serde_json::Value, String> {
    let mut decoded = BTreeMap::new();
    for (path, data) in files {
        let bytes = dedalo_plugin::base64::decode(&data)
            .ok_or_else(|| format!("`{path}` is not valid base64"))?;
        decoded.insert(path, bytes);
    }
    let id_holder = RefCell::new(String::new());
    let factory = |_: &[u8],
                   _: BTreeMap<String, Vec<u8>>|
     -> Result<Arc<dyn dedalo_plugin::PluginRuntime>, String> {
        Ok(Arc::new(BrowserRuntime {
            id: id_holder.borrow().clone(),
        }))
    };
    // The runtime needs the ID before the manifest is parsed by the loader.
    let manifest = decoded
        .get(dedalo_plugin::MANIFEST)
        .ok_or("the package has no dedalo-plugin.toml")?;
    *id_holder.borrow_mut() = dedalo_plugin::Manifest::parse(&String::from_utf8_lossy(manifest))?
        .plugin
        .id;
    let plugin = dedalo_plugin::load_files(decoded, Some(&factory))?;
    let info = json!({ "id": plugin.id(), "version": plugin.manifest.plugin.version, "sha256": plugin.sha256 });
    REGISTRY.with(|r| {
        let mut next = Registry::builtin();
        let old = r.borrow().clone();
        for p in &old.plugins {
            if p.id() != plugin.id() {
                // Re-adding keeps the order the page loaded them in.
                let files = p.files.clone();
                let id = p.id().to_string();
                let again = dedalo_plugin::load_files(
                    files,
                    Some(&|_: &[u8], _| {
                        Ok(Arc::new(BrowserRuntime { id: id.clone() })
                            as Arc<dyn dedalo_plugin::PluginRuntime>)
                    }),
                )?;
                next.add_plugin(again)?;
            }
        }
        next.add_plugin(plugin)?;
        *r.borrow_mut() = Rc::new(next);
        Ok::<(), String>(())
    })?;
    Ok(info)
}

fn compile(source: &str) -> Compiled {
    let input = if source.trim_start().starts_with('{') {
        Input::Json(source)
    } else {
        Input::Dsl(source)
    };
    dedalo_core::compile(input, &registry())
}

fn failure(diagnostic: Diagnostic) -> Value {
    json!({ "ok": false, "report": Report::new(vec![diagnostic]) })
}

/// Answers one request. Pure: the same request always gives the same answer.
pub fn handle(request: &str) -> String {
    let response = match serde_json::from_str::<Request>(request) {
        Err(e) => json!({ "ok": false, "error": format!("invalid request: {e}") }),
        Ok(Request::Version) => json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }),
        Ok(Request::Compile { source }) => {
            let compiled = compile(&source);
            let exports: Vec<&str> = compiled
                .document
                .iter()
                .flat_map(|d| d.dungeon.exports.iter().map(|e| e.name.as_str()))
                .collect();
            let levels: Vec<Value> = compiled
                .document
                .iter()
                .flat_map(|d| &d.dungeon.levels)
                .map(|l| json!({ "id": l.id, "name": l.name }))
                .collect();
            json!({ "ok": !compiled.has_errors(), "report": compiled.report(), "exports": exports, "levels": levels })
        }
        Ok(Request::Render {
            source,
            export,
            preset,
            format,
            rotation,
            level,
            language,
            toggles,
            host_png,
            force,
        }) => {
            let compiled = compile(&source);
            let report = compiled.report();
            if report.has_errors() && !force {
                return json!({ "ok": false, "report": report }).to_string();
            }
            let mut spec = match dedalo_core::export_spec(&compiled, export.as_deref()) {
                Ok(spec) => spec,
                Err(d) => return failure(d).to_string(),
            };
            if let Some(preset) = preset.as_deref().and_then(Preset::parse) {
                spec.preset = preset;
            }
            if let Some(format) = format {
                spec.format = format;
            }
            if let Some(rotation) = rotation.filter(|r| [0, 90, 180, 270].contains(r)) {
                spec.rotation = rotation;
            }
            // The page's language applies where the document chose none.
            let chosen = compiled.document.as_ref().is_some_and(|doc| {
                let d = &doc.dungeon;
                let of_export = d
                    .exports
                    .iter()
                    .find(|e| e.name == spec.name)
                    .and_then(|e| e.render.as_ref())
                    .and_then(|r| r.language);
                of_export
                    .or(d.render.as_ref().and_then(|r| r.language))
                    .is_some()
            });
            if let Some(language) = language
                .as_deref()
                .and_then(dedalo_core::model::Language::parse)
                .filter(|_| !chosen)
            {
                spec.language = language;
            }
            if level.as_ref().is_some_and(|l| !l.is_empty()) {
                // A level the dungeon does not have leaves the map whole.
                spec.level = level;
            }
            spec.toggles.extend(toggles);
            spec.host_png = host_png.as_deref().and_then(dedalo_plugin::base64::decode);
            // The levels, so a host can offer them without compiling again.
            let levels: Vec<Value> = compiled
                .document
                .iter()
                .flat_map(|d| &d.dungeon.levels)
                .map(|l| json!({ "id": l.id, "name": l.name }))
                .collect();
            match dedalo_core::render(&compiled, &spec, &registry()) {
                Ok(rendered) => json!({
                    "ok": !report.has_errors(),
                    "report": report,
                    "output": String::from_utf8_lossy(&rendered.bytes),
                    "media_type": rendered.media_type,
                    "extension": rendered.extension,
                    "levels": levels,
                    "export": spec.name,
                    "preset": spec.preset.as_str(),
                }),
                Err(d) => json!({ "ok": false, "report": report, "error": d.message }),
            }
        }
        Ok(Request::Format { source }) => match dedalo_core::format(&source, &registry()) {
            Ok(text) => json!({ "ok": true, "text": text }),
            Err(diagnostics) => json!({ "ok": false, "report": Report::new(diagnostics) }),
        },
        Ok(Request::Convert { source, to }) => {
            let compiled = compile(&source);
            match (&compiled.document, to.as_str()) {
                (Some(doc), "json") => json!({ "ok": true, "text": dedalo_core::to_json(doc) }),
                (Some(doc), "dsl") => {
                    json!({ "ok": true, "text": dedalo_core::to_dsl(doc, &registry()) })
                }
                (Some(_), other) => {
                    json!({ "ok": false, "error": format!("`{other}` is not a format; use json or dsl") })
                }
                (None, _) => json!({ "ok": false, "report": compiled.report() }),
            }
        }
        Ok(Request::UnpackPackage { package }) => match unpack_package(&package) {
            Ok(answer) => answer,
            Err(e) => json!({ "ok": false, "error": e }),
        },
        Ok(Request::LoadPlugin { files }) => match load_plugin(files) {
            Ok(info) => json!({ "ok": true, "plugin": info }),
            Err(e) => json!({ "ok": false, "error": e }),
        },
        Ok(Request::Plugins) => {
            let reg = registry();
            let list: Vec<serde_json::Value> = reg.plugins.iter().map(|p| json!({ "id": p.id(), "version": p.manifest.plugin.version, "sha256": p.sha256 })).collect();
            json!({ "ok": true, "plugins": list, "themes": reg.themes.keys().collect::<Vec<_>>() })
        }
        Ok(Request::Complete { source, offset }) => {
            let c = dedalo_core::assist::complete(&source, to_byte(&source, offset), &registry());
            json!({ "ok": true, "from": to_utf16(&source, c.from), "items": c.items })
        }
        Ok(Request::Hover { source, offset }) => {
            match dedalo_core::assist::hover(&source, to_byte(&source, offset), &registry()) {
                Some(h) => {
                    json!({ "ok": true, "from": to_utf16(&source, h.from), "to": to_utf16(&source, h.to), "markdown": h.markdown })
                }
                None => json!({ "ok": false }),
            }
        }
        Ok(Request::Locate { source, id }) => match dedalo_core::assist::locate(&source, &id) {
            Some([line, column]) => json!({ "ok": true, "line": line, "column": column }),
            None => json!({ "ok": false }),
        },
        Ok(Request::Outline { source }) => {
            json!({ "ok": true, "items": dedalo_core::assist::outline(&source, &registry()) })
        }
        Ok(Request::Edit { source, ops, force }) => {
            let batch = dedalo_core::model::ops::Batch {
                base_hash: None,
                ops,
            };
            let result = dedalo_core::edit(&source, &batch, &registry(), force, "dungeon.dedalo");
            let mut value = serde_json::to_value(&result).unwrap_or_default();
            value["ok"] = json!(result.applied);
            value["source"] = json!(result.source);
            value
        }
        Ok(Request::Golden { source }) => {
            let compiled = compile(&source);
            json!({ "ok": true, "files": dedalo_core::golden_outputs(&compiled, &registry(), false) })
        }
    };
    response.to_string()
}

/// Gives the host a buffer of `len` bytes inside the module's memory.
///
/// The raw exports pack a pointer into 32 bits, so they exist only in the
/// WebAssembly build.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn dedalo_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}

/// Text metrics for plugins: answers `dedalo.measure_text` for the page.
///
/// # Safety
/// `ptr` and `len` must describe a readable buffer.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dedalo_measure(ptr: *const u8, len: usize) -> u64 {
    // SAFETY: the caller wrote `len` bytes at `ptr`.
    let request = unsafe { std::slice::from_raw_parts(ptr, len) };
    let out = dedalo_plugin::runtime::measure_text(request).into_boxed_slice();
    let out_len = out.len() as u64;
    let out_ptr = Box::into_raw(out) as *mut u8 as usize as u64;
    (out_ptr << 32) | out_len
}

/// Takes back a buffer obtained from `dedalo_alloc` or `dedalo_call`.
///
/// # Safety
/// `ptr` and `len` must describe exactly one such buffer, not yet freed.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dedalo_free(ptr: *mut u8, len: usize) {
    // SAFETY: the caller passes a buffer this module allocated as a boxed
    // slice of exactly `len` bytes.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
}

/// Handles one JSON request. Returns the response buffer as a pointer in the
/// high 32 bits and a length in the low 32 bits; free it with `dedalo_free`.
///
/// # Safety
/// `ptr` and `len` must describe a readable buffer of UTF-8 text.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dedalo_call(ptr: *const u8, len: usize) -> u64 {
    // SAFETY: the caller wrote `len` bytes at `ptr` in a buffer from `dedalo_alloc`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let response = match std::str::from_utf8(bytes) {
        Ok(request) => handle(request),
        Err(_) => json!({ "ok": false, "error": "the request is not UTF-8" }).to_string(),
    };
    let out = response.into_bytes().into_boxed_slice();
    let out_len = out.len() as u64;
    let out_ptr = Box::into_raw(out) as *mut u8 as usize as u64;
    (out_ptr << 32) | out_len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_render_request_returns_the_map() {
        let request = json!({
            "cmd": "render",
            "source": "dungeon \"d\" {
 room \"a\" { shape = square(3) }
}
",
            "format": "txt",
        });
        let value: Value = serde_json::from_str(&handle(&request.to_string())).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(
            value["output"],
            "#####
#...#
#...#
#...#
#####
"
        );
        assert_eq!(handle(&request.to_string()), handle(&request.to_string()));
    }

    #[test]
    fn errors_are_reported_not_thrown() {
        let value: Value = serde_json::from_str(&handle(
            "{\"cmd\":\"compile\",\"source\":\"dungeon \\\"d\\\" { room \\\"a\\\" { } }\"}",
        ))
        .unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["report"]["diagnostics"][0]["code"], "INVALID_SHAPE");
        let value: Value = serde_json::from_str(&handle("not json")).unwrap();
        assert_eq!(value["ok"], false);
    }
}
