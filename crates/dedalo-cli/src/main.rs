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

mod outline;

#[derive(Parser)]
#[command(
    name = "dedalo",
    version,
    about = "Compile a dungeon description into a map."
)]
struct Cli {
    /// Print the machine-readable form on standard output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
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
    },
    /// Run parse, lower and resolve; print diagnostics.
    Validate { file: String },
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
    Schema { name: String },
    /// Describe a diagnostic code with a worked fix.
    Explain { code: String },
    /// Print the built-in language reference.
    Guide { topic: Option<String> },
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

/// A failure that ends the command, with its exit code (spec section 12).
enum Failure {
    /// Diagnostics contain at least one error.
    Diagnostics,
    Usage(String),
    Io(String),
}

type Outcome = Result<(), Failure>;

const QUICKSTART: &str = include_str!("../../../docs/guide/quickstart.md");
const EXAMPLE_CRYPT: &str = include_str!("../../../tests/corpus/forgotten_crypt/input.dedalo");
const STARTER: &str = "dungeon \"my_dungeon\" {\n    name = \"My Dungeon\"\n\n    room \"entrance\" {\n        name = \"Entrance\"\n        shape = rectangle(8, 6)\n    }\n}\n";

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
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

fn run(cli: &Cli) -> Outcome {
    let reg = Registry::builtin();
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
        Command::Validate { file } => {
            let source = read(file)?;
            let compiled = compile(&source, &reg);
            let result = report(&compiled, &source, cli, json!({}));
            if result.is_ok() && !cli.json {
                eprintln!("{}: ok", source.name);
            }
            result
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
        Command::Schema { name } => match dedalo_core::model::schema(name) {
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
            let text = match topic {
                "quickstart" => QUICKSTART.to_string(),
                "examples/forgotten_crypt" => format!("```hcl\n{EXAMPLE_CRYPT}```\n"),
                "errors" => Code::ALL
                    .iter()
                    .map(|c| format!("- `{}`: {}\n", c.as_str(), c.summary()))
                    .collect(),
                t if t.starts_with("errors/") => match Code::parse(&t[7..].to_uppercase()) {
                    Some(c) => format!("# {}\n\n{}\n\n{}\n", c.as_str(), c.summary(), c.explain()),
                    None => {
                        return Err(Failure::Usage(format!(
                            "`{}` is not a diagnostic code",
                            &t[7..]
                        )));
                    }
                },
                other => {
                    return Err(Failure::Usage(format!(
                        "`{other}` is not a guide topic; the topics are quickstart, errors, errors/<CODE> and examples/forgotten_crypt (syntax, layout and catalog topics arrive with milestone M5; use `dedalo catalog` meanwhile)"
                    )));
                }
            };
            print!("{text}");
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
