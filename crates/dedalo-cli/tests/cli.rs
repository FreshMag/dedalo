//! The command-line contract (spec section 12): exit codes, and standard
//! output that carries only the result.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn corpus(case: &str, file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus")
        .join(case)
        .join(file)
}

fn read(case: &str, file: &str) -> String {
    std::fs::read_to_string(corpus(case, file))
        .unwrap()
        .replace("\r\n", "\n")
}

fn dedalo(args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dedalo"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .replace("\r\n", "\n")
}

/// A fresh directory under the system's temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dedalo-cli-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn exit_codes_follow_the_specification() {
    let clean = corpus("forgotten_crypt", "input.dedalo");
    let broken = corpus("err_door_not_placeable", "input.dedalo");
    let warned = corpus("err_unreachable_room", "input.dedalo");
    assert_eq!(
        dedalo(&["validate", clean.to_str().unwrap()], None)
            .status
            .code(),
        Some(0),
        "success"
    );
    assert_eq!(
        dedalo(&["validate", warned.to_str().unwrap()], None)
            .status
            .code(),
        Some(0),
        "warnings only"
    );
    assert_eq!(
        dedalo(&["validate", broken.to_str().unwrap()], None)
            .status
            .code(),
        Some(1),
        "diagnostics with an error"
    );
    assert_eq!(
        dedalo(&["validate"], None).status.code(),
        Some(2),
        "invalid usage"
    );
    assert_eq!(
        dedalo(
            &["render", clean.to_str().unwrap(), "--rotation", "45"],
            None
        )
        .status
        .code(),
        Some(2),
        "invalid value"
    );
    assert_eq!(
        dedalo(&["validate", "no/such/file.dedalo"], None)
            .status
            .code(),
        Some(3),
        "file failure"
    );
}

#[test]
fn json_output_is_the_report_and_nothing_else() {
    let broken = corpus("err_door_not_placeable", "input.dedalo");
    let output = dedalo(&["validate", broken.to_str().unwrap(), "--json"], None);
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("standard output is one JSON document");
    assert_eq!(report["status"], "error");
    assert_eq!(report["summary"]["errors"], 1);
    let diagnostic = &report["diagnostics"][0];
    assert_eq!(diagnostic["code"], "DOOR_NOT_PLACEABLE");
    assert_eq!(diagnostic["element"], "entrance_door");
    assert_eq!(diagnostic["span"]["file"], broken.to_str().unwrap());
    assert_eq!(diagnostic["fix"]["set"]["placement.offset"], 3);
}

#[test]
fn human_diagnostics_go_to_standard_error_with_an_excerpt() {
    let broken = corpus("err_door_not_placeable", "input.dedalo");
    let output = dedalo(&["validate", broken.to_str().unwrap()], None);
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error[DOOR_NOT_PLACEABLE]"), "{stderr}");
    assert!(stderr.contains("18 |             offset = 7"), "{stderr}");
    assert!(stderr.contains("= fix: {\"op\":\"update\""), "{stderr}");
}

#[test]
fn render_writes_the_golden_bytes_to_standard_output() {
    let input = corpus("forgotten_crypt", "input.dedalo");
    for (preset, format, golden) in [("player", "txt", "player.txt"), ("dm", "svg", "dm.svg")] {
        let output = dedalo(
            &[
                "render",
                input.to_str().unwrap(),
                "--preset",
                preset,
                "--format",
                format,
                "-o",
                "-",
            ],
            None,
        );
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            stdout(&output),
            read("forgotten_crypt", golden),
            "{preset}.{format}"
        );
    }
    // Standard input works the same way.
    let output = dedalo(
        &["render", "-", "--export", "player", "--format", "txt"],
        Some(&read("forgotten_crypt", "input.dedalo")),
    );
    assert_eq!(stdout(&output), read("forgotten_crypt", "player.txt"));
}

#[test]
fn render_refuses_errors_unless_forced() {
    let broken = corpus("err_door_not_placeable", "input.dedalo");
    let refused = dedalo(
        &[
            "render",
            broken.to_str().unwrap(),
            "--format",
            "txt",
            "-o",
            "-",
        ],
        None,
    );
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
    // With --force, whatever resolved is drawn: both rooms, without the door.
    let forced = dedalo(
        &[
            "render",
            broken.to_str().unwrap(),
            "--format",
            "txt",
            "-o",
            "-",
            "--force",
        ],
        None,
    );
    assert_eq!(forced.status.code(), Some(1));
    let map = stdout(&forced);
    assert!(map.lines().count() > 10 && !map.contains('+'), "{map}");
}

