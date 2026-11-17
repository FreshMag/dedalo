//! Plugins through the binary (spec 11.6): installing, the lockfile, the
//! conformance suite, and rendering with plugins found on disk.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

include!("../../../tests/support/build_plugins.rs");

fn dedalo(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dedalo"))
        .current_dir(dir)
        .env("DEDALO_CONFIG_DIR", dir.join("no-user-config"))
        .args(args)
        .output()
        .expect("the binary starts")
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("dedalo-plugin-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const DOC: &str = "dedalo {\n    plugin \"hand_drawn\" {\n        version = \"^1.2\"\n    }\n}\n\ndungeon \"d\" {\n    render {\n        theme = hand_drawn.sepia\n    }\n\n    room \"a\" {\n        shape = rectangle(6, 4)\n    }\n\n    room \"b\" {\n        shape = square(4)\n        layout { north_of = \"a\" }\n    }\n\n    connection \"door\" {\n        from = \"a\"\n        to = \"b\"\n        type = door\n    }\n}\n";

#[test]
fn install_lock_and_render() {
    let plugins = reference_plugins();
    let dir = scratch("install");
    std::fs::write(dir.join("map.dedalo"), DOC).unwrap();

    // Without the plugin, the document names what is missing.
    let missing = dedalo(&dir, &["validate", "map.dedalo", "--json"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stdout).contains("PLUGIN_NOT_FOUND"));

    let add = dedalo(
        &dir,
        &[
            "plugin",
            "add",
            plugins.join("hand_drawn").to_str().unwrap(),
        ],
    );
    assert_eq!(
        add.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let lock = std::fs::read_to_string(dir.join("dedalo.lock")).unwrap();
    assert!(
        lock.contains("id = \"hand_drawn\"") && lock.contains("sha256 = \"sha256:"),
        "{lock}"
    );
    assert!(
        dir.join(".dedalo/plugins/hand_drawn/hand_drawn.wasm")
            .exists()
    );

    let render = dedalo(&dir, &["render", "map.dedalo", "-o", "-"]);
    assert_eq!(
        render.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&render.stderr)
    );
    let svg = String::from_utf8_lossy(&render.stdout);
    assert!(svg.contains("#efe3c8"), "the sepia background is used");
    let walls = svg
        .split("<g id=\"layer-walls\">")
        .nth(1)
        .unwrap_or_default()
        .split(
            "</g>
<g id=",
        )
        .next()
        .unwrap_or_default();
    assert!(
        walls.contains('C') && !walls.contains('L'),
        "the wobble pass bent the walls: {walls}"
    );

    // A changed package is refused until the lock is updated.
    let theme = dir.join(".dedalo/plugins/hand_drawn/themes/sepia.toml");
    let text = std::fs::read_to_string(&theme)
        .unwrap()
        .replace("#efe3c8", "#ffffff");
    std::fs::write(&theme, text).unwrap();
    let refused = dedalo(&dir, &["render", "map.dedalo", "-o", "-"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&refused.stderr)
            .contains("differs from the one recorded in dedalo.lock")
    );
    let updated = dedalo(&dir, &["render", "map.dedalo", "-o", "-", "--update-lock"]);
    assert_eq!(
        updated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&updated.stderr)
    );
    assert!(
        dedalo(&dir, &["render", "map.dedalo", "-o", "-"])
            .status
            .success()
    );

    let list = dedalo(&dir, &["plugin", "list", "--json"]);
    let listed: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(listed[0]["id"], "hand_drawn");
    assert!(
        dedalo(&dir, &["plugin", "remove", "hand_drawn"])
            .status
            .success()
    );
    assert!(!dir.join(".dedalo/plugins/hand_drawn").exists());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn reference_plugins_pass_the_conformance_suite() {
    let plugins = reference_plugins();
    let dir = scratch("check");
    for id in ["blueprint", "hand_drawn", "uvtt"] {
        let out = dedalo(
            &dir,
            &[
                "plugin",
                "check",
                plugins.join(id).to_str().unwrap(),
                "--json",
            ],
        );
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(report["status"], "ok", "{id}: {report}");
    }
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn an_exporter_plugin_writes_its_format() {
    let plugins = reference_plugins();
    let dir = scratch("uvtt");
    let doc = DOC
        .replace(
            "plugin \"hand_drawn\" {\n        version = \"^1.2\"\n    }",
            "plugin \"uvtt\" {}",
        )
        .replace("        theme = hand_drawn.sepia\n", "");
    std::fs::write(dir.join("map.dedalo"), doc).unwrap();
    let out = dedalo(
        &dir,
        &[
            "render",
            "map.dedalo",
            "--plugin",
            plugins.to_str().unwrap(),
            "--format",
            "uvtt.dd2vtt",
            "--preset",
            "player",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let file = dir.join("d.player.dd2vtt");
    let vtt: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(vtt["format"], 0.3);
    assert_eq!(vtt["portals"].as_array().unwrap().len(), 1);
    assert!(vtt["line_of_sight"].as_array().unwrap().len() >= 6);
    assert!(vtt["image"].as_str().unwrap().starts_with("iVBORw0KGgo"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn new_plugins_scaffold_and_check() {
    let dir = scratch("new");
    assert!(
        dedalo(&dir, &["plugin", "new", "parchment", "--kind", "theme"])
            .status
            .success()
    );
    let out = dedalo(&dir, &["plugin", "check", "parchment"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        dedalo(&dir, &["plugin", "new", "scifi", "--kind", "catalog"])
            .status
            .success()
    );
    let doc = "dedalo {\n    plugin \"scifi\" {}\n}\n\ndungeon \"d\" {\n    room \"bridge\" {\n        shape = square(5)\n\n        feature \"helm\" {\n            type = scifi.console\n            position = [2, 2]\n        }\n    }\n}\n";
    std::fs::write(dir.join("ship.dedalo"), doc).unwrap();
    let out = dedalo(&dir, &["validate", "ship.dedalo", "--plugin", "scifi"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(dir).ok();
}

/// Serves `body` once on a local port and returns the URL.
fn serve_once(body: Vec<u8>) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/blueprint.dedalopkg",
        listener.local_addr().unwrap().port()
    );
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    url
}

#[test]
fn zipped_packages_install_from_a_file_and_from_a_url() {
    let plugins = reference_plugins();
    let dir = scratch("zipped");
    let source = plugins.join("blueprint");
    let pack = dedalo(
        &dir,
        &["plugin", "pack", source.to_str().unwrap(), "--json"],
    );
    assert_eq!(
        pack.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&pack.stderr)
    );
    let packed: serde_json::Value = serde_json::from_slice(&pack.stdout).unwrap();
    let hash = packed["sha256"].as_str().unwrap().to_string();
    let archive = dir.join("blueprint-1.0.0.dedalopkg");
    assert!(archive.exists());

    // The zipped package has the hash of the folder it was made from.
    let listed = dedalo(
        &dir,
        &[
            "plugin",
            "list",
            "--plugin",
            archive.to_str().unwrap(),
            "--json",
        ],
    );
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed[0]["sha256"].as_str(), Some(hash.as_str()));
    assert!(
        listed[0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "symbols/brazier.svg")
    );

    // A wrong hash installs nothing.
    let refused = dedalo(
        &dir,
        &[
            "plugin",
            "add",
            archive.to_str().unwrap(),
            "--sha256",
            "sha256:00",
        ],
    );
    assert_eq!(refused.status.code(), Some(2));
    assert!(!dir.join(".dedalo").exists());

    let add = dedalo(
        &dir,
        &[
            "plugin",
            "add",
            archive.to_str().unwrap(),
            "--sha256",
            &hash,
        ],
    );
    assert_eq!(
        add.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(
        dir.join(".dedalo/plugins/blueprint/symbols/table.svg")
            .exists()
    );

    // From a URL, when the system has curl.
    if Command::new("curl").arg("--version").output().is_err() {
        eprintln!("skipping the URL install: no curl on this system");
        return;
    }
    let remote = scratch("zipped-url");
    let url = serve_once(std::fs::read(&archive).unwrap());
    let add = dedalo(&remote, &["plugin", "add", &url, "--sha256", &hash]);
    assert_eq!(
        add.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let lock = std::fs::read_to_string(remote.join("dedalo.lock")).unwrap();
    assert!(lock.contains(&hash), "{lock}");
    assert!(
        remote
            .join(".dedalo/plugins/blueprint/dedalo-plugin.toml")
            .exists()
    );

    // A URL that does not answer is an error, not a crash.
    let dead = dedalo(
        &remote,
        &["plugin", "add", "http://127.0.0.1:9/x.dedalopkg"],
    );
    assert_eq!(dead.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&dead.stderr).contains("cannot"));
}
