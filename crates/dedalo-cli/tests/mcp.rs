//! MCP contract tests (spec 15.1): the intended agent loop, driven through
//! the real binary over stdio and over HTTP.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use serde_json::{Value, json};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dedalo-mcp-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct McpStdio {
    child: Child,
    out: BufReader<std::process::ChildStdout>,
    next: u64,
}

impl McpStdio {
    fn start(root: &std::path::Path) -> McpStdio {
        let mut child = Command::new(env!("CARGO_BIN_EXE_dedalo"))
            .args(["mcp", "--root", root.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        McpStdio {
            child,
            out,
            next: 1,
        }
    }

    fn send(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.child.stdin.as_mut().unwrap(), "{message}").unwrap();
        let mut line = String::new();
        self.out.read_line(&mut line).unwrap();
        let answer: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(answer["id"], id);
        answer
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let answer = self.send(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        answer["result"].clone()
    }
}

impl Drop for McpStdio {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn the_agent_loop_works_over_stdio() {
    let root = scratch("stdio");
    let mut s = McpStdio::start(&root);

    // Older clients shake hands; newer ones just call.
    let init = s.send("initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }));
    assert_eq!(init["result"]["serverInfo"]["name"], "dedalo");
    writeln!(
        s.child.stdin.as_mut().unwrap(),
        "{}",
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
    )
    .unwrap();

    let tools = s.send("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "dedalo_guide",
            "dedalo_validate",
            "dedalo_inspect",
            "dedalo_write",
            "dedalo_edit",
            "dedalo_render"
        ]
    );

    let guide = s.tool("dedalo_guide", json!({ "topic": "quickstart" }));
    assert!(
        guide["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("# Dedalo quickstart")
    );

    // Write a file with a door on the wrong side.
    let source = "dungeon \"d\" {\n    room \"a\" {\n        shape = rectangle(8, 6)\n    }\n\n    room \"b\" {\n        shape = square(6)\n        layout { north_of = \"a\" }\n    }\n\n    connection \"door\" {\n        from = \"a\"\n        to = \"b\"\n        type = door\n        placement { wall = south }\n    }\n}\n";
    let written = s.tool(
        "dedalo_write",
        json!({ "path": "maps/d.dedalo", "source": source }),
    );
    assert_eq!(written["structuredContent"]["status"], "error");
    let diagnostic = written["structuredContent"]["diagnostics"][0].clone();
    assert_eq!(diagnostic["code"], "DOOR_NOT_PLACEABLE");

    // Apply the fix the diagnostic carries.
    let edited = s.tool(
        "dedalo_edit",
        json!({ "path": "maps/d.dedalo", "ops": [diagnostic["fix"]] }),
    );
    assert_eq!(edited["isError"], false, "{edited}");
    assert_eq!(edited["structuredContent"]["status"], "ok");
    assert!(
        edited["structuredContent"]["diff"]
            .as_str()
            .unwrap()
            .contains("+        placement { wall = north }")
    );
    let hash = edited["structuredContent"]["hash"]
        .as_str()
        .unwrap()
        .to_string();

    let valid = s.tool("dedalo_validate", json!({ "path": "maps/d.dedalo" }));
    assert_eq!(valid["structuredContent"]["status"], "ok");

    let map = s.tool(
        "dedalo_inspect",
        json!({ "path": "maps/d.dedalo", "view": "text_map" }),
    );
    assert!(map["content"][0]["text"].as_str().unwrap().contains('+'));

    // A stale hash is refused.
    let stale = s.tool("dedalo_edit", json!({ "path": "maps/d.dedalo", "base_hash": "sha256:0", "ops": [{ "op": "remove", "id": "door" }] }));
    assert_eq!(stale["isError"], true);
    let fresh = s.tool("dedalo_edit", json!({ "path": "maps/d.dedalo", "base_hash": hash, "dry_run": true, "ops": [{ "op": "remove", "id": "door" }] }));
    assert_eq!(fresh["structuredContent"]["written"], false);

    let image = s.tool(
        "dedalo_render",
        json!({ "path": "maps/d.dedalo", "preset": "player", "max_px": 256 }),
    );
    assert_eq!(image["content"][0]["type"], "image");
    assert_eq!(image["content"][0]["mimeType"], "image/png");
    assert!(
        image["content"][0]["data"]
            .as_str()
            .unwrap()
            .starts_with("iVBORw0KGgo")
    );
    let pdf = s.tool(
        "dedalo_render",
        json!({ "path": "maps/d.dedalo", "format": "pdf", "out": "maps/d.pdf" }),
    );
    assert_eq!(pdf["isError"], false, "{pdf}");
    assert!(
        std::fs::read(root.join("maps/d.pdf"))
            .unwrap()
            .starts_with(b"%PDF-")
    );

    // Confinement.
    let outside = s.tool("dedalo_validate", json!({ "path": "../escape.dedalo" }));
    assert_eq!(outside["isError"], true);

    let resource = s.send(
        "resources/read",
        json!({ "uri": "dedalo://guide/catalog/trap" }),
    );
    assert!(
        resource["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("detection_dc")
    );
    let prompt = s.send(
        "prompts/get",
        json!({ "name": "review_dungeon", "arguments": { "path": "maps/d.dedalo" } }),
    );
    assert!(
        prompt["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("maps/d.dedalo")
    );
    let unknown = s.send("no/such", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);
    drop(s);
    std::fs::remove_dir_all(root).ok();
}

/// Kills the server when the test ends, even when it fails.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn post(port: u16, body: &str, origin: Option<&str>) -> (String, String) {
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let origin = origin
        .map(|o| format!("Origin: {o}\r\n"))
        .unwrap_or_default();
    write!(stream, "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nMCP-Protocol-Version: 2026-07-28\r\n{origin}Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (head.lines().next().unwrap().to_string(), body.to_string())
}

#[test]
fn the_server_answers_over_http_on_loopback_only() {
    let root = scratch("http");
    let port = 17_000 + (std::process::id() % 2000) as u16;
    let server = Command::new(env!("CARGO_BIN_EXE_dedalo"))
        .args([
            "mcp",
            "--root",
            root.to_str().unwrap(),
            "--http",
            &format!("127.0.0.1:{port}"),
        ])
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let server = KillOnDrop(server);
    let mut ready = false;
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready);
    // Stateless: a tool call needs no handshake.
    let call = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": "dedalo_validate", "arguments": { "source": "dungeon \"d\" {\n room \"a\" { shape = square(3) }\n}\n" } } });
    let (status, body) = post(port, &call.to_string(), None);
    assert_eq!(status, "HTTP/1.1 200 OK");
    let answer: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(answer["result"]["structuredContent"]["status"], "ok");
    let (status, _) = post(port, &call.to_string(), Some("https://evil.example"));
    assert_eq!(status, "HTTP/1.1 403 Forbidden");
    let (status, body) = post(
        port,
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
        Some("http://localhost:3000"),
    );
    assert_eq!(
        (status.as_str(), body.as_str()),
        ("HTTP/1.1 202 Accepted", "")
    );
    drop(server);
    std::fs::remove_dir_all(root).ok();
    // A non-loopback address is refused.
    let refused = Command::new(env!("CARGO_BIN_EXE_dedalo"))
        .args(["mcp", "--http", "0.0.0.0:0"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(3));
}
