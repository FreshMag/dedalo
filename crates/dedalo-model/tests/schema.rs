//! The published schemas are generated from the Rust types, never written
//! by hand (spec 6.4). The copies under `spec/schema/` are what the Pages
//! site serves; this test keeps them from drifting.
//!
//! Run with `DEDALO_BLESS=1` to regenerate them after a reviewed change.

use std::path::PathBuf;

#[test]
fn published_schemas_match_the_types() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/schema");
    let bless = std::env::var_os("DEDALO_BLESS").is_some();
    for name in dedalo_model::SCHEMA_NAMES {
        let schema = dedalo_model::schema(name).expect("a published schema");
        let mut text = serde_json::to_string_pretty(&schema).unwrap();
        text.push('\n');
        let path = dir.join(format!("{name}-1.json"));
        if bless {
            std::fs::write(&path, &text).unwrap();
        }
        let stored = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert_eq!(
            stored,
            text,
            "{} is out of date; rerun with DEDALO_BLESS=1",
            path.display()
        );
    }
}

#[test]
fn the_document_schema_describes_the_model() {
    let schema = dedalo_model::schema("document").unwrap();
    assert_eq!(schema["required"], serde_json::json!(["dungeon"]));
    let defs = schema["$defs"].as_object().expect("definitions");
    for ty in [
        "Dungeon",
        "Space",
        "Connection",
        "Element",
        "Note",
        "Export",
        "Layout",
        "Comments",
    ] {
        assert!(defs.contains_key(ty), "missing {ty}");
    }
    // Closed objects: an unknown field is an error, in JSON as in the DSL.
    assert_eq!(defs["Space"]["additionalProperties"], false);
    assert_eq!(
        defs["Visibility"]["enum"],
        serde_json::json!(["public", "dm", "conditional", "hidden"])
    );
}
