//! The golden corpus (spec 15.1): every corpus file has expected
//! diagnostics, document JSON, resolved JSON, text maps and SVG bytes,
//! compared byte for byte. The same expected files are checked on every
//! platform and against the WebAssembly build, which is how identical output
//! across targets is enforced.
//!
//! Run with `DEDALO_BLESS=1` to rewrite the expected files after a reviewed
//! change of output.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dedalo_core::model::{Preset, Value};
use dedalo_core::{Audience, Compiled, ExportSpec, Input, Registry, build_view};

include!("../../../tests/support/build_plugins.rs");

/// The built-ins and the reference plugins, which the plugin case needs.
fn registry() -> Registry {
    let mut reg = Registry::builtin();
    let dir = reference_plugins();
    for id in ["blueprint", "hand_drawn", "uvtt"] {
        let files = dedalo_plugin::read_dir(&dir.join(id)).expect("a reference plugin");
        let plugin = dedalo_plugin::load_files(files, Some(&dedalo_plugin::wasmi_factory))
            .expect("it loads");
        reg.add_plugin(plugin).expect("it registers");
    }
    reg
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

fn cases() -> Vec<(String, PathBuf)> {
    let mut cases: Vec<(String, PathBuf)> = std::fs::read_dir(corpus_dir())
        .expect("the corpus directory exists")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?.to_string();
            path.join("input.dedalo").exists().then_some((name, path))
        })
        .collect();
    cases.sort();
    assert!(cases.len() >= 20, "the corpus is missing");
    cases
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        .replace("\r\n", "\n")
}

fn compile(dir: &Path, reg: &Registry) -> (String, Compiled) {
    let source = read(&dir.join("input.dedalo"));
    let compiled = dedalo_core::compile(Input::Dsl(&source), reg);
    (source, compiled)
}

/// The exports to render for a case: those it declares, else both presets.
fn specs(compiled: &Compiled) -> Vec<ExportSpec> {
    let names: Vec<String> = compiled
        .document
        .iter()
        .flat_map(|d| d.dungeon.exports.iter().map(|e| e.name.clone()))
        .collect();
    if names.is_empty() {
        return vec![
            ExportSpec::preset(Preset::Dm),
            ExportSpec::preset(Preset::Player),
        ];
    }
    names
        .iter()
        .map(|n| dedalo_core::export_spec(compiled, Some(n)).expect("a declared export"))
        .collect()
}

fn outputs(compiled: &Compiled, reg: &Registry) -> BTreeMap<String, String> {
    dedalo_core::golden_outputs(compiled, reg, true)
}

