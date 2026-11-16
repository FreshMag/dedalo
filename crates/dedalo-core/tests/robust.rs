//! Robustness on damaged input. Parse, lower and resolve must never panic,
//! parsing must stay lossless, and the formatter must stay idempotent
//! (spec 15.1). This is a quick deterministic stand-in for the `cargo fuzz`
//! targets, which need a nightly toolchain and run in CI.

use std::path::PathBuf;

use dedalo_core::{ExportSpec, Input, Preset, Registry};

/// A small deterministic generator, so that failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn sources() -> Vec<String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("input.dedalo")).ok())
        .map(|s| s.replace("\r\n", "\n"))
        .collect();
    out.sort();
    out
}

const FRAGMENTS: &[&str] = &[
    "{",
    "}",
    "[",
    "]",
    "(",
    ")",
    "=",
    ",",
    "\"",
    "\n",
    "#",
    "/*",
    "*/",
    "<<EOT\n",
    "<<-EOT\n",
    "EOT",
    "${",
    "\\",
    "-",
    ".",
    "0",
    "9999999999",
    "1.2345",
    "true",
    "null",
    "room",
    "layout",
    "shape = circle(0.5)",
    "polygon([[0,0],[1,1],[2,2]])",
    "rotation = 90",
    "è",
    "\u{feff}",
    "\r\n",
    "\t",
    "north_of = \"entrance\"",
    "connects = [\"a\", \"a\"]",
    "position = [-3, 400000]",
    "width = 0",
    "size = [0, 0]",
];

fn mutate(source: &str, rng: &mut Rng) -> String {
    let mut text = source.to_string();
    for _ in 0..1 + rng.below(4) {
        let boundaries: Vec<usize> = text
            .char_indices()
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect();
        let at = boundaries[rng.below(boundaries.len())];
        match rng.below(4) {
            0 => text.insert_str(at, FRAGMENTS[rng.below(FRAGMENTS.len())]),
            1 => {
                let end = boundaries[(boundaries.iter().position(|b| *b == at).unwrap()
                    + 1
                    + rng.below(12))
                .min(boundaries.len() - 1)];
                text.replace_range(at..end, "");
            }
            2 => {
                // Duplicate a line somewhere else: duplicate IDs, misplaced blocks.
                let lines: Vec<&str> = text.lines().collect();
                let line = lines
                    .get(rng.below(lines.len()))
                    .copied()
                    .unwrap_or("room")
                    .to_string();
                text.insert_str(at, &format!("\n{line}\n"));
            }
            _ => text.truncate(at),
        }
    }
    text
}

#[test]
fn damaged_input_never_panics_and_parsing_stays_lossless() {
    let reg = Registry::builtin();
    let sources = sources();
    // `DEDALO_FUZZ_ROUNDS` and `DEDALO_FUZZ_SEED` widen the search by hand.
    let env = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok());
    let mut rng = Rng(env("DEDALO_FUZZ_SEED").unwrap_or(0x9e37_79b9_7f4a_7c15) | 1);
    let mut formatted = 0;
    for round in 0..env("DEDALO_FUZZ_ROUNDS").unwrap_or(1500) as usize {
        let text = mutate(&sources[round % sources.len()], &mut rng);
        let parsed = dedalo_core::parse(&text);
        assert_eq!(
            parsed.syntax().text().to_string(),
            text,
            "parsing lost text in round {round}"
        );
        let compiled = dedalo_core::compile(Input::Dsl(&text), &reg);
        // Whatever resolved must render for every audience.
        if compiled.resolved.is_some() {
            for preset in [Preset::Dm, Preset::Player, Preset::Clean] {
                for format in ["svg", "txt"] {
                    let mut spec = ExportSpec::preset(preset);
                    spec.format = format.to_string();
                    spec.rotation = 90 * (round as i64 % 4);
                    dedalo_core::render(&compiled, &spec, &reg).expect("a resolved model renders");
                }
            }
        }
        if let Ok(once) = dedalo_core::format(&text, &reg) {
            let twice = dedalo_core::format(&once, &reg).expect("formatted text parses");
            assert_eq!(
                twice, once,
                "fmt is not idempotent in round {round}:\n{text}"
            );
            formatted += 1;
        }
        // The document survives a trip through JSON and regenerated text.
        if let Some(doc) = compiled.document.as_ref().filter(|_| {
            compiled
                .diagnostics
                .iter()
                .all(|d| !matches!(d.code.stage(), "parse" | "lower"))
        }) {
            let json = dedalo_core::to_json(doc);
            let from_json = dedalo_core::compile(Input::Json(&json), &reg);
            assert_eq!(
                from_json.document.as_ref(),
                Some(doc),
                "JSON round trip failed in round {round}"
            );
            let regenerated = dedalo_core::to_dsl(doc, &reg);
            let again = dedalo_core::compile(Input::Dsl(&regenerated), &reg);
            assert_eq!(
                again.document.as_ref(),
                Some(doc),
                "DSL round trip failed in round {round}:\n{text}\n--- regenerated ---\n{regenerated}"
            );
        }
    }
    assert!(
        formatted > 50,
        "too few mutants parsed cleanly to exercise the formatter"
    );
}

#[test]
fn arbitrary_bytes_never_panic() {
    let reg = Registry::builtin();
    let mut rng = Rng(42);
    for _ in 0..500 {
        let text: String = (0..rng.below(200))
            .map(|_| FRAGMENTS[rng.below(FRAGMENTS.len())])
            .collect();
        let parsed = dedalo_core::parse(&text);
        assert_eq!(parsed.syntax().text().to_string(), text);
        dedalo_core::compile(Input::Dsl(&text), &reg);
        dedalo_core::compile(Input::Json(&text), &reg);
    }
}
