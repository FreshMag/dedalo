//! The `dedalo` binary (spec section 12).
//!
//! This host adds file I/O, argument parsing and human-readable diagnostics
//! to the core. It holds no map logic. Standard output carries only the
//! result; diagnostics and logs go to standard error. No command prompts.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use dedalo_core::model::diag::Span;
use dedalo_core::model::{Code, Diagnostic, Preset, Report, Severity};
use dedalo_core::{Compiled, ExportSpec, Input, Registry};
use serde_json::json;

mod guide;
mod mcp;
mod outline;
mod plugins;

#[derive(Parser, Clone)]
#[command(
    name = "dedalo",
    version,
    about = "Compile a dungeon description into a map."
)]
struct Cli {
    /// Print the machine-readable form on standard output.
    #[arg(long, global = true)]
    json: bool,
    /// Load a plugin package, or every package in a folder. Repeatable.
    #[arg(long = "plugin", global = true, value_name = "PATH")]
    plugin_paths: Vec<PathBuf>,
    /// Record the plugins in use in `dedalo.lock` instead of checking them.
    #[arg(long, global = true)]
    update_lock: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
enum Command {
    /// Compile and write a map.
    Render {
        /// A .dedalo or .json file, or `-` for standard input.
        file: String,
        /// Output path, or `-` for standard output.
        #[arg(short, long)]
        output: Option<String>,
        /// Render the `export` block with this name.
        #[arg(long)]
        export: Option<String>,
        /// Render every `export` block of the document.
        #[arg(long, conflicts_with = "export")]
        all: bool,
        /// dm, player or clean.
        #[arg(long)]
        preset: Option<String>,
        /// svg or txt (png and pdf arrive with milestone M3).
        #[arg(long)]
        format: Option<String>,
        /// Pixels per cell, 16 to 512.
        #[arg(long)]
        cell_px: Option<u32>,
        /// 0, 90, 180 or 270 degrees clockwise.
        #[arg(long)]
        rotation: Option<i64>,
        #[arg(long)]
        theme: Option<String>,
        /// Print a stage output instead of the map: document, resolved, view or scene.
        #[arg(long)]
        emit: Option<String>,
        /// Render whatever resolved even when there are errors.
        #[arg(long)]
        force: bool,
        /// Render again whenever the file changes, until interrupted.
        #[arg(long)]
        watch: bool,
    },
    /// Run parse, lower and resolve; print diagnostics.
    Validate {
        file: String,
        /// Apply every fix the diagnostics propose, then report what is left.
        #[arg(long)]
        fix: bool,
    },
    /// Apply edit operations addressed by element ID.
    Edit {
        file: String,
        /// A JSON file with the operations, or `-` for standard input: either
        /// `{ "base_hash": ..., "ops": [...] }` or a bare array of operations.
        #[arg(long)]
        ops: String,
        /// Report the result without writing the file.
        #[arg(long)]
        dry_run: bool,
        /// Refuse the edit unless the file still has this content hash.
        #[arg(long)]
        base_hash: Option<String>,
        /// Write the result even if it introduces errors.
        #[arg(long)]
        force: bool,
    },
    /// Format in place.
    Fmt {
        file: String,
        /// Exit with 1 when the file is not formatted; write nothing.
        #[arg(long)]
        check: bool,
    },
    /// Switch between DSL and JSON.
    Convert {
        file: String,
        /// json or dsl.
        #[arg(long)]
        to: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Print an outline of rooms, connections and counts.
    Inspect {
        file: String,
        /// Print one element's resolved data.
        #[arg(long)]
        id: Option<String>,
        /// Print a stage output: document, resolved, view or scene.
        #[arg(long)]
        stage: Option<String>,
        /// Print the map as text.
        #[arg(long)]
        text_map: bool,
        /// Audience of the text map, view and scene: dm, player or clean.
        #[arg(long, default_value = "dm")]
        preset: String,
    },
    /// List types, subtypes and property schemas.
    Catalog { query: Option<String> },
    /// Print a JSON Schema: document, ops or diagnostics.
    Schema {
        name: String,
        /// `full` (default) or `authoring`: a closed, inlined profile of the
        /// document schema for model structured output.
        #[arg(long, default_value = "full")]
        profile: String,
    },
    /// Describe a diagnostic code with a worked fix.
    Explain { code: String },
    /// Print the built-in language reference.
    Guide { topic: Option<String> },
    /// Start the MCP server, on standard input and output by default.
    Mcp {
        /// The workspace directory every path argument is confined to.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Serve Streamable HTTP on this loopback address instead, such as 127.0.0.1:7410.
        #[arg(long)]
        http: Option<String>,
    },
    /// Plugin tools: add, list, remove, new, check.
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
    /// Agent skill tools.
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// Write a starter file.
    New {
        /// The example to start from: starter or forgotten_crypt.
        #[arg(long, default_value = "starter")]
        example: String,
        /// Output path, or `-` for standard output.
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[derive(Subcommand, Clone)]
enum PluginAction {
    /// Install a plugin package into `.dedalo/plugins/` and record it in `dedalo.lock`.
    Add { path: PathBuf },
    /// List the plugins found and what they contribute.
    List,
    /// Remove an installed plugin and its lockfile entry.
    Remove { id: String },
    /// Write a starter plugin package.
    New {
        id: String,
        /// theme, catalog or wasm.
        #[arg(long, default_value = "theme")]
        kind: String,
        /// The folder to write; default `./<id>`.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Run the conformance suite on a plugin package.
    Check { path: PathBuf },
}

#[derive(Subcommand, Clone)]
enum SkillAction {
    /// Write the agent skill folder: SKILL.md and its reference files.
    Export {
        dir: PathBuf,
        /// Also write `llms.txt`, the reference as one Markdown file.
        #[arg(long)]
        llms_txt: Option<PathBuf>,
    },
}

/// A failure that ends the command, with its exit code (spec section 12).
enum Failure {
    /// Diagnostics contain at least one error.
    Diagnostics,
    Usage(String),
    Io(String),
}

type Outcome = Result<(), Failure>;

const EXAMPLE_CRYPT: &str = include_str!("../../../tests/corpus/forgotten_crypt/input.dedalo");
const STARTER: &str = "dungeon \"my_dungeon\" {\n    name = \"My Dungeon\"\n\n    room \"entrance\" {\n        name = \"Entrance\"\n        shape = rectangle(8, 6)\n    }\n}\n";

fn main() -> ExitCode {
    let cli = Cli::parse();
    // A large stack: deep source files and plugin scenes recurse.
    let outcome = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || run(&cli))
        .map(|t| {
            t.join()
                .unwrap_or_else(|_| Err(Failure::Io("internal error".to_string())))
        })
        .unwrap_or_else(|e| Err(Failure::Io(format!("cannot start: {e}"))));
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Diagnostics) => ExitCode::from(1),
        Err(Failure::Usage(message)) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
        Err(Failure::Io(message)) => {
            eprintln!("error: {message}");
            ExitCode::from(3)
        }
    }
}

struct Source {
    /// The name shown in diagnostics.
    name: String,
    path: Option<PathBuf>,
    text: String,
    json: bool,
}

fn read(file: &str) -> Result<Source, Failure> {
    if file == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| Failure::Io(format!("cannot read standard input: {e}")))?;
        let json = text.trim_start().starts_with('{');
        return Ok(Source {
            name: "<stdin>".to_string(),
            path: None,
            text,
            json,
        });
    }
    let path = PathBuf::from(file);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| Failure::Io(format!("cannot read `{file}`: {e}")))?;
    let json = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    Ok(Source {
        name: file.to_string(),
        path: Some(path),
        text,
        json,
    })
}

