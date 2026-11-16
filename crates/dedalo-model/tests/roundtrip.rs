//! The round-trip laws of spec 6.3, checked on every corpus file.

use std::collections::BTreeMap;
use std::path::PathBuf;

use dedalo_catalog::Catalog;
use dedalo_model::{Document, check::check, lower::lower, write::to_dsl};

fn corpus() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut cases = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let input = entry.unwrap().path().join("input.dedalo");
        if input.exists() {
            cases.push((
                input.display().to_string(),
                std::fs::read_to_string(&input)
                    .unwrap()
                    .replace("\r\n", "\n"),
            ));
        }
    }
    assert!(!cases.is_empty());
    cases
}

fn json(source: &str, cat: &Catalog) -> Option<Document> {
    let parsed = dedalo_syntax::parse(source);
    if !parsed.errors.is_empty() {
        return None;
    }
    lower(&parsed.syntax(), source, cat).document
}

fn fmt(source: &str, cat: &Catalog) -> String {
    let parsed = dedalo_syntax::parse(source);
    let lowered = lower(&parsed.syntax(), source, cat);
    dedalo_syntax::fmt::format(&parsed.syntax(), &lowered.rewrites)
}

#[test]
fn parsing_is_lossless() {
    for (name, source) in corpus() {
        assert_eq!(
            dedalo_syntax::parse(&source).syntax().text().to_string(),
            source,
            "{name}"
        );
    }
}

#[test]
fn round_trip_laws_hold_on_the_corpus() {
    let cat = Catalog::builtin();
    for (name, source) in corpus() {
        let Some(doc) = json(&source, &cat) else {
            continue;
        };
        // Law 2: regenerating the text never changes the meaning.
        let regenerated = to_dsl(&doc, &cat);
        let again = json(&regenerated, &cat)
            .unwrap_or_else(|| panic!("{name}: regenerated text does not parse:\n{regenerated}"));
        assert_eq!(
            again, doc,
            "{name}: json(dsl(json(S))) != json(S)\n{regenerated}"
        );
        // Law 1: JSON survives its own serialisation exactly.
        let text = serde_json::to_string_pretty(&doc).unwrap();
        let reread: Document = serde_json::from_str(&text).unwrap();
        assert_eq!(reread, doc, "{name}: JSON serialisation is not exact");
        // Law 3: the formatter is idempotent and keeps the meaning.
        let once = fmt(&source, &cat);
        assert_eq!(fmt(&once, &cat), once, "{name}: fmt is not idempotent");
        assert_eq!(
            json(&once, &cat).as_ref(),
            Some(&doc),
            "{name}: fmt changed the meaning"
        );
        // Regenerated text is already canonical.
        assert_eq!(
            fmt(&regenerated, &cat),
            regenerated,
            "{name}: regenerated text is not canonical"
        );
    }
}

#[test]
fn the_reference_example_is_clean_and_keeps_its_comments() {
    let cat = Catalog::builtin();
    let (_, source) = corpus()
        .into_iter()
        .find(|(n, _)| n.contains("forgotten_crypt"))
        .unwrap();
    let parsed = dedalo_syntax::parse(&source);
    assert_eq!(parsed.errors, vec![]);
    let lowered = lower(&parsed.syntax(), &source, &cat);
    let doc = lowered.document.unwrap();
    let mut diagnostics = lowered.diagnostics;
    diagnostics.extend(check(&doc, &cat));
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    // The corpus file is canonical, so formatting leaves it untouched.
    assert_eq!(
        dedalo_syntax::fmt::format(&parsed.syntax(), &BTreeMap::new()),
        source
    );
    let treasure = doc.dungeon.space("treasure_room").unwrap();
    assert_eq!(
        treasure.comments.leading,
        vec!["# Hidden from the players until they find the secret door."]
    );
    assert_eq!(
        treasure.comments.attrs["layout.align"].trailing.as_deref(),
        Some("# without this the room overlaps the entrance")
    );
    // Aliases are stored normalised (spec 5.10).
    let secret = doc
        .dungeon
        .connections
        .iter()
        .find(|c| c.id == "secret_door")
        .unwrap();
    assert_eq!((secret.ty.as_str(), secret.secret), ("door", Some(true)));
    let layout = doc
        .dungeon
        .space("guard_room")
        .unwrap()
        .layout
        .clone()
        .unwrap();
    assert_eq!(
        (layout.adjacent_to.as_deref(), layout.direction),
        (Some("entrance"), Some(dedalo_model::Direction::North))
    );
}

#[test]
fn lenient_spellings_are_rewritten_by_the_formatter() {
    let cat = Catalog::builtin();
    let source = "dungeon d {\n  room entrance { shape = rectangle(4, 4) }\n  room b {\n shape = square(3)\n layout { north_of = entrance, align = \"start\" }\n }\n}\n";
    let expected = "dungeon \"d\" {\n    room \"entrance\" { shape = rectangle(4, 4) }\n\n    room \"b\" {\n        shape = square(3)\n\n        layout {\n            north_of = \"entrance\"\n            align = start\n        }\n    }\n}\n";
    assert_eq!(fmt(source, &cat), expected);
}

#[test]
fn every_problem_is_reported_at_once_with_suggestions() {
    let cat = Catalog::builtin();
    let source = r#"dungeon "d" {
    room "a" {
        shape = rectangle(4, 4)
        colour = "red"
        feature "t" {
            type = trapp
        }
        feature "t" {
            type = trap
            properties { detection = 12 }
        }
    }
    connection "door1" {
        from = "a"
        to = "bb"
        type = dor
    }
    room "b" { shape = 4 }
}
"#;
    let parsed = dedalo_syntax::parse(source);
    let lowered = lower(&parsed.syntax(), source, &cat);
    let doc = lowered.document.unwrap();
    let mut diagnostics = lowered.diagnostics;
    let mut checked = check(&doc, &cat);
    lowered
        .source_map
        .attach(&mut checked, &dedalo_syntax::text::LineIndex::new(source));
    diagnostics.extend(checked);
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    for expected in [
        "UNKNOWN_ATTRIBUTE",
        "TYPE_MISMATCH",
        "DUPLICATE_ID",
        "UNKNOWN_TYPE",
        "UNKNOWN_PROPERTY",
        "UNKNOWN_REFERENCE",
    ] {
        assert!(codes.contains(&expected), "missing {expected} in {codes:?}");
    }
    assert!(
        diagnostics.iter().all(|d| d.span.is_some()),
        "{diagnostics:#?}"
    );
    let reference = diagnostics
        .iter()
        .find(|d| d.code.as_str() == "UNKNOWN_REFERENCE")
        .unwrap();
    assert_eq!(reference.suggestion.as_deref(), Some("Did you mean `b`?"));
    assert_eq!(reference.span.as_ref().unwrap().start, [15, 14]);
    let property = diagnostics
        .iter()
        .find(|d| d.code.as_str() == "UNKNOWN_PROPERTY")
        .unwrap();
    assert!(
        property.message.contains("Did you mean `detection_dc`?"),
        "{}",
        property.message
    );
}