#[test]
fn corpus_matches_the_golden_files() {
    let reg = registry();
    let bless = std::env::var_os("DEDALO_BLESS").is_some();
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut failures = Vec::new();
    for (name, dir) in cases() {
        let (_, compiled) = compile(&dir, &reg);
        let files = outputs(&compiled, &reg);
        // Rendering twice gives the same bytes.
        assert_eq!(
            files,
            outputs(&compiled, &reg),
            "{name}: output is not deterministic"
        );
        for (file, actual) in &files {
            let path = dir.join(file);
            if bless {
                std::fs::write(&path, actual).expect("writing an expected file");
            } else if !path.exists() {
                failures.push(format!("{name}/{file}: no expected file"));
            } else if read(&path) != *actual {
                let expected = read(&path);
                let line = expected
                    .lines()
                    .zip(actual.lines())
                    .position(|(a, b)| a != b)
                    .unwrap_or(0)
                    + 1;
                failures.push(format!("{name}/{file}: differs from line {line}"));
            }
        }
        if bless {
            // Remove expected files the case no longer produces.
            for entry in std::fs::read_dir(&dir).expect("a case directory").flatten() {
                let file = entry.file_name().to_string_lossy().to_string();
                if file != "input.dedalo" && !files.contains_key(&file) {
                    std::fs::remove_file(entry.path()).expect("removing a stale file");
                }
            }
        }
        index.insert(name, files.keys().cloned().collect());
    }
    let index_text = dedalo_core::stage_json(&index);
    let index_path = corpus_dir().join("index.json");
    if bless {
        std::fs::write(&index_path, &index_text).expect("writing the corpus index");
    } else if !index_path.exists() || read(&index_path) != index_text {
        failures.push("index.json: out of date".to_string());
    }
    assert!(
        failures.is_empty(),
        "golden files differ; review the change and rerun with DEDALO_BLESS=1:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn each_error_fixture_raises_its_code_with_a_span() {
    let reg = registry();
    let mut seen = 0;
    for (name, dir) in cases() {
        let Some(code) = name.strip_prefix("err_") else {
            continue;
        };
        let (_, compiled) = compile(&dir, &reg);
        let code = code.to_uppercase();
        let hit = compiled
            .diagnostics
            .iter()
            .find(|d| d.code.as_str() == code);
        let hit =
            hit.unwrap_or_else(|| panic!("{name}: no {code} among {:#?}", compiled.diagnostics));
        assert!(
            hit.span.is_some() || code == "PLUGIN_NOT_FOUND",
            "{name}: {code} has no span"
        );
        assert!(
            hit.message.len() > 30,
            "{name}: the message is too terse to act on"
        );
        seen += 1;
    }
    assert_eq!(
        seen, 20,
        "one failing fixture per implemented diagnostic code"
    );
}

#[test]
fn the_spec_examples_get_the_fixes_the_spec_describes() {
    let reg = registry();
    let find = |case: &str, code: &str| {
        let (_, compiled) = compile(&corpus_dir().join(case), &reg);
        compiled
            .diagnostics
            .into_iter()
            .find(|d| d.code.as_str() == code)
            .expect("the diagnostic")
    };
    // Spec 10.1: the door at offset 7.
    let door = find("err_door_not_placeable", "DOOR_NOT_PLACEABLE");
    assert_eq!(
        door.message,
        "Door `entrance_door` at offset 7 on the north wall of `entrance` lies outside the wall shared with `guard_room`. Valid offsets are 1 to 6."
    );
    assert_eq!(door.related[0].message, "shares x 1 to 7 of that wall");
    assert_eq!(door.span.as_ref().map(|s| s.start), Some([18, 22]));
    let fix = serde_json::to_value(door.fix.expect("a fix")).unwrap();
    assert_eq!(
        fix,
        serde_json::json!({ "op": "update", "id": "entrance_door", "set": { "placement.offset": 3 } })
    );
    // Spec 7.3: the draft's reference example overlaps until the treasure room moves.
    let overlap = find("err_overlap", "OVERLAP");
    assert_eq!(overlap.element.as_deref(), Some("treasure_room"));
    assert!(
        overlap.message.contains("x 7 to 8, y 0 to 1"),
        "{}",
        overlap.message
    );
    let fix = serde_json::to_value(overlap.fix.expect("a fix")).unwrap();
    assert_eq!(
        fix,
        serde_json::json!({ "op": "update", "id": "treasure_room", "set": { "layout.offset": -1 } })
    );
}

fn strings(
    values: &[Option<&String>],
    properties: Option<&dedalo_core::model::Props>,
) -> Vec<String> {
    let mut out: Vec<String> = values.iter().flatten().map(|s| s.to_string()).collect();
    for value in properties.into_iter().flat_map(|p| p.values()) {
        if let Value::Str(s) = value {
            out.push(s.clone());
        }
    }
    out
}

/// The leak test (spec 15.1): no ID, name or text of an element removed from
/// a player view appears anywhere in the player output.
#[test]
fn player_outputs_leak_nothing() {
    let reg = registry();
    let mut checked = 0;
    for (name, dir) in cases() {
        let (_, compiled) = compile(&dir, &reg);
        if compiled.has_errors() {
            continue;
        }
        let resolved = compiled.resolved.as_ref().expect("a clean case resolves");
        let mut all = specs(&compiled);
        all.push(ExportSpec::preset(Preset::Player));
        all.push(ExportSpec::preset(Preset::Clean));
        for mut spec in all.into_iter().filter(|s| s.audience() == Audience::Player) {
            let view = build_view(resolved, &spec);
            let mut hidden: Vec<String> = Vec::new();
            let mut visible: Vec<String> = Vec::new();
            let mut sort = |shown: bool, found: Vec<String>| {
                if shown {
                    visible.extend(found)
                } else {
                    hidden.extend(found)
                }
            };
            for s in &resolved.spaces {
                sort(
                    view.spaces.iter().any(|v| v.id == s.id),
                    strings(&[Some(&s.id), s.name.as_ref()], None),
                );
            }
            for o in &resolved.openings {
                sort(
                    view.openings.iter().any(|v| v.id == o.id),
                    strings(&[Some(&o.id), o.name.as_ref()], None),
                );
            }
            for e in &resolved.elements {
                let shown = view.elements.iter().any(|v| v.id == e.id);
                sort(
                    shown,
                    strings(
                        &[
                            Some(&e.id),
                            e.name.as_ref(),
                            e.reference.as_ref(),
                            Some(&e.label),
                        ],
                        Some(&e.properties),
                    ),
                );
            }
            for n in &resolved.notes {
                sort(
                    view.notes.iter().any(|v| v.id == n.id),
                    strings(&[Some(&n.id), Some(&n.text)], None),
                );
            }
            let secrets: Vec<&String> = hidden
                .iter()
                .filter(|s| s.len() >= 4 && !visible.contains(s))
                .collect();

            // Live text, so that every word on the map is searchable.
            spec.live_text = true;
            spec.format = "svg".to_string();
            let svg = String::from_utf8(dedalo_core::render(&compiled, &spec, &reg).unwrap().bytes)
                .unwrap();
            spec.format = "txt".to_string();
            let txt = String::from_utf8(dedalo_core::render(&compiled, &spec, &reg).unwrap().bytes)
                .unwrap();
            // Source annotations are emitted only for the DM audience.
            assert!(
                !svg.contains("data-id"),
                "{name}/{}: the player SVG carries element IDs",
                spec.name
            );
            let texts: Vec<&str> = svg
                .split("</text>")
                .filter_map(|chunk| chunk.rsplit_once('>').map(|(_, text)| text))
                .collect();
            for secret in secrets {
                let escaped = secret
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;");
                assert!(
                    !texts.iter().any(|t| t.contains(&escaped)),
                    "{name}/{}: `{secret}` leaks into the player SVG",
                    spec.name
                );
                assert!(
                    !txt.to_lowercase().contains(&secret.to_lowercase()),
                    "{name}/{}: `{secret}` leaks into the player text map",
                    spec.name
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= 8);
}

#[test]
fn the_text_map_of_one_room_matches_the_specification() {
    let reg = registry();
    // The entrance room of spec 9.6, on its own.
    let source = r#"dungeon "d" {
    room "entrance" {
        shape = rectangle(8, 6)
        feature "pressure_trap" {
            type = trap
            position = [4, 3]
        }
        feature "wall_torch" {
            type = lighting
            subtype = torch
            placement { wall = south, offset = 2 }
        }
    }
    room "north" {
        shape = rectangle(8, 1)
        visibility = hidden
        layout { north_of = "entrance" }
    }
    connection "door" {
        from = "entrance"
        to = "north"
        type = door
        placement { wall = north, offset = 3 }
    }
    export "map" {
        format = txt
        include = ["north", "door"]
        exclude = ["north"]
    }
}
"#;
    let compiled = dedalo_core::compile(Input::Dsl(source), &reg);
    assert!(!compiled.has_errors(), "{:#?}", compiled.diagnostics);
    // Draw the door without its far room by rendering the full model's
    // entrance alone: hide the room, keep the door's wall cut.
    let mut spec = ExportSpec::preset(Preset::Dm);
    spec.format = "txt".to_string();
    let mut resolved = compiled.resolved.clone().unwrap();
    resolved.spaces.retain(|s| s.id == "entrance");
    resolved.openings[0].to = None;
    let view = build_view(&resolved, &spec);
    let expected = "\
####+#####      + door    T trap    * light
#........#
#........#
#........#
#....T...#
#........#
#..*.....#
##########
";
    assert_eq!(dedalo_core::text_map(&view), expected);
}

#[test]
fn documentation_samples_compile() {
    let reg = registry();
    let docs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let mut samples = 0;
    // The guide and the user documentation; the specification holds
    // fragments, not complete files.
    let files = std::fs::read_dir(docs.join("guide"))
        .expect("the guide directory")
        .chain(std::fs::read_dir(&docs).expect("the docs directory"))
        .flatten()
        .filter(|e| {
            e.path().extension().is_some_and(|x| x == "md") && e.file_name() != "specification.md"
        });
    for entry in files {
        let text = read(&entry.path());
        for block in text.split("```hcl\n").skip(1) {
            let sample = block.split("```").next().unwrap_or_default();
            if !sample.trim_start().starts_with("dungeon")
                && !sample.trim_start().starts_with("dedalo")
            {
                continue;
            }
            let compiled = dedalo_core::compile(Input::Dsl(sample), &reg);
            assert!(
                compiled.diagnostics.is_empty(),
                "{}: {:#?}",
                entry.path().display(),
                compiled.diagnostics
            );
            samples += 1;
        }
    }
    assert!(samples >= 15, "{samples} samples");
}

#[test]
fn json_input_gives_the_same_map_as_dsl_input() {
    let reg = registry();
    for (name, dir) in cases() {
        let (_, compiled) = compile(&dir, &reg);
        let Some(doc) = compiled
            .document
            .as_ref()
            .filter(|_| !compiled.has_errors())
        else {
            continue;
        };
        let from_json = dedalo_core::compile(Input::Json(&dedalo_core::to_json(doc)), &reg);
        assert_eq!(from_json.document.as_ref(), Some(doc), "{name}");
        assert_eq!(from_json.resolved, compiled.resolved, "{name}");
        // And the text regenerated from JSON compiles to the same model.
        let regenerated = dedalo_core::to_dsl(doc, &reg);
        let again = dedalo_core::compile(Input::Dsl(&regenerated), &reg);
        assert_eq!(again.resolved, compiled.resolved, "{name}");
    }
}