fn write(target: Option<&str>, bytes: &[u8]) -> Outcome {
    match target {
        None | Some("-") => std::io::stdout()
            .write_all(bytes)
            .map_err(|e| Failure::Io(format!("cannot write to standard output: {e}"))),
        Some(path) => std::fs::write(path, bytes)
            .map_err(|e| Failure::Io(format!("cannot write `{path}`: {e}"))),
    }
}

fn compile(source: &Source, reg: &Registry) -> Compiled {
    let input = if source.json {
        Input::Json(&source.text)
    } else {
        Input::Dsl(&source.text)
    };
    let mut compiled = dedalo_core::compile(input, reg);
    // The plugins in use must match the lockfile beside the document.
    let dir = source.path.as_ref().and_then(|p| p.parent()).map(|p| {
        if p.as_os_str().is_empty() {
            Path::new(".")
        } else {
            p
        }
    });
    if let (Some(dir), Some(doc)) = (dir, &compiled.document) {
        let required: Vec<String> = doc
            .dedalo
            .iter()
            .flat_map(|h| h.plugins.iter().map(|p| p.id.clone()))
            .collect();
        let update = UPDATE_LOCK.load(std::sync::atomic::Ordering::Relaxed);
        match plugins::check_lock(dir, &required, reg, update) {
            Ok(problems) => compiled.diagnostics.extend(
                problems
                    .into_iter()
                    .map(|m| Diagnostic::new(Code::PluginFailed, m)),
            ),
            Err(e) => compiled
                .diagnostics
                .push(Diagnostic::new(Code::PluginFailed, e)),
        }
    }
    for d in &mut compiled.diagnostics {
        if let Some(span) = &mut d.span {
            span.file = Some(source.name.clone());
        }
    }
    compiled
}

