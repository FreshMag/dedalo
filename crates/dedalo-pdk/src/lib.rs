//! The plugin development kit (spec 11.5).
//!
//! It hides the WebAssembly ABI behind plain Rust: write one function from a
//! [`Request`] to a [`Response`], and the [`plugin!`] macro generates the four
//! exports the host calls. The host's services, a seeded random stream,
//! package assets and text metrics, are ordinary functions here.
//!
//! ```ignore
//! use dedalo_pdk::{Request, Response, json, plugin};
//!
//! fn handle(request: Request) -> Response {
//!     match request.point.as_str() {
//!         "scene_pass" => Ok(json!({ "scene": request.input["scene"] })),
//!         other => Err(format!("unsupported extension point `{other}`").into()),
//!     }
//! }
//!
//! plugin!(handle);
//! ```
//!
//! Build with `cargo build --target wasm32-unknown-unknown --release` and
//! name the `.wasm` file in `dedalo-plugin.toml`. Do not use WASI: a module
//! that imports anything outside the `dedalo` namespace is rejected.

pub use dedalo_scene as scene;
pub use dedalo_scene::{Cmd, Node, Paint, Scene, Stroke, TextAnchor};
pub use serde_json::{self, Value, json};

use serde::Deserialize;

/// One request from the host.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    /// The extension point: `painter`, `scene_pass`, `renderer`,
    /// `exporter`, `validator` or `shape`.
    pub point: String,
    /// The name declared in the manifest, such as the scene pass name.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub input: Value,
    /// The plugin's options: manifest defaults, then the document's values.
    #[serde(default)]
    pub options: Value,
}

impl Request {
    /// A numeric option, or `default`.
    pub fn option_f64(&self, name: &str, default: f64) -> f64 {
        self.options
            .get(name)
            .and_then(Value::as_f64)
            .unwrap_or(default)
    }
}

/// A failure the host reports as `PLUGIN_FAILED`.
#[derive(Debug, Clone)]
pub struct PluginError {
    pub message: String,
    /// The element the failure is about, if any.
    pub element: Option<String>,
}

impl From<String> for PluginError {
    fn from(message: String) -> Self {
        PluginError {
            message,
            element: None,
        }
    }
}

impl From<&str> for PluginError {
    fn from(message: &str) -> Self {
        PluginError {
            message: message.to_string(),
            element: None,
        }
    }
}

/// The `ok` value of a response, or an error.
pub type Response = Result<Value, PluginError>;

#[cfg(target_arch = "wasm32")]
mod host {
    #[link(wasm_import_module = "dedalo")]
    unsafe extern "C" {
        pub fn log(level: i32, ptr: *const u8, len: usize);
        pub fn rand_u64() -> i64;
        pub fn asset(ptr: *const u8, len: usize) -> i64;
        pub fn measure_text(ptr: *const u8, len: usize) -> i64;
    }
}

/// Takes a buffer the host wrote with `dedalo_alloc`, given packed.
#[cfg(target_arch = "wasm32")]
fn take(packed: i64) -> Option<Vec<u8>> {
    if packed == 0 {
        return None;
    }
    let (ptr, len) = (
        (packed as u64 >> 32) as usize,
        (packed as u64 & 0xffff_ffff) as usize,
    );
    // SAFETY: the host wrote `len` bytes into a buffer it got from our
    // `dedalo_alloc`, which allocated it as a boxed slice of that length.
    Some(
        unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr as *mut u8, len)) }
            .into_vec(),
    )
}

