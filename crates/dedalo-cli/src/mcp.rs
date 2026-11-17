//! `dedalo mcp`: the MCP server (spec 13.4).
//!
//! A small JSON-RPC server over stdio, or over Streamable HTTP on the
//! loopback interface. It is hand-written rather than built on an SDK, as
//! spec risk 6 allows: it offers only tools, resources and prompts, every
//! call is self-contained, and it answers both the stateless requests of MCP
//! revision 2026-07-28 and the `initialize` handshake of earlier revisions,
//! so older clients keep working. Paths are confined to one root directory.

use std::io::{BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};

use dedalo_core::markdown;
use dedalo_core::model::ops::{Batch, Op};
use dedalo_core::model::{Preset, Report};
use dedalo_core::{ExportSpec, Input, Registry};
use serde_json::{Value, json};

use crate::guide;

/// Revisions this server answers, newest first.
const VERSIONS: &[&str] = &["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"];
const INSTRUCTIONS: &str = "Dedalo compiles a text description of a tabletop-RPG dungeon into a map. Read the `quickstart` guide topic once, then write a file, validate it, apply the fixes the diagnostics carry, inspect the text map, render, and continue with small edit operations addressed by element ID. A path may also be a Markdown note: the dungeon is then a ```dedalo code block of the note, chosen with `block` when the note has several, and line numbers are the note's.";

pub struct Server {
    root: PathBuf,
    reg: Registry,
}

/// A tool failure, reported to the model as a tool result with `isError`.
struct ToolError(String);

type ToolResult = Result<Value, ToolError>;

fn text_result(summary: String, structured: Value) -> Value {
    json!({ "content": [{ "type": "text", "text": summary }], "structuredContent": structured, "isError": false })
}

fn str_arg<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str)
}

/// Text to compile, the file it came from, and the note and block when the
/// file is a Markdown note.
type Loaded = (String, Option<PathBuf>, Option<(String, markdown::Block)>);

fn block_arg(args: &Value) -> Result<Option<usize>, ToolError> {
    match args.get("block") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .filter(|n| *n >= 1)
            .map(|n| Some(n as usize))
            .ok_or_else(|| ToolError("`block` counts from 1".to_string())),
    }
}

fn summary_of(report: &Report) -> String {
    let status = match report.status {
        dedalo_core::model::Status::Ok => "ok",
        dedalo_core::model::Status::Warning => "warning",
        dedalo_core::model::Status::Error => "error",
    };
    let first: Vec<String> = report
        .diagnostics
        .iter()
        .take(3)
        .map(|d| format!("{}: {}", d.code.as_str(), d.message))
        .collect();
    let more = if report.diagnostics.len() > 3 {
        format!(
            "
(and {} more)",
            report.diagnostics.len() - 3
        )
    } else {
        String::new()
    };
    format!(
        "{status}: {} error(s), {} warning(s).{}{more}",
        report.summary.errors,
        report.summary.warnings,
        first
            .iter()
            .map(|l| format!(
                "
{l}"
            ))
            .collect::<String>()
    )
}