/// Prints diagnostics for a person: the record with a source excerpt.
fn print_human(diagnostics: &[Diagnostic], source: &Source) {
    let lines: Vec<&str> = source.text.lines().collect();
    for d in diagnostics {
        let level = if d.severity == Severity::Error {
            "error"
        } else {
            "warning"
        };
        eprintln!("{level}[{}]: {}", d.code.as_str(), d.message);
        if let Some(Span { start, end, .. }) = &d.span {
            eprintln!("  --> {}:{}:{}", source.name, start[0], start[1]);
            if let Some(text) = lines.get(start[0] as usize - 1).filter(|_| !source.json) {
                let gutter = start[0].to_string();
                let from = start[1] as usize - 1;
                let to = if end[0] == start[0] {
                    (end[1] as usize - 1).max(from + 1)
                } else {
                    text.chars().count().max(from + 1)
                };
                eprintln!("{} |", " ".repeat(gutter.len()));
                eprintln!("{gutter} | {text}");
                eprintln!(
                    "{} | {}{}",
                    " ".repeat(gutter.len()),
                    " ".repeat(from),
                    "^".repeat(to - from)
                );
            }
        }
        for related in &d.related {
            eprintln!("  = `{}` {}", related.element, related.message);
        }
        if let Some(suggestion) = &d.suggestion {
            eprintln!("  = help: {suggestion}");
        }
        if let Some(fix) = &d.fix {
            eprintln!(
                "  = fix: {}",
                serde_json::to_string(fix).unwrap_or_default()
            );
        }
        eprintln!();
    }
    let report = Report::new(diagnostics.to_vec());
    if !diagnostics.is_empty() {
        eprintln!(
            "{} error(s), {} warning(s) in {}",
            report.summary.errors, report.summary.warnings, source.name
        );
    }
}

