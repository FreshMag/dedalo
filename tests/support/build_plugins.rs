// Builds the reference WebAssembly plugins into their package folders, once
// per test process. Shared by the test suites through `include!`.

/// The repository's `plugins/` folder, with every package ready to load.
#[allow(dead_code)]
fn reference_plugins() -> std::path::PathBuf {
    static BUILT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    BUILT
        .get_or_init(|| {
            let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
            // A separate target folder: the outer cargo may still hold the lock on its own.
            let target = root.join("target").join("plugin-build");
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
            let status = std::process::Command::new(cargo)
                .current_dir(&root)
                .args(["build", "--quiet", "-p", "dedalo-plugin-hand-drawn", "-p", "dedalo-plugin-uvtt", "--target", "wasm32-unknown-unknown", "--profile", "wasm", "--target-dir"])
                .arg(&target)
                .status()
                .expect("cargo runs");
            assert!(status.success(), "building the reference plugins failed");
            for (krate, id) in [("dedalo_plugin_hand_drawn", "hand_drawn"), ("dedalo_plugin_uvtt", "uvtt")] {
                let from = target.join("wasm32-unknown-unknown").join("wasm").join(format!("{krate}.wasm"));
                let to = root.join("plugins").join(id).join(format!("{id}.wasm"));
                let fresh = std::fs::read(&from).expect("the plugin was built");
                if std::fs::read(&to).ok().as_deref() != Some(fresh.as_slice()) {
                    std::fs::write(&to, fresh).expect("copying the plugin");
                }
            }
            root.join("plugins")
        })
        .clone()
}
