//! Hostile plugin fixtures (spec 11.7 and 15.1): each is contained.

use std::collections::BTreeMap;

use dedalo_plugin::runtime::WasmiRuntime;
use dedalo_plugin::{CallError, PluginRuntime};

/// A module with the ABI exports around a custom `dedalo_call` body.
fn module(imports: &str, call_body: &str) -> Vec<u8> {
    let text = format!(
        r#"(module
            {imports}
            (memory (export "memory") 1)
            (global $heap (mut i32) (i32.const 1024))
            (func (export "dedalo_abi") (result i32) i32.const 1)
            (func (export "dedalo_alloc") (param $len i32) (result i32)
                (local $p i32)
                global.get $heap
                local.set $p
                global.get $heap
                local.get $len
                i32.add
                global.set $heap
                local.get $p)
            (func (export "dedalo_free") (param i32 i32))
            (data (i32.const 16) "{{\"ok\":{{\"nodes\":7}}}}")
            (func (export "dedalo_call") (param $ptr i32) (param $len i32) (result i64)
                {call_body}))"#
    );
    wat::parse_str(text).expect("valid fixture")
}

fn load(bytes: &[u8]) -> Result<WasmiRuntime, String> {
    WasmiRuntime::load(bytes, BTreeMap::new())
}

#[test]
fn an_endless_loop_runs_out_of_fuel() {
    let rt = load(&module("", "(loop $l (br $l)) i64.const 0"))
        .unwrap()
        .with_fuel(5_000_000);
    assert!(matches!(rt.call(b"{}", 0), Err(CallError::Timeout(_))));
}

#[test]
fn a_memory_bomb_hits_the_cap() {
    // Grows memory until refused; with traps on grow failure this stops it.
    let body = "(loop $l (drop (memory.grow (i32.const 256))) (br $l)) i64.const 0";
    let rt = load(&module("", body)).unwrap();
    assert!(
        matches!(rt.call(b"{}", 0), Err(CallError::Timeout(_))),
        "{:?}",
        rt.call(b"{}", 0)
    );
}

#[test]
fn forbidden_imports_are_rejected_at_load() {
    let wasi = r#"(import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))"#;
    let err = load(&module(wasi, "i64.const 0")).err().expect("rejected");
    assert!(err.contains("only from the `dedalo` namespace"), "{err}");
    let unknown = r#"(import "dedalo" "open_file" (func (param i32 i32) (result i32)))"#;
    assert!(
        load(&module(unknown, "i64.const 0"))
            .err()
            .unwrap()
            .contains("does not provide")
    );
}

#[test]
fn a_trap_and_an_out_of_range_answer_are_failures() {
    let rt = load(&module("", "unreachable")).unwrap();
    assert!(matches!(rt.call(b"{}", 0), Err(CallError::Failed(_))));
    // A pointer far outside memory.
    let rt = load(&module("", "i64.const 0x7fff000000000010")).unwrap();
    assert!(matches!(rt.call(b"{}", 0), Err(CallError::Failed(_))));
}

#[test]
fn invalid_output_is_refused_by_the_adapter() {
    // Answers `{"ok":{"nodes":7}}`: valid JSON, but not scene nodes.
    let rt = load(&module("", "i64.const 0x0000001000000012")).unwrap();
    assert_eq!(rt.call(b"{}", 0).unwrap(), br#"{"ok":{"nodes":7}}"#);
    let manifest = "[plugin]\nid = \"bad\"\nversion = \"1.0.0\"\napi = 1\n\n[wasm]\nmodule = \"bad.wasm\"\n\n[[painter]]\nmatch = { kind = \"space\" }\nlayer = \"floor\"\n";
    let files = BTreeMap::from([
        (
            "dedalo-plugin.toml".to_string(),
            manifest.as_bytes().to_vec(),
        ),
        (
            "bad.wasm".to_string(),
            module("", "i64.const 0x0000001000000012"),
        ),
    ]);
    let plugin = dedalo_plugin::load_files(files, Some(&dedalo_plugin::wasmi_factory)).unwrap();
    let request = serde_json::json!({ "point": "painter", "input": {} });
    let ok = dedalo_plugin::points::call(&plugin, &request, 0).unwrap();
    assert!(serde_json::from_value::<Vec<dedalo_scene::Node>>(ok["nodes"].clone()).is_err());
}

#[test]
fn a_wrong_abi_version_is_rejected() {
    let bytes = wat::parse_str(
        r#"(module (memory (export "memory") 1)
            (func (export "dedalo_abi") (result i32) i32.const 2)
            (func (export "dedalo_alloc") (param i32) (result i32) i32.const 0)
            (func (export "dedalo_free") (param i32 i32))
            (func (export "dedalo_call") (param i32 i32) (result i64) i64.const 0))"#,
    )
    .unwrap();
    assert!(load(&bytes).err().unwrap().contains("ABI 2"));
}
