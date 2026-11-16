//! Document JSON input never panics, and a valid document survives DSL.
#![no_main]

use dedalo_core::{Input, Registry};

libfuzzer_sys::fuzz_target!(|text: &str| {
    let reg = Registry::builtin();
    let compiled = dedalo_core::compile(Input::Json(text), &reg);
    if let Some(doc) = compiled.document.filter(|_| !compiled.diagnostics.iter().any(|d| matches!(d.code.stage(), "parse" | "lower"))) {
        let regenerated = dedalo_core::to_dsl(&doc, &reg);
        assert_eq!(dedalo_core::compile(Input::Dsl(&regenerated), &reg).document, Some(doc));
    }
});