/// Writes a message to the host's diagnostic log. Levels: 0 debug to 3 error.
pub fn log(level: i32, text: &str) {
    #[cfg(target_arch = "wasm32")]
    // SAFETY: the pointer and length describe `text`, valid for the call.
    unsafe {
        host::log(level, text.as_ptr(), text.len())
    };
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("[{level}] {text}");
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static RNG: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The next value of the host's seeded random stream. The same request
/// always sees the same stream.
pub fn rand_u64() -> u64 {
    #[cfg(target_arch = "wasm32")]
    // SAFETY: the import takes no arguments.
    return unsafe { host::rand_u64() } as u64;
    #[cfg(not(target_arch = "wasm32"))]
    RNG.with(|r| {
        let mut s = r.get();
        s = s.wrapping_add(0x9e37_79b9_7f4a_7c15);
        r.set(s);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    })
}

/// A random number in `[0, 1)` from the seeded stream.
pub fn rand_f64() -> f64 {
    (rand_u64() >> 11) as f64 / (1u64 << 53) as f64
}

/// A file from the plugin's own package.
pub fn asset(name: &str) -> Option<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    // SAFETY: the pointer and length describe `name`, valid for the call.
    return take(unsafe { host::asset(name.as_ptr(), name.len()) });
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = name;
        None
    }
}

/// The width of a text in the host's bundled fonts, in the units of `size`.
pub fn measure_text(text: &str, size: f64, bold: bool) -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        let request = json!({ "text": text, "size": size, "bold": bold }).to_string();
        // SAFETY: the pointer and length describe `request`, valid for the call.
        let answer = take(unsafe { host::measure_text(request.as_ptr(), request.len()) });
        answer
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["width"].as_f64())
            .unwrap_or(0.0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    dedalo_scene::font::measure(text, size, bold)
}

/// Handles one raw request: used by the code [`plugin!`] generates.
pub fn dispatch(request: &[u8], handler: fn(Request) -> Response) -> Vec<u8> {
    let response = match serde_json::from_slice::<Request>(request) {
        Err(e) => json!({ "error": { "message": format!("invalid request: {e}") } }),
        Ok(request) => match handler(request) {
            Ok(ok) => json!({ "ok": ok }),
            Err(e) => json!({ "error": { "message": e.message, "element": e.element } }),
        },
    };
    response.to_string().into_bytes()
}

/// Allocates a buffer for the host: used by the code [`plugin!`] generates.
pub fn alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}

/// Frees a buffer from [`alloc`].
///
/// # Safety
/// `ptr` and `len` must describe exactly one buffer from [`alloc`].
pub unsafe fn free(ptr: *mut u8, len: usize) {
    // SAFETY: guaranteed by the caller.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
}

/// Hands a response to the host as a pointer and length packed in 64 bits.
pub fn pack(bytes: Vec<u8>) -> i64 {
    let len = bytes.len() as u64;
    let ptr = Box::into_raw(bytes.into_boxed_slice()) as *mut u8 as usize as u64;
    ((ptr << 32) | len) as i64
}

/// Generates the four exports of Dedalo plugin ABI version 1 around a
/// handler function `fn(Request) -> Response`.
#[macro_export]
macro_rules! plugin {
    ($handler:path) => {
        /// The plugin's handler, callable directly in native tests.
        #[cfg(not(target_arch = "wasm32"))]
        pub fn dedalo_handle(request: &[u8]) -> Vec<u8> {
            $crate::dispatch(request, $handler)
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn dedalo_abi() -> i32 {
            1
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn dedalo_alloc(len: i32) -> i32 {
            $crate::alloc(len as usize) as usize as i32
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn dedalo_free(ptr: i32, len: i32) {
            // SAFETY: the host frees only buffers it got from `dedalo_alloc` or `dedalo_call`.
            unsafe { $crate::free(ptr as usize as *mut u8, len as usize) }
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn dedalo_call(ptr: i32, len: i32) -> i64 {
            // SAFETY: the host wrote `len` bytes at `ptr`, in a buffer from `dedalo_alloc`.
            let request =
                unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len as usize) };
            $crate::pack($crate::dispatch(request, $handler))
        }
    };
}

/// Helpers for scene geometry.
pub mod geometry {
    use super::Cmd;

    /// The end point of a path command, when it has one.
    pub fn end(cmd: &Cmd) -> Option<(f64, f64)> {
        match *cmd {
            Cmd::Move(x, y) | Cmd::Line(x, y) => Some((x, y)),
            Cmd::Arc { x, y, .. } => Some((x, y)),
            Cmd::Cubic(_, _, _, _, x, y) => Some((x, y)),
            Cmd::Close => None,
        }
    }
}
