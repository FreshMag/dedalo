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

use std::sync::OnceLock;

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
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::builtin)
}

fn compile(source: &str) -> Compiled {
    let input = if source.trim_start().starts_with('{') {
        Input::Json(source)
    } else {
        Input::Dsl(source)
    };
    dedalo_core::compile(input, registry())
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
            json!({ "ok": !compiled.has_errors(), "report": compiled.report(), "exports": exports })
        }
        Ok(Request::Render {
            source,
            export,
            preset,
            format,
            rotation,
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
            match dedalo_core::render(&compiled, &spec, registry()) {
                Ok(rendered) => json!({
                    "ok": !report.has_errors(),
                    "report": report,
                    "output": String::from_utf8_lossy(&rendered.bytes),
                    "media_type": rendered.media_type,
                    "extension": rendered.extension,
                }),
                Err(d) => json!({ "ok": false, "report": report, "error": d.message }),
            }
        }
        Ok(Request::Format { source }) => match dedalo_core::format(&source, registry()) {
            Ok(text) => json!({ "ok": true, "text": text }),
            Err(diagnostics) => json!({ "ok": false, "report": Report::new(diagnostics) }),
        },
        Ok(Request::Convert { source, to }) => {
            let compiled = compile(&source);
            match (&compiled.document, to.as_str()) {
                (Some(doc), "json") => json!({ "ok": true, "text": dedalo_core::to_json(doc) }),
                (Some(doc), "dsl") => {
                    json!({ "ok": true, "text": dedalo_core::to_dsl(doc, registry()) })
                }
                (Some(_), other) => {
                    json!({ "ok": false, "error": format!("`{other}` is not a format; use json or dsl") })
                }
                (None, _) => json!({ "ok": false, "report": compiled.report() }),
            }
        }
        Ok(Request::Golden { source }) => {
            let compiled = compile(&source);
            json!({ "ok": true, "files": dedalo_core::golden_outputs(&compiled, registry()) })
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
