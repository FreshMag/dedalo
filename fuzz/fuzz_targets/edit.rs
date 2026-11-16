//! Edit operations from arbitrary JSON never panic, and a refused batch
//! leaves the text as it was.
#![no_main]

use dedalo_core::Registry;
use dedalo_core::model::ops::Batch;

const BASE: &str = include_str!("../../tests/corpus/forgotten_crypt/input.dedalo");

libfuzzer_sys::fuzz_target!(|ops: &str| {
    let Ok(batch) = serde_json::from_str::<Batch>(ops) else { return };
    let reg = Registry::builtin();
    let result = dedalo_core::edit(BASE, &batch, &reg, false, "x");
    if !result.applied {
        assert_eq!(result.hash, dedalo_core::content_hash(BASE));
    }
});
