//! The performance budgets (spec 15.3), measured on the 100-room sample.
//!
//!     cargo run --release -p dedalo-core --example budgets [-- --check]
//!
//! Prints each operation's best time over a few runs. With `--check`, exits
//! with 1 when one is more than 20% over its budget.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use dedalo_core::model::ops::{Batch, Op};
use dedalo_core::model::{Num, Preset, Value};
use dedalo_core::{ExportSpec, Input, Registry};

fn best<T>(runs: usize, mut f: impl FnMut() -> T) -> Duration {
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(f());
            start.elapsed()
        })
        .min()
        .unwrap_or_default()
}

fn main() {
    let check = std::env::args().any(|a| a == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/perf");
    let source = std::fs::read_to_string(root.join("rooms_100.dedalo"))
        .expect("the 100-room sample")
        .replace("\r\n", "\n");
    let small = std::fs::read_to_string(root.join("rooms_50.dedalo"))
        .expect("the 50-room sample")
        .replace("\r\n", "\n");
    let reg = Registry::builtin();
    let compiled = dedalo_core::compile(Input::Dsl(&source), &reg);
    assert!(
        !compiled.has_errors(),
        "{:#?}",
        compiled.diagnostics.iter().take(3).collect::<Vec<_>>()
    );
    let elements = compiled
        .document
        .as_ref()
        .map_or(0, |d| d.dungeon.elements().count());

    let mut svg = ExportSpec::preset(Preset::Dm);
    svg.format = "svg".to_string();
    let mut png = svg.clone();
    png.format = "png".to_string();
    png.cell_px = 64;
    let batch = Batch {
        base_hash: None,
        ops: vec![Op::set(
            "r050_e3",
            "position",
            Value::List(vec![Value::Num(Num::int(2)), Value::Num(Num::int(2))]),
        )],
    };

    let results = [
        (
            "validate, 100 rooms",
            best(5, || dedalo_core::compile(Input::Dsl(&source), &reg)),
            50,
        ),
        (
            "render to SVG, 100 rooms",
            best(5, || dedalo_core::render(&compiled, &svg, &reg)),
            150,
        ),
        (
            "render to PNG at 64 px per cell, 100 rooms",
            best(2, || dedalo_core::render(&compiled, &png, &reg)),
            1500,
        ),
        (
            "apply an edit batch, 100 rooms",
            best(5, || dedalo_core::edit(&source, &batch, &reg, false, "x")),
            50,
        ),
        (
            "keystroke to preview, 50 rooms (compile and SVG)",
            best(5, || {
                let c = dedalo_core::compile(Input::Dsl(&small), &reg);
                dedalo_core::render(&c, &svg, &reg)
            }),
            100,
        ),
    ];
    println!("100-room sample: {elements} elements\n");
    let mut over = false;
    for (name, took, budget_ms) in results {
        let ms = took.as_secs_f64() * 1000.0;
        let flag = if ms > budget_ms as f64 * 1.2 {
            over = true;
            "OVER"
        } else {
            "ok"
        };
        println!("{flag:<5} {name:<50} {ms:>9.1} ms   budget {budget_ms} ms");
    }
    if check && over {
        std::process::exit(1);
    }
}