fn base64(bytes: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            out.push(if k <= chunk.len() {
                B64[(n >> (18 - 6 * k) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

fn tools() -> Value {
    let source_or_path = json!({
        "path": { "type": "string", "description": "A .dedalo file or a Markdown note, relative to the server's root" },
        "block": { "type": "integer", "minimum": 1, "description": "With a Markdown note, its ```dedalo code block to use, counted from 1; needed when the note has more than one" },
        "source": { "type": "string", "description": "DSL text, instead of a path" }
    });
    let with = |extra: Value| {
        let mut props = source_or_path.clone();
        if let (Some(p), Value::Object(e)) = (props.as_object_mut(), extra) {
            p.extend(e);
        }
        props
    };
    json!([
        {
            "name": "dedalo_guide",
            "title": "Dedalo language guide",
            "description": "Read a topic of the Dedalo reference, about 1,500 tokens each. Start with `quickstart`. Topics: quickstart, syntax, layout, catalog, catalog/<type>, errors, errors/<CODE>, examples, examples/<name>.",
            "inputSchema": { "type": "object", "properties": { "topic": { "type": "string", "default": "quickstart" } } },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "dedalo_validate",
            "title": "Validate a dungeon",
            "description": "Parse, check and lay out a dungeon. Returns every diagnostic with a stable code, the element at fault, a message with the allowed values and, where certain, a `fix` edit operation to pass to dedalo_edit.",
            "inputSchema": { "type": "object", "properties": source_or_path },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "dedalo_inspect",
            "title": "Inspect a dungeon",
            "description": "A compact view of a dungeon: `outline` (rooms with their map boxes, elements and connections), `text_map` (the map drawn with characters), `element` (one element's resolved geometry, needs `id`) or `resolved` (the whole resolved model, large).",
            "inputSchema": { "type": "object", "properties": with(json!({
                "view": { "type": "string", "enum": ["outline", "text_map", "element", "resolved"], "default": "outline" },
                "id": { "type": "string" },
                "preset": { "type": "string", "enum": ["dm", "player", "clean"], "default": "dm" }
            })) },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "dedalo_write",
            "title": "Write a dungeon file",
            "description": "Validate DSL text, then create or replace the file. The file is written even when there are errors, so they can be fixed with dedalo_edit; the diagnostics and the new content hash are returned. With a Markdown note, only the ```dedalo block changes: the one numbered `block`, else the only one, else a new block is added at the end of the note (created if missing); `fence` sets the options of a new block, such as `player`.",
            "inputSchema": { "type": "object", "properties": with(json!({
                "fence": { "type": "string", "description": "Options after ```dedalo for a new block of a note, such as `player level=ground`" }
            })), "required": ["path", "source"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "dedalo_edit",
            "title": "Edit a dungeon by element ID",
            "description": "Apply edit operations in order, atomically: add (a DSL fragment as `source`, or `kind`, `id`, `set`), update (`set` and `unset` dotted paths such as `placement.offset`), remove (`cascade` to take referrers too), move (`to`: room and position, or layout fields), connect. The file is written only if no new error appears. Returns status, diagnostics, a unified diff, the changed IDs and the new hash. Pass `base_hash` to refuse the edit if the file changed.",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string" },
                "block": { "type": "integer", "minimum": 1, "description": "With a Markdown note, its ```dedalo code block, counted from 1" },
                "ops": { "type": "array", "items": { "type": "object" }, "description": "Edit operations; `dedalo schema ops` describes them" },
                "dry_run": { "type": "boolean", "default": false },
                "base_hash": { "type": "string" }
            }, "required": ["path", "ops"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "dedalo_render",
            "title": "Render a dungeon map",
            "description": "Render a map as an image (PNG, long edge capped at `max_px`), as text (`txt`), or as SVG text. With `out`, write the file (any format, including pdf) under the root and return its path.",
            "inputSchema": { "type": "object", "properties": with(json!({
                "preset": { "type": "string", "enum": ["dm", "player", "clean"] },
                "export": { "type": "string", "description": "A named export block of the document" },
                "format": { "type": "string", "enum": ["png", "svg", "txt", "pdf"], "default": "png" },
                "rotation": { "type": "integer", "enum": [0, 90, 180, 270] },
                "level": { "type": "string", "description": "One level of a multi-level dungeon; all levels side by side when absent" },
                "language": { "type": "string", "enum": ["en", "it"], "description": "Language of default legend labels" },
                "max_px": { "type": "integer", "default": 1024, "minimum": 64, "maximum": 4096 },
                "out": { "type": "string", "description": "Where to write the output, relative to the root" }
            })) },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false }
        }
    ])
}

const PROMPTS: &[(&str, &str)] = &[
    (
        "new_dungeon",
        "Design a new dungeon from a brief and write it to a file",
    ),
    (
        "review_dungeon",
        "Review a dungeon file for errors, layout problems and secrets that leak to players",
    ),
];

impl Server {
    pub fn new(root: &Path, reg: Registry) -> Result<Server, String> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("cannot open the root `{}`: {e}", root.display()))?;
        Ok(Server { root, reg })
    }

    /// Resolves a path argument inside the root, through symbolic links.
    fn resolve(&self, path: &str) -> Result<PathBuf, ToolError> {
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(ToolError(format!(
                "`{path}` must be a path relative to the root, without `..`"
            )));
        }
        let full = self.root.join(relative);
        // The deepest existing ancestor must stay inside the root.
        let mut existing = full.clone();
        while !existing.exists() {
            if !existing.pop() {
                break;
            }
        }
        let real = existing
            .canonicalize()
            .map_err(|e| ToolError(format!("cannot resolve `{path}`: {e}")))?;
        if !real.starts_with(&self.root) {
            return Err(ToolError(format!("`{path}` leads outside the root")));
        }
        Ok(full)
    }

    fn source(&self, args: &Value) -> Result<(String, Option<PathBuf>), ToolError> {
        let (text, full, _) = self.load(args)?;
        Ok((text, full))
    }

    /// The text to compile and, for a block of a note, the note and block.
    /// A note's text is masked: the note with every other line emptied.
    fn load(&self, args: &Value) -> Result<Loaded, ToolError> {
        match (str_arg(args, "source"), str_arg(args, "path")) {
            (Some(text), _) => Ok((text.to_string(), None, None)),
            (None, Some(path)) => {
                let full = self.resolve(path)?;
                let text = std::fs::read_to_string(&full)
                    .map_err(|e| ToolError(format!("cannot read `{path}`: {e}")))?;
                if !markdown::is_markdown(&full) {
                    return Ok((text, Some(full), None));
                }
                let block = markdown::select(&text, block_arg(args)?)
                    .map_err(|e| ToolError(format!("{path}: {e}; pass `block`")))?;
                Ok((block.masked(&text), Some(full), Some((text, block))))
            }
            (None, None) => Err(ToolError("pass `path` or `source`".to_string())),
        }
    }

    fn compile(&self, text: &str) -> dedalo_core::Compiled {
        let input = if text.trim_start().starts_with('{') {
            Input::Json(text)
        } else {
            Input::Dsl(text)
        };
        dedalo_core::compile(input, &self.reg)
    }

    fn call_tool(&self, name: &str, args: &Value) -> ToolResult {
        match name {
            "dedalo_guide" => {
                let topic = str_arg(args, "topic").unwrap_or("quickstart");
                let text = guide::topic(topic, &self.reg).map_err(ToolError)?;
                Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": false }))
            }
            "dedalo_validate" => {
                let whole_note = str_arg(args, "path")
                    .filter(|p| {
                        str_arg(args, "source").is_none() && markdown::is_markdown(Path::new(p))
                    })
                    .filter(|_| args.get("block").is_none_or(Value::is_null));
                let report = match whole_note {
                    // Every block of the note; line numbers tell them apart.
                    Some(path) => {
                        let full = self.resolve(path)?;
                        let text = std::fs::read_to_string(&full)
                            .map_err(|e| ToolError(format!("cannot read `{path}`: {e}")))?;
                        let blocks = markdown::blocks(&text);
                        if blocks.is_empty() {
                            return Err(ToolError(format!(
                                "{path}: the note has no ```dedalo code block"
                            )));
                        }
                        Report::new(
                            blocks
                                .iter()
                                .flat_map(|b| self.compile(&b.masked(&text)).diagnostics)
                                .collect(),
                        )
                    }
                    None => {
                        let (text, _) = self.source(args)?;
                        self.compile(&text).report()
                    }
                };
                Ok(text_result(
                    summary_of(&report),
                    serde_json::to_value(&report).unwrap_or_default(),
                ))
            }
            "dedalo_inspect" => {
                let (text, _) = self.source(args)?;
                let compiled = self.compile(&text);
                let resolved = compiled
                    .resolved
                    .as_ref()
                    .ok_or_else(|| ToolError(summary_of(&compiled.report())))?;
                match str_arg(args, "view").unwrap_or("outline") {
                    "outline" => {
                        let outline = crate::outline::text(resolved, compiled.document.as_ref());
                        let mut value = crate::outline::json(resolved, compiled.document.as_ref());
                        value["diagnostics"] =
                            serde_json::to_value(compiled.report()).unwrap_or_default();
                        Ok(text_result(outline, value))
                    }
                    "text_map" => {
                        let preset = str_arg(args, "preset")
                            .and_then(Preset::parse)
                            .unwrap_or(Preset::Dm);
                        let mut spec = ExportSpec::preset(preset);
                        spec.format = "txt".to_string();
                        let rendered = dedalo_core::render(&compiled, &spec, &self.reg)
                            .map_err(|d| ToolError(d.message))?;
                        let map = String::from_utf8_lossy(&rendered.bytes).into_owned();
                        Ok(text_result(
                            map.clone(),
                            json!({ "text_map": map, "report": compiled.report() }),
                        ))
                    }
                    "element" => {
                        let id = str_arg(args, "id")
                            .ok_or_else(|| ToolError("`view = element` needs `id`".to_string()))?;
                        let value = crate::outline::element(resolved, id).ok_or_else(|| {
                            ToolError(format!("no resolved element has the ID `{id}`"))
                        })?;
                        Ok(text_result(
                            serde_json::to_string(&value).unwrap_or_default(),
                            value,
                        ))
                    }
                    "resolved" => {
                        let value = serde_json::to_value(resolved).unwrap_or_default();
                        Ok(text_result(
                            format!("The resolved model of `{}`.", resolved.id),
                            value,
                        ))
                    }
                    other => Err(ToolError(format!(
                        "`{other}` is not a view; use outline, text_map, element or resolved"
                    ))),
                }
            }
            "dedalo_write" => {
                let path =
                    str_arg(args, "path").ok_or_else(|| ToolError("pass `path`".to_string()))?;
                let text = str_arg(args, "source")
                    .ok_or_else(|| ToolError("pass `source`".to_string()))?;
                let full = self.resolve(path)?;
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        ToolError(format!("cannot create the folder of `{path}`: {e}"))
                    })?;
                }
                let mut checked = text.to_string();
                let bytes = if markdown::is_markdown(&full) {
                    // Only the dungeon's block of the note changes.
                    let note = std::fs::read_to_string(&full).unwrap_or_default();
                    let count = markdown::blocks(&note).len();
                    let chosen = block_arg(args)?;
                    let next = match (chosen, count) {
                        (Some(_), _) | (None, 1) => {
                            let block = markdown::select(&note, chosen)
                                .map_err(|e| ToolError(format!("{path}: {e}")))?;
                            markdown::replace(&note, &block, text)
                        }
                        (None, 0) => {
                            markdown::append(&note, text, str_arg(args, "fence").unwrap_or(""))
                        }
                        (None, n) => {
                            return Err(ToolError(format!(
                                "{path}: the note has {n} ```dedalo blocks; pass `block` to replace one"
                            )));
                        }
                    };
                    let index = chosen.unwrap_or(count.max(1));
                    if let Ok(block) = markdown::select(&next, Some(index)) {
                        checked = block.masked(&next);
                    }
                    next
                } else {
                    text.to_string()
                };
                std::fs::write(&full, &bytes)
                    .map_err(|e| ToolError(format!("cannot write `{path}`: {e}")))?;
                let text = checked.as_str();
                let report = self.compile(text).report();
                let hash = dedalo_core::content_hash(text);
                let mut value = serde_json::to_value(&report).unwrap_or_default();
                value["hash"] = json!(hash);
                value["path"] = json!(path);
                Ok(text_result(
                    format!("Wrote {path} ({hash}). {}", summary_of(&report)),
                    value,
                ))
            }
            "dedalo_edit" => {
                let path =
                    str_arg(args, "path").ok_or_else(|| ToolError("pass `path`".to_string()))?;
                let (text, _, note) =
                    self.load(&json!({ "path": path, "block": args.get("block") }))?;
                let full = self.resolve(path)?;
                let ops: Vec<Op> = serde_json::from_value(
                    args.get("ops").cloned().unwrap_or(json!([])),
                )
                .map_err(|e| ToolError(format!("`ops` is not a list of edit operations: {e}")))?;
                let batch = Batch {
                    base_hash: str_arg(args, "base_hash").map(str::to_string),
                    ops,
                };
                let result = dedalo_core::edit(&text, &batch, &self.reg, false, path);
                let dry = args
                    .get("dry_run")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let written = result.applied && !dry && result.source != text;
                if written {
                    let bytes = match &note {
                        Some((note, block)) => markdown::replace(note, block, &result.source),
                        None => result.source.clone(),
                    };
                    std::fs::write(&full, bytes)
                        .map_err(|e| ToolError(format!("cannot write `{path}`: {e}")))?;
                }
                let mut value = serde_json::to_value(&result).unwrap_or_default();
                value["written"] = json!(written);
                let verdict = if !result.applied {
                    "Not applied"
                } else if written {
                    "Applied"
                } else if dry {
                    "Dry run"
                } else {
                    "No change"
                };
                let report = Report::new(result.diagnostics.clone());
                let summary = format!(
                    "{verdict}: {}. {}\n{}",
                    result.changed.join(", "),
                    summary_of(&report),
                    result.diff
                );
                Ok(
                    json!({ "content": [{ "type": "text", "text": summary }], "structuredContent": value, "isError": !result.applied }),
                )
            }
            "dedalo_render" => self.render(args),
            other => Err(ToolError(format!("there is no tool named `{other}`"))),
        }
    }

    fn render(&self, args: &Value) -> ToolResult {
        let (text, _, note) = self.load(args)?;
        let compiled = self.compile(&text);
        let report = compiled.report();
        if report.has_errors() {
            return Err(ToolError(format!(
                "The dungeon has errors; fix them first.\n{}",
                summary_of(&report)
            )));
        }
        let block = note.as_ref().map(|(_, b)| b);
        let export = str_arg(args, "export").or_else(|| block.and_then(|b| b.option("export")));
        let mut spec =
            dedalo_core::export_spec(&compiled, export).map_err(|d| ToolError(d.message))?;
        if let Some(block) = block {
            // The fence's options, which the arguments below override.
            let levels: Vec<&str> = compiled
                .document
                .iter()
                .flat_map(|d| d.dungeon.levels.iter().map(|l| l.id.as_str()))
                .collect();
            block.apply(&mut spec, &levels).map_err(ToolError)?;
        }
        if let Some(p) = str_arg(args, "preset") {
            spec.preset =
                Preset::parse(p).ok_or_else(|| ToolError(format!("`{p}` is not a preset")))?;
        }
        if let Some(r) = args.get("rotation").and_then(Value::as_i64) {
            spec.rotation = r;
        }
        if let Some(lang) = str_arg(args, "language") {
            spec.language = dedalo_core::model::Language::parse(lang)
                .ok_or_else(|| ToolError(format!("`{lang}` is not a language; use en or it")))?;
        }
        if let Some(level) = str_arg(args, "level") {
            let known: Vec<&str> = compiled
                .document
                .iter()
                .flat_map(|d| d.dungeon.levels.iter().map(|l| l.id.as_str()))
                .collect();
            if !known.contains(&level) {
                return Err(ToolError(format!(
                    "`{level}` is not a level of this dungeon. Its levels: {}.",
                    if known.is_empty() {
                        "none".to_string()
                    } else {
                        known.join(", ")
                    }
                )));
            }
            spec.level = Some(level.to_string());
        }
        let format = str_arg(args, "format").unwrap_or("png").to_string();
        spec.format = format.clone();
        let out = str_arg(args, "out");
        if format == "png" && out.is_none() {
            // Fit the long edge into `max_px`, for the model's eyes.
            let max_px = args
                .get("max_px")
                .and_then(Value::as_u64)
                .unwrap_or(1024)
                .clamp(64, 4096) as f64;
            let mut probe = spec.clone();
            probe.format = "svg".to_string();
            let scene = dedalo_core::render(&compiled, &probe, &self.reg)
                .map_err(|d| ToolError(d.message))?
                .scene;
            let longest = scene.bounds[2].max(scene.bounds[3]).max(1.0);
            spec.cell_px = ((max_px / longest).floor() as u32).clamp(4, 512);
        }
        let rendered =
            dedalo_core::render(&compiled, &spec, &self.reg).map_err(|d| ToolError(d.message))?;
        if let Some(out) = out {
            let full = self.resolve(out)?;
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ToolError(format!("cannot create the folder of `{out}`: {e}")))?;
            }
            std::fs::write(&full, &rendered.bytes)
                .map_err(|e| ToolError(format!("cannot write `{out}`: {e}")))?;
            return Ok(text_result(
                format!("Wrote {out} ({} bytes).", rendered.bytes.len()),
                json!({ "path": out, "bytes": rendered.bytes.len(), "media_type": rendered.media_type }),
            ));
        }
        match format.as_str() {
            "png" => Ok(json!({
                "content": [
                    { "type": "image", "data": base64(&rendered.bytes), "mimeType": "image/png" },
                    { "type": "text", "text": format!("{} map of `{}`, {} pixels per cell.", spec.preset.as_str(), rendered.view.title.clone().unwrap_or_default(), spec.cell_px) }
                ],
                "isError": false
            })),
            "pdf" => Err(ToolError(
                "PDF output needs `out`, a file to write".to_string(),
            )),
            _ => {
                let body = String::from_utf8_lossy(&rendered.bytes).into_owned();
                Ok(json!({ "content": [{ "type": "text", "text": body }], "isError": false }))
            }
        }
    }

    fn resources(&self) -> Vec<(String, String, String, &'static str)> {
        let mut out: Vec<(String, String, String, &'static str)> = guide::topics(&self.reg.catalog)
            .into_iter()
            .map(|(name, desc)| {
                (
                    format!("dedalo://guide/{name}"),
                    name,
                    desc,
                    "text/markdown",
                )
            })
            .collect();
        for name in dedalo_core::model::SCHEMA_NAMES {
            out.push((
                format!("dedalo://schema/{name}"),
                format!("{name} schema"),
                format!("The JSON Schema of `{name}`"),
                "application/schema+json",
            ));
        }
        out.push((
            "dedalo://catalog".to_string(),
            "catalog".to_string(),
            "The element catalog as JSON".to_string(),
            "application/json",
        ));
        out
    }

    fn read_resource(&self, uri: &str) -> Result<(String, &'static str), String> {
        if let Some(topic) = uri.strip_prefix("dedalo://guide/") {
            return guide::topic(topic, &self.reg).map(|t| (t, "text/markdown"));
        }
        if let Some(name) = uri.strip_prefix("dedalo://schema/") {
            return dedalo_core::model::schema(name)
                .map(|s| {
                    (
                        serde_json::to_string_pretty(&s).unwrap_or_default(),
                        "application/schema+json",
                    )
                })
                .ok_or(format!("no schema `{name}`"));
        }
        if uri == "dedalo://catalog" {
            return Ok((
                serde_json::to_string_pretty(&self.reg.catalog).unwrap_or_default(),
                "application/json",
            ));
        }
        Err(format!("no resource `{uri}`"))
    }

    fn prompt(&self, name: &str, args: &Value) -> Result<Value, String> {
        let text = match name {
            "new_dungeon" => {
                let brief =
                    str_arg(args, "brief").unwrap_or("a small dungeon of four to six rooms");
                let path = str_arg(args, "path").unwrap_or("dungeon.dedalo");
                format!(
                    "Design {brief}. Read the dedalo_guide topic `quickstart` first, and `layout` if rooms must be placed precisely. Write the file `{path}` with dedalo_write, then validate, apply the fixes the diagnostics carry with dedalo_edit, check the text map with dedalo_inspect, and render a DM and a player map with dedalo_render. Prefer relative placement and wall or anchor placement over coordinates."
                )
            }
            "review_dungeon" => {
                let path = str_arg(args, "path").unwrap_or("dungeon.dedalo");
                format!(
                    "Review `{path}`. Run dedalo_validate and explain each diagnostic. Inspect the outline and the DM and player text maps, and check that nothing the players must not see (traps, secret doors, hidden rooms, DM notes) appears on the player map. Propose fixes as edit operations, and apply them with dedalo_edit only after listing them."
                )
            }
            other => return Err(format!("no prompt `{other}`")),
        };
        Ok(
            json!({ "description": PROMPTS.iter().find(|p| p.0 == name).map(|p| p.1), "messages": [{ "role": "user", "content": { "type": "text", "text": text } }] }),
        )
    }

    fn capabilities() -> Value {
        json!({ "tools": { "listChanged": false }, "resources": { "listChanged": false, "subscribe": false }, "prompts": { "listChanged": false } })
    }

    fn server_info() -> Value {
        json!({ "name": "dedalo", "title": "Dedalo", "version": env!("CARGO_PKG_VERSION") })
    }

    /// Answers one JSON-RPC message. `None` for a notification.
    pub fn handle(&self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(json!({}));
        let id = id?; // notifications get no answer
        let result: Result<Value, (i64, String)> = match method {
            "initialize" => {
                let asked = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or(VERSIONS[0]);
                let version = if VERSIONS.contains(&asked) {
                    asked
                } else {
                    VERSIONS[0]
                };
                Ok(
                    json!({ "protocolVersion": version, "capabilities": Self::capabilities(), "serverInfo": Self::server_info(), "instructions": INSTRUCTIONS }),
                )
            }
            "server/discover" => Ok(json!({
                "supportedVersions": VERSIONS,
                "protocolVersion": VERSIONS[0],
                "capabilities": Self::capabilities(),
                "serverInfo": Self::server_info(),
                "instructions": INSTRUCTIONS
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools(), "ttlMs": 3_600_000 })),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                if !tools()
                    .as_array()
                    .is_some_and(|t| t.iter().any(|x| x["name"] == name))
                {
                    Err((-32602, format!("unknown tool `{name}`")))
                } else {
                    Ok(match self.call_tool(name, &args) {
                        Ok(value) => value,
                        Err(ToolError(message)) => {
                            json!({ "content": [{ "type": "text", "text": message }], "isError": true })
                        }
                    })
                }
            }
            "resources/list" => Ok(json!({
                "resources": self.resources().into_iter().map(|(uri, name, description, mime)| json!({ "uri": uri, "name": name, "description": description, "mimeType": mime })).collect::<Vec<_>>(),
                "ttlMs": 3_600_000
            })),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
            "resources/read" => {
                let uri = params
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                self.read_resource(uri).map(|(text, mime)| json!({ "contents": [{ "uri": uri, "mimeType": mime, "text": text }], "ttlMs": 3_600_000 })).map_err(|e| (-32002, e))
            }
            "prompts/list" => Ok(json!({
                "prompts": PROMPTS.iter().map(|(name, description)| json!({
                    "name": name,
                    "description": description,
                    "arguments": if *name == "new_dungeon" {
                        json!([{ "name": "brief", "description": "What the dungeon should be", "required": false }, { "name": "path", "description": "The file to write", "required": false }])
                    } else {
                        json!([{ "name": "path", "description": "The file to review", "required": true }])
                    }
                })).collect::<Vec<_>>()
            })),
            "prompts/get" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                self.prompt(name, &params.get("arguments").cloned().unwrap_or(json!({})))
                    .map_err(|e| (-32602, e))
            }
            other => Err((-32601, format!("method `{other}` is not supported"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
            }
        })
    }

    fn handle_text(&self, line: &str) -> Option<Value> {
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Array(batch)) => {
                let answers: Vec<Value> = batch.iter().filter_map(|m| self.handle(m)).collect();
                (!answers.is_empty()).then_some(Value::Array(answers))
            }
            Ok(message) => self.handle(&message),
            Err(e) => Some(
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("parse error: {e}") } }),
            ),
        }
    }

    /// Serves newline-delimited JSON-RPC on standard input and output.
    pub fn serve_stdio(&self) -> std::io::Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout().lock();
        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(answer) = self.handle_text(&line) {
                writeln!(stdout, "{answer}")?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    /// Serves Streamable HTTP on a loopback address: each POST carries one
    /// message and gets a JSON answer.
    pub fn serve_http(&self, addr: &str) -> Result<(), String> {
        let listener = std::net::TcpListener::bind(addr)
            .map_err(|e| format!("cannot listen on {addr}: {e}"))?;
        let local = listener.local_addr().map_err(|e| e.to_string())?;
        if !local.ip().is_loopback() {
            return Err(format!(
                "{addr} is not a loopback address; the MCP server listens on 127.0.0.1 or ::1 only"
            ));
        }
        eprintln!("dedalo mcp listening on http://{local}/mcp");
        for stream in listener.incoming().flatten() {
            if let Err(e) = self.http_exchange(stream) {
                eprintln!("dedalo mcp: {e}");
            }
        }
        Ok(())
    }

    fn http_exchange(&self, mut stream: std::net::TcpStream) -> std::io::Result<()> {
        stream.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
        let mut reader = std::io::BufReader::new(stream.try_clone()?);
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
            }
        }
        let header = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        let respond = |stream: &mut std::net::TcpStream, status: &str, body: &str| {
            let kind = if body.is_empty() {
                ""
            } else {
                "Content-Type: application/json\r\n"
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\n{kind}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        };
        let len: usize = header("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if len > 8 * 1024 * 1024 {
            return respond(&mut stream, "413 Payload Too Large", "");
        }
        // The body is read before any answer: closing a socket with unread
        // data resets the connection instead of delivering the answer.
        let mut body = vec![0; len];
        reader.read_exact(&mut body)?;
        // A browser page from another site must not reach the server.
        if let Some(origin) = header("origin") {
            let host = origin.split("://").nth(1).unwrap_or_default();
            let host = if host.starts_with('[') {
                host.split(']')
                    .next()
                    .map(|h| format!("{h}]"))
                    .unwrap_or_default()
            } else {
                host.split(':').next().unwrap_or_default().to_string()
            };
            if !matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]") {
                return respond(&mut stream, "403 Forbidden", "");
            }
        }
        if !request_line.starts_with("POST ") {
            return respond(&mut stream, "405 Method Not Allowed", "");
        }
        match self.handle_text(&String::from_utf8_lossy(&body)) {
            Some(answer) => respond(&mut stream, "200 OK", &answer.to_string()),
            None => respond(&mut stream, "202 Accepted", ""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server on a root of its own: tests run in parallel and each one
    /// removes its root when done.
    fn server(name: &str) -> (Server, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("dedalo-mcp-unit-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        (Server::new(&dir, Registry::builtin()).unwrap(), dir)
    }

    #[test]
    fn paths_stay_inside_the_root() {
        let (s, dir) = server("paths");
        assert!(s.resolve("a/b.dedalo").is_ok());
        assert!(s.resolve("../x.dedalo").is_err());
        assert!(s.resolve(&dir.join("x").display().to_string()).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn both_handshakes_are_answered() {
        let (s, dir) = server("handshakes");
        let old = s.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } })).unwrap();
        assert_eq!(old["result"]["protocolVersion"], "2025-06-18");
        let new = s
            .handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "server/discover" }))
            .unwrap();
        assert_eq!(new["result"]["supportedVersions"][0], "2026-07-28");
        assert!(
            s.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .is_none()
        );
        let tools = s
            .handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }))
            .unwrap();
        assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 6);
        std::fs::remove_dir_all(dir).ok();
    }

    fn call(s: &Server, name: &str, args: Value) -> Value {
        s.call_tool(name, &args)
            .unwrap_or_else(|e| panic!("{name}: {}", e.0))
    }

    #[test]
    fn dungeons_live_in_markdown_notes() {
        let (s, dir) = server("markdown");
        let dungeon =
            "dungeon \"d\" {\n    room \"a\" {\n        shape = rectangle(4, 4)\n    }\n}\n";
        // A new note gets a block; the prose of an existing one is kept.
        call(
            &s,
            "dedalo_write",
            json!({ "path": "Maps/Crypt.md", "source": dungeon, "fence": "player" }),
        );
        let note = std::fs::read_to_string(dir.join("Maps/Crypt.md")).unwrap();
        assert_eq!(note, format!("```dedalo player\n{dungeon}```\n"));
        std::fs::write(
            dir.join("Maps/Crypt.md"),
            format!("# Crypt\n\nIntro.\n\n{note}\nOutro.\n"),
        )
        .unwrap();
        let broken = dungeon.replace("rectangle(4, 4)", "rectangle(4, 4) colour = red");
        let written = call(
            &s,
            "dedalo_write",
            json!({ "path": "Maps/Crypt.md", "source": broken }),
        );
        // The error is reported on the note's own line.
        let line = &written["structuredContent"]["diagnostics"][0]["span"]["start"][0];
        assert_eq!(line, 8, "{written}");
        let note = std::fs::read_to_string(dir.join("Maps/Crypt.md")).unwrap();
        assert!(note.starts_with("# Crypt\n\nIntro.\n\n```dedalo player\n"));
        assert!(note.ends_with("```\n\nOutro.\n"));
        call(
            &s,
            "dedalo_write",
            json!({ "path": "Maps/Crypt.md", "source": dungeon }),
        );
        let checked = call(&s, "dedalo_validate", json!({ "path": "Maps/Crypt.md" }));
        assert_eq!(checked["structuredContent"]["status"], "ok", "{checked}");
        let edited = call(
            &s,
            "dedalo_edit",
            json!({ "path": "Maps/Crypt.md", "ops": [{ "op": "update", "id": "a", "set": { "name": "Hall" } }] }),
        );
        assert_eq!(edited["structuredContent"]["written"], true, "{edited}");
        let note = std::fs::read_to_string(dir.join("Maps/Crypt.md")).unwrap();
        assert!(note.contains("name = \"Hall\""), "{note}");
        assert!(note.ends_with("```\n\nOutro.\n"), "{note}");
        let map = call(
            &s,
            "dedalo_render",
            json!({ "path": "Maps/Crypt.md", "format": "txt" }),
        );
        assert!(!map["content"][0]["text"].as_str().unwrap().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }
}
