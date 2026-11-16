//! The formatter is idempotent and keeps the meaning (spec 6.3).
#![no_main]

use dedalo_core::{Input, Registry};

libfuzzer_sys::fuzz_target!(|text: &str| {
    let reg = Registry::builtin();
    let Ok(once) = dedalo_core::format(text, &reg) else { return };
    let twice = dedalo_core::format(&once, &reg).expect("formatted text parses");
    assert_eq!(once, twice);
    let a = dedalo_core::compile(Input::Dsl(text), &reg).document;
    let b = dedalo_core::compile(Input::Dsl(&once), &reg).document;
    assert_eq!(a, b);
});
