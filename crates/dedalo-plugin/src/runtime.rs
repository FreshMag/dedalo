//! Running plugin code (spec 11.5).
//!
//! A plugin is core WebAssembly with four exports: `dedalo_abi`,
//! `dedalo_alloc`, `dedalo_free` and `dedalo_call`. Requests and responses
//! are JSON. The only imports are in the `dedalo` namespace, so a plugin has
//! no files, network, clock or environment. In the binary the interpreter
//! is wasmi, with a fuel budget and a memory cap per call; each call runs in
//! a fresh instance, so the same request always gives the same response.

/// Why a call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// The fuel or memory budget ran out.
    Timeout(String),
    /// Any other failure: a trap, a bad response, a missing export.
    Failed(String),
}

/// Something that answers plugin requests: wasmi in the binary, the
/// browser's own engine in the web editor.
pub trait PluginRuntime: Send + Sync {
    /// Handles one request. `seed` starts the random stream the plugin reads
    /// through `dedalo.rand_u64`.
    fn call(&self, request: &[u8], seed: u64) -> Result<Vec<u8>, CallError>;
}

/// The next value of the seeded random stream (SplitMix64).
pub fn next_random(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Answers `dedalo.measure_text`: `{ "text", "size", "bold" }` to `{ "width" }`.
pub fn measure_text(request: &[u8]) -> Vec<u8> {
    let value: serde_json::Value = serde_json::from_slice(request).unwrap_or_default();
    let text = value
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let size = value.get("size").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let bold = value.get("bold").and_then(|v| v.as_bool()).unwrap_or(false);
    serde_json::json!({ "width": dedalo_scene::font::measure(text, size, bold) })
        .to_string()
        .into_bytes()
}

#[cfg(feature = "wasmi")]
pub use native::WasmiRuntime;

#[cfg(feature = "wasmi")]
mod native {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use wasmi::{
        Caller, Config, Engine, Extern, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
    };

    use super::{CallError, PluginRuntime, measure_text, next_random};

    /// Memory cap per call (spec 11.5).
    pub const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
    /// Fuel per call: roughly one unit per instruction.
    pub const FUEL: u64 = 4_000_000_000;

    struct Host {
        limits: StoreLimits,
        rng: u64,
        assets: Arc<BTreeMap<String, Vec<u8>>>,
        logs: Vec<String>,
    }

    pub struct WasmiRuntime {
        engine: Engine,
        module: Module,
        assets: Arc<BTreeMap<String, Vec<u8>>>,
        fuel: u64,
    }

    fn read(caller: &Caller<'_, Host>, ptr: i32, len: i32) -> Option<Vec<u8>> {
        let memory = caller.get_export("memory").and_then(Extern::into_memory)?;
        let data = memory.data(caller);
        let (start, len) = (usize::try_from(ptr).ok()?, usize::try_from(len).ok()?);
        data.get(start..start.checked_add(len)?).map(<[u8]>::to_vec)
    }

    /// Copies bytes into a buffer the plugin allocates; returns ptr and len packed.
    fn give(caller: &mut Caller<'_, Host>, bytes: &[u8]) -> i64 {
        let Some(alloc) = caller
            .get_export("dedalo_alloc")
            .and_then(Extern::into_func)
        else {
            return 0;
        };
        let Ok(alloc) = alloc.typed::<i32, i32>(&*caller) else {
            return 0;
        };
        let Ok(ptr) = alloc.call(&mut *caller, bytes.len() as i32) else {
            return 0;
        };
        let Some(memory) = caller.get_export("memory").and_then(Extern::into_memory) else {
            return 0;
        };
        if memory.write(&mut *caller, ptr as usize, bytes).is_err() {
            return 0;
        }
        ((ptr as u32 as i64) << 32) | bytes.len() as i64
    }

    impl WasmiRuntime {
        /// Compiles a plugin module and checks its imports and ABI version.
        pub fn load(
            bytes: &[u8],
            assets: BTreeMap<String, Vec<u8>>,
        ) -> Result<WasmiRuntime, String> {
            let mut config = Config::default();
            config.consume_fuel(true);
            let engine = Engine::new(&config);
            let module = Module::new(&engine, bytes)
                .map_err(|e| format!("the module is not valid WebAssembly: {e}"))?;
            for import in module.imports() {
                if import.module() != "dedalo" {
                    return Err(format!(
                        "the module imports `{}.{}`; plugins may import only from the `dedalo` namespace (no WASI, files, network or clock)",
                        import.module(),
                        import.name()
                    ));
                }
                if !matches!(import.name(), "log" | "rand_u64" | "asset" | "measure_text") {
                    return Err(format!(
                        "the module imports `dedalo.{}`, which this host does not provide",
                        import.name()
                    ));
                }
            }
            for export in [
                "memory",
                "dedalo_abi",
                "dedalo_alloc",
                "dedalo_free",
                "dedalo_call",
            ] {
                if module.get_export(export).is_none() {
                    return Err(format!("the module does not export `{export}`"));
                }
            }
            let runtime = WasmiRuntime {
                engine,
                module,
                assets: Arc::new(assets),
                fuel: FUEL,
            };
            let mut store = runtime.store(0);
            let instance = runtime.instantiate(&mut store).map_err(|e| match e {
                CallError::Timeout(m) | CallError::Failed(m) => m,
            })?;
            let abi = instance
                .get_typed_func::<(), i32>(&store, "dedalo_abi")
                .map_err(|e| e.to_string())?;
            match abi.call(&mut store, ()) {
                Ok(1) => Ok(runtime),
                Ok(v) => Err(format!(
                    "the module speaks plugin ABI {v}; this host speaks ABI 1"
                )),
                Err(e) => Err(format!("`dedalo_abi` failed: {e}")),
            }
        }

        /// Lowers the fuel budget, for tests of runaway plugins.
        pub fn with_fuel(mut self, fuel: u64) -> WasmiRuntime {
            self.fuel = fuel;
            self
        }

        fn store(&self, seed: u64) -> Store<Host> {
            let limits = StoreLimitsBuilder::new()
                .memory_size(MEMORY_LIMIT)
                .instances(1)
                .trap_on_grow_failure(true)
                .build();
            let mut store = Store::new(
                &self.engine,
                Host {
                    limits,
                    rng: seed,
                    assets: self.assets.clone(),
                    logs: Vec::new(),
                },
            );
            store.limiter(|host| &mut host.limits);
            let _ = store.set_fuel(self.fuel);
            store
        }

        fn instantiate(&self, store: &mut Store<Host>) -> Result<wasmi::Instance, CallError> {
            let mut linker = <Linker<Host>>::new(&self.engine);
            let wrap = |e: wasmi::errors::LinkerError| CallError::Failed(e.to_string());
            linker
                .func_wrap(
                    "dedalo",
                    "log",
                    |caller: Caller<'_, Host>, level: i32, ptr: i32, len: i32| {
                        let text = read(&caller, ptr, len)
                            .map(|b| String::from_utf8_lossy(&b).into_owned())
                            .unwrap_or_default();
                        let mut caller = caller;
                        if caller.data().logs.len() < 1000 {
                            caller.data_mut().logs.push(format!("[{level}] {text}"));
                        }
                    },
                )
                .map_err(wrap)?;
            linker
                .func_wrap(
                    "dedalo",
                    "rand_u64",
                    |mut caller: Caller<'_, Host>| -> i64 {
                        next_random(&mut caller.data_mut().rng) as i64
                    },
                )
                .map_err(wrap)?;
            linker
                .func_wrap(
                    "dedalo",
                    "asset",
                    |mut caller: Caller<'_, Host>, ptr: i32, len: i32| -> i64 {
                        let Some(name) = read(&caller, ptr, len)
                            .map(|b| String::from_utf8_lossy(&b).into_owned())
                        else {
                            return 0;
                        };
                        let Some(bytes) = caller.data().assets.get(&name).cloned() else {
                            return 0;
                        };
                        give(&mut caller, &bytes)
                    },
                )
                .map_err(wrap)?;
            linker
                .func_wrap(
                    "dedalo",
                    "measure_text",
                    |mut caller: Caller<'_, Host>, ptr: i32, len: i32| -> i64 {
                        let Some(request) = read(&caller, ptr, len) else {
                            return 0;
                        };
                        give(&mut caller, &measure_text(&request))
                    },
                )
                .map_err(wrap)?;
            linker
                .instantiate_and_start(&mut *store, &self.module)
                .map_err(classify)
        }
    }

    fn classify(e: wasmi::Error) -> CallError {
        let text = e.to_string();
        let lower = text.to_lowercase();
        if lower.contains("fuel") || lower.contains("limit") || lower.contains("grow") {
            CallError::Timeout(text)
        } else {
            CallError::Failed(text)
        }
    }

    /// Stack of the thread a call runs on: the interpreter and the JSON of
    /// large scenes need more than a default thread has.
    const STACK: usize = 32 * 1024 * 1024;

    impl PluginRuntime for WasmiRuntime {
        fn call(&self, request: &[u8], seed: u64) -> Result<Vec<u8>, CallError> {
            std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .stack_size(STACK)
                    .spawn_scoped(scope, || self.call_here(request, seed))
                    .map_err(|e| CallError::Failed(format!("cannot start the plugin thread: {e}")))?
                    .join()
                    .unwrap_or_else(|_| {
                        Err(CallError::Failed("the plugin host panicked".to_string()))
                    })
            })
        }
    }

    impl WasmiRuntime {
        fn call_here(&self, request: &[u8], seed: u64) -> Result<Vec<u8>, CallError> {
            let mut store = self.store(seed);
            let instance = self.instantiate(&mut store)?;
            let memory = instance
                .get_memory(&store, "memory")
                .ok_or_else(|| CallError::Failed("no memory export".to_string()))?;
            let alloc = instance
                .get_typed_func::<i32, i32>(&store, "dedalo_alloc")
                .map_err(|e| CallError::Failed(e.to_string()))?;
            let call = instance
                .get_typed_func::<(i32, i32), i64>(&store, "dedalo_call")
                .map_err(|e| CallError::Failed(e.to_string()))?;
            let ptr = alloc
                .call(&mut store, request.len() as i32)
                .map_err(classify)?;
            memory
                .write(&mut store, ptr as usize, request)
                .map_err(|e| CallError::Failed(e.to_string()))?;
            let packed = call
                .call(&mut store, (ptr, request.len() as i32))
                .map_err(classify)? as u64;
            let (out_ptr, out_len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
            let data = memory.data(&store);
            let end = out_ptr
                .checked_add(out_len)
                .filter(|e| *e <= data.len())
                .ok_or_else(|| {
                    CallError::Failed("the response lies outside the plugin's memory".to_string())
                })?;
            Ok(data[out_ptr..end].to_vec())
        }
    }
}