#[test]
fn render_all_writes_one_file_per_export_beside_the_source() {
    let dir = scratch("render-all");
    let input = dir.join("crypt.dedalo");
    std::fs::copy(corpus("forgotten_crypt", "input.dedalo"), &input).unwrap();
    let output = dedalo(
        &["render", input.to_str().unwrap(), "--all", "--json"],
        None,
    );
    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["outputs"].as_array().unwrap().len(), 2);
    for (name, golden) in [
        ("forgotten_crypt.dm.svg", "dm.svg"),
        ("forgotten_crypt.player.svg", "player.svg"),
    ] {
        let written = std::fs::read_to_string(dir.join(name))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(written, read("forgotten_crypt", golden), "{name}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fmt_and_convert_work_through_pipes() {
    let source = read("forgotten_crypt", "input.dedalo");
    // The corpus file is canonical.
    let input = corpus("forgotten_crypt", "input.dedalo");
    assert_eq!(
        dedalo(&["fmt", input.to_str().unwrap(), "--check"], None)
            .status
            .code(),
        Some(0)
    );
    let messy = "dungeon d {\n room a { shape = square(3) }\n}\n";
    assert_eq!(
        dedalo(&["fmt", "-", "--check"], Some(messy)).status.code(),
        Some(1)
    );
    assert_eq!(
        stdout(&dedalo(&["fmt", "-"], Some(messy))),
        "dungeon \"d\" {\n    room \"a\" { shape = square(3) }\n}\n"
    );
    // DSL to JSON to DSL to JSON: the model survives.
    let json = stdout(&dedalo(&["convert", "-", "--to", "json"], Some(&source)));
    assert_eq!(json, read("forgotten_crypt", "document.json"));
    let regenerated = stdout(&dedalo(&["convert", "-", "--to", "dsl"], Some(&json)));
    assert_eq!(
        stdout(&dedalo(
            &["convert", "-", "--to", "json"],
            Some(&regenerated)
        )),
        json
    );
}

#[test]
fn reference_commands_answer() {
    let explain = stdout(&dedalo(&["explain", "door_not_placeable"], None));
    assert!(
        explain.starts_with("DOOR_NOT_PLACEABLE (Error, resolve stage)"),
        "{explain}"
    );
    let catalog = stdout(&dedalo(&["catalog", "trap"], None));
    assert!(catalog.contains("detection_dc"), "{catalog}");
    let schema: serde_json::Value =
        serde_json::from_slice(&dedalo(&["schema", "document"], None).stdout).unwrap();
    assert_eq!(schema["title"], "Document");
    assert!(stdout(&dedalo(&["guide"], None)).starts_with("# Dedalo quickstart"));
    assert_eq!(dedalo(&["explain", "NOPE"], None).status.code(), Some(2));
    let starter = stdout(&dedalo(&["new", "-o", "-"], None));
    assert_eq!(
        dedalo(&["validate", "-"], Some(&starter)).status.code(),
        Some(0)
    );
}

#[test]
fn markdown_notes_hold_dungeons_in_code_blocks() {
    let dir = scratch("markdown");
    let note = dir.join("Crypt.md");
    let crypt = read("forgotten_crypt", "input.dedalo");
    let broken = read("err_unknown_attribute", "input.dedalo");
    let prose = "# The crypt\n\nRead this aloud.\n\n";
    std::fs::write(
        &note,
        format!("{prose}```dedalo player\n{crypt}```\n\nMore prose.\n\n```dedalo\n{broken}```\n"),
    )
    .unwrap();
    let path = note.to_str().unwrap();

    // The fence's `player` applies: the bytes are the player golden file.
    let output = dedalo(
        &["render", path, "--block", "1", "--format", "txt", "-o", "-"],
        None,
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), read("forgotten_crypt", "player.txt"));

    // Several blocks and no `--block`: a usage error naming them.
    let output = dedalo(&["render", path], None);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--block"));

    // `validate` checks every block, with the note's own line numbers.
    let line = 4 + 1 + crypt.lines().count() + 1 + 3 + 1 + 4;
    let output = dedalo(&["validate", path], None);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr).replace('\\', "/");
    assert!(stderr.contains(&format!("Crypt.md:{line}:9")), "{stderr}");

    // `fmt` rewrites the blocks and nothing else.
    let messy = "dungeon \"d\" {\nroom \"a\" { shape = rectangle(4,4) }\n}\n";
    std::fs::write(
        &note,
        format!("{prose}```dedalo\n{messy}```\n\nMore prose.\n"),
    )
    .unwrap();
    let output = dedalo(&["fmt", path], None);
    assert_eq!(output.status.code(), Some(0));
    let formatted = dedalo(&["fmt", "-"], Some(messy));
    assert_eq!(
        std::fs::read_to_string(&note).unwrap(),
        format!(
            "{prose}```dedalo\n{}```\n\nMore prose.\n",
            stdout(&formatted)
        )
    );
    let output = dedalo(&["fmt", "--check", path], None);
    assert_eq!(output.status.code(), Some(0));
    std::fs::remove_dir_all(dir).ok();
}
