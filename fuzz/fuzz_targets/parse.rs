//! Parsing never panics, and printing a parsed file returns its bytes.
#![no_main]

libfuzzer_sys::fuzz_target!(|text: &str| {
    let parsed = dedalo_syntax::parse(text);
    assert_eq!(parsed.syntax().text().to_string(), text);
});
