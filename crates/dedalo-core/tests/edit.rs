//! Edit operations leave untouched text intact (spec 13.3 and 15.1), and
//! every diagnostic fix removes its diagnostic (spec 10.3).

use std::collections::BTreeMap;
use std::path::PathBuf;

use dedalo_core::model::ops::{Batch, Op};
use dedalo_core::model::{Num, Value};
use dedalo_core::{Input, Registry};

fn corpus() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path().join("input.dedalo")).ok()?;
            Some((
                e.file_name().to_string_lossy().to_string(),
                text.replace("\r\n", "\n"),
            ))
        })
        .collect();
    out.sort();
    out
}

/// The byte range of the block that declares `id`.
fn block_range(source: &str, id: &str) -> Option<(usize, usize)> {
    use dedalo_syntax::ast::{Block, label_text};
    let root = dedalo_syntax::parse(source).syntax();
    root.descendants()
        .filter(|n| n.kind() == dedalo_syntax::SyntaxKind::Block)
        .map(Block)
        .find_map(|b| {
            let own = dedalo_ops::ELEMENT_KEYWORDS.contains(&b.keyword_text().as_str())
                && b.labels().first().map(label_text).as_deref() == Some(id);
            own.then(|| {
                (
                    u32::from(b.0.text_range().start()) as usize,
                    u32::from(b.0.text_range().end()) as usize,
                )
            })
        })
}

fn ids(source: &str) -> Vec<String> {
    let reg = Registry::builtin();
    let compiled = dedalo_core::compile(Input::Dsl(source), &reg);
    let Some(doc) = compiled.document else {
        return Vec::new();
    };
    let d = &doc.dungeon;
    let mut out: Vec<String> = d.spaces.iter().map(|s| s.id.clone()).collect();
    out.extend(d.elements().map(|(_, e)| e.id.clone()));
    out.extend(d.connections.iter().map(|c| c.id.clone()));
    out.extend(d.notes.iter().map(|n| n.id.clone()));
    out
}

#[test]
fn updates_touch_only_their_own_block() {
    let reg = Registry::builtin();
    let mut checked = 0;
    for (name, source) in corpus()
        .into_iter()
        .filter(|(n, _)| !n.starts_with("err_syntax") && !n.starts_with("err_duplicate"))
    {
        for id in ids(&source) {
            let Some((start, end)) = block_range(&source, &id) else {
                continue;
            };
            let op = Op::Update {
                id: id.clone(),
                set: BTreeMap::from([(
                    "properties.x_note".to_string(),
                    Value::Str("checked".to_string()),
                )]),
                unset: Vec::new(),
            };
            let applied =
                dedalo_ops_apply(&source, op, &reg).unwrap_or_else(|e| panic!("{name}/{id}: {e}"));
            let tail = source.len() - end;
            assert_eq!(
                &applied[..start],
                &source[..start],
                "{name}/{id}: text before the block changed"
            );
            assert_eq!(
                &applied[applied.len() - tail..],
                &source[end..],
                "{name}/{id}: text after the block changed"
            );
            assert!(
                applied[start..applied.len() - tail].contains("x_note = \"checked\""),
                "{name}/{id}"
            );
            checked += 1;
        }
    }
    assert!(checked > 40, "{checked}");
}

#[test]
fn removing_a_block_removes_only_its_lines() {
    let reg = Registry::builtin();
    for (name, source) in corpus()
        .into_iter()
        .filter(|(n, _)| !n.starts_with("err_syntax") && !n.starts_with("err_duplicate"))
    {
        for id in ids(&source) {
            let Some((start, end)) = block_range(&source, &id) else {
                continue;
            };
            let Ok(applied) = dedalo_ops_apply(
                &source,
                Op::Remove {
                    id: id.clone(),
                    cascade: false,
                },
                &reg,
            ) else {
                continue;
            };
            // Whatever was removed is one contiguous region that covers the
            // block, plus whitespace and comment lines next to it.
            let removed = source.len() - applied.len();
            let fits = (start.saturating_sub(400)..=start).any(|s0| {
                let e0 = s0 + removed;
                e0 >= end
                    && source.is_char_boundary(s0)
                    && source.get(e0..).is_some()
                    && format!("{}{}", &source[..s0], &source[e0..]) == applied
            });
            assert!(
                fits,
                "{name}/{id}: the removal is not one region around the block"
            );
            assert!(
                !applied.contains(&format!("\"{id}\" {{")),
                "{name}/{id}: the block is still there"
            );
        }
    }
}

#[test]
fn every_fix_removes_its_diagnostic() {
    let reg = Registry::builtin();
    let mut fixes = 0;
    for (name, source) in corpus() {
        let compiled = dedalo_core::compile(Input::Dsl(&source), &reg);
        for d in compiled.diagnostics.iter().filter(|d| d.fix.is_some()) {
            let batch = Batch {
                base_hash: None,
                ops: vec![d.fix.clone().unwrap()],
            };
            let result = dedalo_core::edit(&source, &batch, &reg, false, "x");
            assert!(
                result.applied,
                "{name}: the fix for {} did not apply: {:#?}",
                d.code.as_str(),
                result.diagnostics
            );
            assert!(
                !result
                    .diagnostics
                    .iter()
                    .any(|x| x.code == d.code && x.element == d.element),
                "{name}: the fix for {} left it in place",
                d.code.as_str()
            );
            fixes += 1;
        }
    }
    assert!(fixes >= 3, "{fixes}");
}

#[test]
fn stale_sources_and_new_errors_are_refused() {
    let reg = Registry::builtin();
    let (_, source) = corpus()
        .into_iter()
        .find(|(n, _)| n == "forgotten_crypt")
        .unwrap();
    let stale = Batch {
        base_hash: Some("sha256:0".to_string()),
        ops: vec![],
    };
    let result = dedalo_core::edit(&source, &stale, &reg, false, "x");
    assert!(!result.applied);
    assert_eq!(result.diagnostics[0].code.as_str(), "STALE_SOURCE");
    // Moving the door off the shared wall would add an error.
    let bad = Batch {
        base_hash: Some(dedalo_ops::content_hash(&source)),
        ops: vec![Op::set(
            "entrance_door",
            "placement.offset",
            Value::Num(Num::int(9)),
        )],
    };
    let result = dedalo_core::edit(&source, &bad, &reg, false, "x");
    assert!(!result.applied);
    assert_eq!(result.diagnostics[0].code.as_str(), "DOOR_NOT_PLACEABLE");
    assert_eq!(result.hash, dedalo_ops::content_hash(&source));
    assert!(
        dedalo_core::edit(&source, &bad, &reg, true, "x").applied,
        "force writes it anyway"
    );
}

fn dedalo_ops_apply(source: &str, op: Op, reg: &Registry) -> Result<String, String> {
    dedalo_ops::apply_ops(source, &[op], &reg.catalog)
        .map(|a| a.source)
        .map_err(|d| d.message)
}
