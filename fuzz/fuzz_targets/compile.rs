//! Parse, lower, resolve and render never panic (spec 15.1).
#![no_main]

use dedalo_core::{ExportSpec, Input, Preset, Registry};

libfuzzer_sys::fuzz_target!(|text: &str| {
    let reg = Registry::builtin();
    let compiled = dedalo_core::compile(Input::Dsl(text), &reg);
    if compiled.resolved.is_some() {
        for preset in [Preset::Dm, Preset::Player] {
            for format in ["svg", "txt"] {
                let mut spec = ExportSpec::preset(preset);
                spec.format = format.to_string();
                let _ = dedalo_core::render(&compiled, &spec, &reg);
            }
        }
    }
});