fn print_json(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

/// Reports diagnostics in the requested form. Fails when there are errors.
fn report(compiled: &Compiled, source: &Source, cli: &Cli, extra: serde_json::Value) -> Outcome {
    let report = compiled.report();
    if cli.json {
        let mut value = serde_json::to_value(&report).unwrap_or_default();
        if let (Some(object), serde_json::Value::Object(extra)) = (value.as_object_mut(), extra) {
            object.extend(extra);
        }
        print_json(&value);
    } else {
        print_human(&compiled.diagnostics, source);
    }
    if report.has_errors() {
        Err(Failure::Diagnostics)
    } else {
        Ok(())
    }
}

fn parse_preset(text: &str) -> Result<Preset, Failure> {
    Preset::parse(text)
        .ok_or_else(|| Failure::Usage(format!("`{text}` is not a preset; use dm, player or clean")))
}

fn stage_output(
    compiled: &Compiled,
    stage: &str,
    spec: &ExportSpec,
    reg: &Registry,
) -> Result<String, Failure> {
    let missing = || Failure::Diagnostics;
    Ok(match stage {
        "document" => dedalo_core::to_json(compiled.document.as_ref().ok_or_else(missing)?),
        "resolved" => dedalo_core::stage_json(compiled.resolved.as_ref().ok_or_else(missing)?),
        "view" | "scene" => {
            let rendered = dedalo_core::render(compiled, spec, reg).map_err(|d| {
                eprintln!("error[{}]: {}", d.code.as_str(), d.message);
                Failure::Diagnostics
            })?;
            if stage == "view" {
                dedalo_core::stage_json(&rendered.view)
            } else {
                dedalo_core::stage_json(&rendered.scene)
            }
        }
        other => {
            return Err(Failure::Usage(format!(
                "`{other}` is not a stage; use document, resolved, view or scene"
            )));
        }
    })
}

static UPDATE_LOCK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The document a command reads, for plugin discovery beside it.
fn document_dir(command: &Command) -> Option<PathBuf> {
    let file = match command {
        Command::Render { file, .. }
        | Command::Validate { file, .. }
        | Command::Edit { file, .. }
        | Command::Fmt { file, .. }
        | Command::Convert { file, .. }
        | Command::Inspect { file, .. } => file,
        Command::Mcp { root, .. } => return Some(root.clone()),
        _ => return None,
    };
    (file != "-").then(|| {
        Path::new(file)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    })
}

fn run(cli: &Cli) -> Outcome {
    UPDATE_LOCK.store(cli.update_lock, std::sync::atomic::Ordering::Relaxed);
    let (reg, origins) =
        plugins::registry(&cli.plugin_paths, document_dir(&cli.command).as_deref());
    if let Command::Render {
        file, watch: true, ..
    } = &cli.command
    {
        if file == "-" {
            return Err(Failure::Usage(
                "`--watch` needs a file, not standard input".to_string(),
            ));
        }
        let mut once = cli.clone();
        if let Command::Render { watch, .. } = &mut once.command {
            *watch = false;
        }
        let stamp = || std::fs::metadata(file).and_then(|m| m.modified()).ok();
        loop {
            let seen = stamp();
            let _ = run(&once);
            eprintln!("watching {file}; press Ctrl+C to stop");
            while stamp() == seen {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
    }
    match &cli.command {
        Command::Render {
            file,
            output,
            export,
            all,
            preset,
            format,
            cell_px,
            rotation,
            theme,
            emit,
            force,
            ..
        } => {
            let source = read(file)?;
            let compiled = compile(&source, &reg);
            let blocked = compiled.has_errors() && !*force;
            let names: Vec<Option<String>> = match (&compiled.document, all) {
                (Some(doc), true) if !doc.dungeon.exports.is_empty() => doc
                    .dungeon
                    .exports
                    .iter()
                    .map(|e| Some(e.name.clone()))
                    .collect(),
                _ => vec![export.clone()],
            };
            if names.len() > 1 && output.is_some() {
                return Err(Failure::Usage(
                    "`--all` writes one file per export and cannot be combined with `-o`"
                        .to_string(),
                ));
            }
            let mut outputs = Vec::new();
            let mut failed = false;
            for name in names.iter().filter(|_| !blocked) {
                let mut spec = match dedalo_core::export_spec(&compiled, name.as_deref()) {
                    Ok(spec) => spec,
                    Err(d) => return Err(Failure::Usage(d.message)),
                };
                if let Some(p) = preset {
                    spec.preset = parse_preset(p)?;
                    if name.is_none() && spec.name == "default" {
                        spec.name = p.clone();
                    }
                }
                if let Some(f) = format {
                    spec.format = f.clone();
                }
                if let Some(px) = cell_px {
                    if !(16..=512).contains(px) {
                        return Err(Failure::Usage(format!(
                            "`--cell-px` is {px}, but must be between 16 and 512"
                        )));
                    }
                    spec.cell_px = *px;
                }
                if let Some(r) = rotation {
                    if ![0, 90, 180, 270].contains(r) {
                        return Err(Failure::Usage(format!(
                            "`--rotation` is {r}, but must be 0, 90, 180 or 270"
                        )));
                    }
                    spec.rotation = *r;
                }
                if theme.is_some() {
                    spec.theme = theme.clone();
                }
                if let Some(stage) = emit {
                    write(
                        output.as_deref(),
                        stage_output(&compiled, stage, &spec, &reg)?.as_bytes(),
                    )?;
                    continue;
                }
                match dedalo_core::render(&compiled, &spec, &reg) {
                    Ok(rendered) => {
                        // Without `-o`, the map goes beside the source as
                        // `<dungeon-id>.<export-name>.<extension>`.
                        let target = output.clone().or_else(|| {
                            let dir = source
                                .path
                                .as_ref()?
                                .parent()
                                .unwrap_or(Path::new(""))
                                .to_path_buf();
                            let id = compiled
                                .document
                                .as_ref()
                                .map_or("dungeon", |d| d.dungeon.id.as_str());
                            Some(
                                dir.join(format!("{id}.{}.{}", spec.name, rendered.extension))
                                    .display()
                                    .to_string(),
                            )
                        });
                        write(target.as_deref(), &rendered.bytes)?;
                        if let Some(path) = target.filter(|t| t != "-") {
                            if !cli.json {
                                eprintln!("wrote {path}");
                            }
                            outputs.push(json!({ "export": spec.name, "path": path, "format": spec.format, "bytes": rendered.bytes.len() }));
                        }
                    }
                    Err(d) => {
                        eprintln!("error[{}]: {}", d.code.as_str(), d.message);
                        failed = true;
                    }
                }
            }
            // With the map on standard output, JSON would corrupt it.
            let to_stdout = emit.is_some()
                || matches!(output.as_deref(), Some("-"))
                || (output.is_none() && source.path.is_none());
            if cli.json && !to_stdout {
                report(&compiled, &source, cli, json!({ "outputs": outputs }))?;
            } else {
                print_human(&compiled.diagnostics, &source);
            }
            if blocked || failed || compiled.has_errors() {
                Err(Failure::Diagnostics)
            } else {
                Ok(())
            }
        }
        Command::Validate { file, fix } => {
            let mut source = read(file)?;
            if *fix {
                if source.json {
                    return Err(Failure::Usage(
                        "`--fix` edits DSL text; convert the JSON document first".to_string(),
                    ));
                }
                let (text, fixed) = dedalo_core::fix_all(&source.text, &reg);
                if !fixed.is_empty() {
                    match &source.path {
                        Some(path) => std::fs::write(path, &text).map_err(|e| {
                            Failure::Io(format!("cannot write `{}`: {e}", source.name))
                        })?,
                        None => write(None, text.as_bytes())?,
                    }
                    if !cli.json {
                        let codes: Vec<&str> = fixed.iter().map(|c| c.as_str()).collect();
                        eprintln!("fixed {} problem(s): {}", fixed.len(), codes.join(", "));
                    }
                }
                source.text = text;
            }
            let compiled = compile(&source, &reg);
            let result = report(&compiled, &source, cli, json!({}));
            if result.is_ok() && !cli.json {
                eprintln!("{}: ok", source.name);
            }
            result
        }
        Command::Edit {
            file,
            ops,
            dry_run,
            base_hash,
            force,
        } => {
            let source = read(file)?;
            if source.json {
                return Err(Failure::Usage(
                    "`edit` patches DSL text; convert the JSON document first".to_string(),
                ));
            }
            if file == "-" && ops == "-" {
                return Err(Failure::Usage(
                    "the file and `--ops` cannot both come from standard input".to_string(),
                ));
            }
            let ops_text = if ops == "-" {
                read("-")?.text
            } else {
                std::fs::read_to_string(ops)
                    .map_err(|e| Failure::Io(format!("cannot read `{ops}`: {e}")))?
            };
            let mut batch: dedalo_core::model::ops::Batch = match serde_json::from_str(&ops_text) {
                Ok(batch) => batch,
                Err(_) => match serde_json::from_str::<Vec<dedalo_core::model::ops::Op>>(&ops_text)
                {
                    Ok(list) => dedalo_core::model::ops::Batch {
                        base_hash: None,
                        ops: list,
                    },
                    Err(e) => {
                        return Err(Failure::Usage(format!(
                            "`{ops}` is not an edit batch: {e}. `dedalo schema ops` prints the schema"
                        )));
                    }
                },
            };
            if base_hash.is_some() {
                batch.base_hash = base_hash.clone();
            }
            let name = source
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map_or("source".to_string(), |n| n.to_string_lossy().to_string());
            let result = dedalo_core::edit(&source.text, &batch, &reg, *force, &name);
            let write_it = result.applied && !dry_run && result.source != source.text;
            if write_it {
                match &source.path {
                    Some(path) => std::fs::write(path, &result.source)
                        .map_err(|e| Failure::Io(format!("cannot write `{}`: {e}", source.name)))?,
                    None if !cli.json => write(None, result.source.as_bytes())?,
                    None => {}
                }
            }
            if cli.json {
                let mut value = serde_json::to_value(&result).unwrap_or_default();
                if source.path.is_none() {
                    value["source"] = json!(result.source);
                }
                value["written"] = json!(write_it && source.path.is_some());
                print_json(&value);
            } else {
                eprint!("{}", result.diff);
                print_human(
                    &result.diagnostics,
                    &Source {
                        name: source.name.clone(),
                        path: None,
                        text: result.source.clone(),
                        json: false,
                    },
                );
                let verdict = if !result.applied {
                    "not applied"
                } else if *dry_run {
                    "dry run, not written"
                } else if write_it {
                    "applied"
                } else {
                    "no change"
                };
                eprintln!("{verdict}: {} ({})", result.changed.join(", "), result.hash);
            }
            if result.applied {
                Ok(())
            } else {
                Err(Failure::Diagnostics)
            }
        }
        Command::Fmt { file, check } => {
            let source = read(file)?;
            if source.json {
                return Err(Failure::Usage(
                    "`fmt` formats DSL text; a JSON document has no formatting to fix".to_string(),
                ));
            }
            match dedalo_core::format(&source.text, &reg) {
                Err(mut diagnostics) => {
                    for d in &mut diagnostics {
                        if let Some(span) = &mut d.span {
                            span.file = Some(source.name.clone());
                        }
                    }
                    if cli.json {
                        print_json(
                            &serde_json::to_value(Report::new(diagnostics)).unwrap_or_default(),
                        );
                    } else {
                        print_human(&diagnostics, &source);
                    }
                    Err(Failure::Diagnostics)
                }
                Ok(formatted) => {
                    let changed = formatted != source.text;
                    if cli.json {
                        print_json(&json!({ "status": "ok", "changed": changed }));
                    }
                    match (&source.path, check) {
                        (_, true) if changed => {
                            eprintln!("{} is not formatted", source.name);
                            Err(Failure::Diagnostics)
                        }
                        (_, true) => Ok(()),
                        (None, false) if !cli.json => write(None, formatted.as_bytes()),
                        (Some(path), false) if changed => {
                            std::fs::write(path, formatted).map_err(|e| {
                                Failure::Io(format!("cannot write `{}`: {e}", source.name))
                            })
                        }
                        _ => Ok(()),
                    }
                }
            }
        }
        Command::Convert { file, to, output } => {
            let source = read(file)?;
            let compiled = compile(&source, &reg);
            // Conversion needs a document; geometry errors do not block it.
            let blocking: Vec<Diagnostic> = compiled
                .diagnostics
                .iter()
                .filter(|d| {
                    d.severity == Severity::Error && matches!(d.code.stage(), "parse" | "lower")
                })
                .cloned()
                .collect();
            let Some(doc) = compiled.document.as_ref().filter(|_| blocking.is_empty()) else {
                print_human(&blocking, &source);
                return Err(Failure::Diagnostics);
            };
            let text = match to.as_str() {
                "json" => dedalo_core::to_json(doc),
                "dsl" => dedalo_core::to_dsl(doc, &reg),
                other => {
                    return Err(Failure::Usage(format!(
                        "`--to {other}` is not a format; use json or dsl"
                    )));
                }
            };
            write(output.as_deref(), text.as_bytes())
        }
        Command::Inspect {
            file,
            id,
            stage,
            text_map,
            preset,
        } => {
            let source = read(file)?;
            let compiled = compile(&source, &reg);
            let mut spec = ExportSpec::preset(parse_preset(preset)?);
            if *text_map {
                spec.format = "txt".to_string();
                let rendered = dedalo_core::render(&compiled, &spec, &reg)
                    .map_err(|d| Failure::Usage(d.message))?;
                write(None, &rendered.bytes)?;
            } else if let Some(stage) = stage {
                write(
                    None,
                    stage_output(&compiled, stage, &spec, &reg)?.as_bytes(),
                )?;
            } else if let Some(id) = id {
                let found = compiled
                    .resolved
                    .as_ref()
                    .and_then(|r| outline::element(r, id));
                match found {
                    Some(value) => print_json(&value),
                    None => {
                        return Err(Failure::Usage(format!(
                            "no resolved element has the ID `{id}`"
                        )));
                    }
                }
            } else if let Some(resolved) = &compiled.resolved {
                let doc = compiled.document.as_ref();
                if cli.json {
                    print_json(&outline::json(resolved, doc));
                } else {
                    print!("{}", outline::text(resolved, doc));
                }
            }
            print_human(&compiled.diagnostics, &source);
            if compiled.has_errors() {
                Err(Failure::Diagnostics)
            } else {
                Ok(())
            }
        }
        Command::Catalog { query } => {
            let cat = &reg.catalog;
            match query {
                None if cli.json => print_json(&serde_json::to_value(cat).unwrap_or_default()),
                None => {
                    println!(
                        "{:<14} {:<8} {:<11} {:<8} SUBTYPES",
                        "TYPE", "CLASS", "CATEGORY", "DEFAULT"
                    );
                    for (name, t) in &cat.types {
                        let class = format!("{:?}", t.class).to_lowercase();
                        let subtypes = if t.subtypes.is_empty() {
                            "(free)".to_string()
                        } else {
                            t.subtypes.join(", ")
                        };
                        println!(
                            "{name:<14} {class:<8} {:<11} {:<8} {subtypes}",
                            t.category, t.default_visibility
                        );
                    }
                    println!("\nRun `dedalo catalog TYPE` for the properties of a type.");
                }
                Some(q) => {
                    let Some(t) = cat.get(q) else {
                        let names: Vec<&str> = cat.types.keys().map(String::as_str).collect();
                        return Err(Failure::Usage(format!(
                            "`{q}` is not a catalog type.{} The types are {}",
                            dedalo_core::model::suggest::did_you_mean(q, names.iter().copied()),
                            names.join(", ")
                        )));
                    };
                    if cli.json {
                        print_json(
                            &json!({ "type": q, "entry": t, "common_properties": cat.common.properties }),
                        );
                    } else {
                        println!(
                            "{q}: {} ({:?}, category {}, default visibility {})",
                            t.label, t.class, t.category, t.default_visibility
                        );
                        println!(
                            "  footprint: {} by {} cells{}",
                            t.footprint[0],
                            t.footprint[1],
                            if t.blocking { ", blocking" } else { "" }
                        );
                        if !t.subtypes.is_empty() {
                            println!("  subtypes: {}", t.subtypes.join(", "));
                        }
                        println!("  properties:");
                        for (name, p) in t.properties.iter().chain(&cat.common.properties) {
                            let mut detail = format!("{:?}", p.ty).to_lowercase();
                            if let Some(values) = &p.values {
                                detail = format!("{detail}: {}", values.join(" | "));
                            }
                            if let (Some(min), Some(max)) = (p.min, p.max) {
                                detail = format!("{detail}, {min} to {max}");
                            }
                            println!(
                                "    {name:<14} {detail}{}",
                                p.doc
                                    .as_ref()
                                    .map(|d| format!("  ({d})"))
                                    .unwrap_or_default()
                            );
                        }
                    }
                }
            }
            Ok(())
        }
        Command::Schema { name, profile } if profile == "authoring" => {
            if name != "document" {
                return Err(Failure::Usage(
                    "the authoring profile exists for the `document` schema only".to_string(),
                ));
            }
            print_json(&dedalo_core::model::authoring_schema());
            Ok(())
        }
        Command::Schema { name, profile } if profile != "full" => Err(Failure::Usage(format!(
            "`{profile}` is not a profile; use full or authoring"
        ))),
        Command::Schema { name, .. } => match dedalo_core::model::schema(name) {
            Some(schema) => {
                print_json(&schema);
                Ok(())
            }
            None => Err(Failure::Usage(format!(
                "`{name}` is not a schema; the schemas are {}",
                dedalo_core::model::SCHEMA_NAMES.join(", ")
            ))),
        },
        Command::Explain { code } => {
            let Some(c) = Code::parse(&code.to_uppercase()) else {
                let names: Vec<&str> = Code::ALL.iter().map(|c| c.as_str()).collect();
                return Err(Failure::Usage(format!(
                    "`{code}` is not a diagnostic code.{} The codes are {}",
                    dedalo_core::model::suggest::did_you_mean(
                        &code.to_uppercase(),
                        names.iter().copied()
                    ),
                    names.join(", ")
                )));
            };
            if cli.json {
                print_json(
                    &json!({ "code": c.as_str(), "severity": c.severity(), "stage": c.stage(), "summary": c.summary(), "explanation": c.explain() }),
                );
            } else {
                println!(
                    "{} ({:?}, {} stage)\n\n{}\n\n{}",
                    c.as_str(),
                    c.severity(),
                    c.stage(),
                    c.summary(),
                    c.explain()
                );
            }
            Ok(())
        }
        Command::Guide { topic } => {
            let topic = topic.as_deref().unwrap_or("quickstart");
            if topic == "list" {
                for (name, description) in guide::topics(&reg.catalog) {
                    println!("{name:<32} {description}");
                }
                return Ok(());
            }
            print!("{}", guide::topic(topic, &reg).map_err(Failure::Usage)?);
            Ok(())
        }
        Command::Plugin { action } => plugin_command(cli, action, &reg, &origins),
        Command::Mcp { root, http } => {
            let server = mcp::Server::new(root, reg).map_err(Failure::Io)?;
            match http {
                Some(addr) => server.serve_http(addr).map_err(Failure::Io),
                None => server
                    .serve_stdio()
                    .map_err(|e| Failure::Io(format!("standard input or output failed: {e}"))),
            }
        }
        Command::Skill {
            action: SkillAction::Export { dir, llms_txt },
        } => {
            let written = guide::export_skill(dir, &reg).map_err(|e| {
                Failure::Io(format!(
                    "cannot write the skill to `{}`: {e}",
                    dir.display()
                ))
            })?;
            if let Some(path) = llms_txt {
                std::fs::write(path, guide::llms_txt(&reg))
                    .map_err(|e| Failure::Io(format!("cannot write `{}`: {e}", path.display())))?;
            }
            if cli.json {
                print_json(&json!({ "dir": dir.display().to_string(), "files": written }));
            } else {
                eprintln!("wrote {} files to {}", written.len(), dir.display());
            }
            Ok(())
        }
        Command::New { example, output } => {
            let text = match example.as_str() {
                "starter" => STARTER,
                "forgotten_crypt" => EXAMPLE_CRYPT,
                other => {
                    return Err(Failure::Usage(format!(
                        "`{other}` is not an example; the examples are starter and forgotten_crypt"
                    )));
                }
            };
            let target = output
                .clone()
                .unwrap_or_else(|| format!("{example}.dedalo"));
            if target != "-" && Path::new(&target).exists() {
                return Err(Failure::Io(format!(
                    "`{target}` already exists; choose another path with `-o`"
                )));
            }
            write(Some(&target), text.as_bytes())?;
            if target != "-" {
                eprintln!("wrote {target}");
            }
            Ok(())
        }
    }
}

fn plugin_command(
    cli: &Cli,
    action: &PluginAction,
    reg: &Registry,
    origins: &std::collections::BTreeMap<String, PathBuf>,
) -> Outcome {
    match action {
        PluginAction::Add { path } => {
            let plugin = plugins::add(path).map_err(Failure::Usage)?;
            if cli.json {
                print_json(
                    &json!({ "id": plugin.id(), "version": plugin.manifest.plugin.version, "sha256": plugin.sha256 }),
                );
            } else {
                eprintln!(
                    "installed {} {} into .dedalo/plugins and recorded it in {}",
                    plugin.id(),
                    plugin.manifest.plugin.version,
                    plugins::LOCKFILE
                );
            }
            Ok(())
        }
        PluginAction::Remove { id } => {
            plugins::remove(id).map_err(Failure::Usage)?;
            eprintln!("removed {id}");
            Ok(())
        }
        PluginAction::List => {
            let list: Vec<serde_json::Value> = reg
                .plugins
                .iter()
                .map(|p| {
                    let m = &p.manifest;
                    let mut provides: Vec<String> = m.theme.keys().map(|t| format!("theme {}.{t}", p.id())).collect();
                    provides.extend(m.catalog.iter().map(|_| "catalog".to_string()));
                    provides.extend(m.painter.iter().map(|d| format!("painter for {}{}", d.matches.kind, d.matches.ty.as_ref().map(|t| format!(" {t}")).unwrap_or_default())));
                    provides.extend(m.scene_pass.iter().map(|s| format!("scene pass {}", s.name)));
                    provides.extend(m.renderer.iter().map(|r| format!("renderer {}", r.name)));
                    provides.extend(m.exporter.iter().map(|e| format!("format {}.{}", p.id(), e.format)));
                    provides.extend(m.validator.iter().map(|v| format!("validator {}", v.name)));
                    provides.extend(m.shape.iter().map(|s| format!("shape {}.{}", p.id(), s.name)));
                    json!({ "id": p.id(), "version": m.plugin.version, "sha256": p.sha256, "path": origins.get(p.id()).map(|o| o.display().to_string()), "provides": provides })
                })
                .collect();
            if cli.json {
                print_json(&json!(list));
            } else if list.is_empty() {
                println!(
                    "No plugins found. Search path: --plugin, .dedalo/plugins/, {}",
                    plugins::user_dir()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default()
                );
            } else {
                for p in &list {
                    println!(
                        "{} {}  ({})",
                        p["id"].as_str().unwrap_or_default(),
                        p["version"].as_str().unwrap_or_default(),
                        p["path"].as_str().unwrap_or_default()
                    );
                    for item in p["provides"].as_array().into_iter().flatten() {
                        println!("    {}", item.as_str().unwrap_or_default());
                    }
                }
            }
            Ok(())
        }
        PluginAction::New { id, kind, dir } => {
            let dir = dir.clone().unwrap_or_else(|| PathBuf::from(id));
            let files = plugins::scaffold(id, kind, &dir).map_err(Failure::Usage)?;
            eprintln!("wrote {} files to {}", files.len(), dir.display());
            Ok(())
        }
        PluginAction::Check { path } => match plugins::check(path) {
            Ok(passed) => {
                if cli.json {
                    print_json(&json!({ "status": "ok", "passed": passed }));
                } else {
                    for p in passed {
                        println!("ok   {p}");
                    }
                }
                Ok(())
            }
            Err(failures) => {
                if cli.json {
                    print_json(&json!({ "status": "error", "failures": failures }));
                } else {
                    for f in failures {
                        println!("FAIL {f}");
                    }
                }
                Err(Failure::Diagnostics)
            }
        },
    }
}
