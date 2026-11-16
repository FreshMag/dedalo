//! The browser's second WebAssembly module (spec 14.2): PNG and PDF.
//!
//! The `core` module covers parsing through SVG and loads at start. This
//! module holds resvg, krilla and the fonts, and loads on the first raster
//! or PDF export. The boundary between the two is an SVG string in and bytes
//! out. It speaks the same three raw exports as the core module; binary
//! output comes back base64-encoded inside the JSON response.

use dedalo_model::{Orientation, Page};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Request {
    Png {
        svg: String,
    },
    Pdf {
        svg: String,
        #[serde(default)]
        page: Option<String>,
        #[serde(default)]
        orientation: Option<String>,
        /// Pixels per cell of the SVG, to print one cell per inch.
        #[serde(default)]
        tabletop_cell_px: Option<u32>,
    },
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Answers one request.
pub fn handle(request: &str) -> String {
    let result = match serde_json::from_str::<Request>(request) {
        Err(e) => Err(format!("invalid request: {e}")),
        Ok(Request::Png { svg }) => dedalo_export::png(&svg).map(|b| (b, "image/png")),
        Ok(Request::Pdf {
            svg,
            page,
            orientation,
            tabletop_cell_px,
        }) => {
            let page = page.as_deref().and_then(Page::parse).unwrap_or(Page::Fit);
            let orientation = orientation.as_deref().and_then(Orientation::parse);
            dedalo_export::pdf(&svg, page, orientation, tabletop_cell_px)
                .map(|b| (b, "application/pdf"))
        }
    };
    let response: Value = match result {
        Ok((bytes, media_type)) => {
            json!({ "ok": true, "media_type": media_type, "base64": base64(&bytes) })
        }
        Err(error) => json!({ "ok": false, "error": error }),
    };
    response.to_string()
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn dedalo_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}

/// # Safety
/// `ptr` and `len` must describe exactly one buffer from this module, not yet freed.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dedalo_free(ptr: *mut u8, len: usize) {
    // SAFETY: the caller passes a buffer this module allocated with this length.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
}

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
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn an_svg_becomes_a_png() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2 2" width="8" height="8"><path d="M0 0L2 0L2 2Z" fill="#111111"/></svg>"##;
        let value: Value =
            serde_json::from_str(&handle(&json!({ "cmd": "png", "svg": svg }).to_string()))
                .unwrap();
        assert_eq!(value["ok"], true);
        assert!(value["base64"].as_str().unwrap().starts_with("iVBORw0KGgo"));
    }
}
