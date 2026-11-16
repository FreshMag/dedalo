//! The facade every host calls (spec 3.5).
//!
//! The binary, the web editor and the MCP server contain no map logic of
//! their own: they call `parse`, `compile` and `render` here, so they cannot
//! disagree. Every function is pure: no I/O, no clock, no ambient randomness.

pub mod assist;
pub mod plugins;

use dedalo_model::diag::Span;
use dedalo_model::lower::lower;
use dedalo_syntax::text::LineIndex;
use dedalo_syntax::{Parse, SyntaxNode};

pub use dedalo_catalog as catalog;
pub use dedalo_export::text_map;
pub use dedalo_geom::Resolved;
pub use dedalo_model as model;
pub use dedalo_model::{Code, Diagnostic, Document, Preset, Report, Severity};
pub use dedalo_ops::content_hash;
pub use dedalo_plugin::Registry;
pub use dedalo_render::CLASSIC_TOML as CLASSIC_THEME_TOML;
pub use dedalo_render::{Audience, ExportSpec, View, build_view};
pub use dedalo_scene::Scene;

/// Largest source the tool accepts (spec 10.3).
pub const MAX_SOURCE_BYTES: usize = 5 * 1024 * 1024;

/// What `compile` reads: DSL text or document JSON.
#[derive(Clone, Copy, Debug)]
pub enum Input<'a> {
    Dsl(&'a str),
    Json(&'a str),
}

/// The result of stage 1.
pub struct Parsed {
    parse: Parse,
    pub diagnostics: Vec<Diagnostic>,
}

impl Parsed {
    pub fn syntax(&self) -> SyntaxNode {
        self.parse.syntax()
    }
}

/// The results of stages 1 to 3.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// Absent when the input has no usable `dungeon` block.
    pub document: Option<Document>,
    /// Whatever resolved, even when there are errors.
    pub resolved: Option<Resolved>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Compiled {
    pub fn report(&self) -> Report {
        Report::new(self.diagnostics.clone())
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// The results of stages 4 to 6 for one export.
#[derive(Debug, Clone)]
pub struct Rendered {
    pub view: View,
    pub scene: Scene,
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub extension: String,
}

fn too_large(source: &str) -> Option<Diagnostic> {
    (source.len() > MAX_SOURCE_BYTES).then(|| {
        Diagnostic::new(
            Code::InvalidValue,
            format!(
                "The source is {} bytes; the limit is {MAX_SOURCE_BYTES} bytes (5 MB).",
                source.len()
            ),
        )
    })
}

/// Stage 1: text to a lossless syntax tree, with syntax diagnostics.
pub fn parse(source: &str) -> Parsed {
    let parse = dedalo_syntax::parse(source);
    let lines = LineIndex::new(source);
    let diagnostics = parse
        .errors
        .iter()
        .map(|e| {
            Diagnostic::new(Code::SyntaxError, e.message.clone())
                .span(Span::from_range(e.range, &lines))
        })
        .collect();
    Parsed { parse, diagnostics }
}

/// Stages 1 to 3: parse, lower and resolve. Every stage reports all it can
/// find and passes on what did resolve, so one bad door does not hide the
/// other errors.
pub fn compile(input: Input, reg: &Registry) -> Compiled {
    let mut diagnostics = Vec::new();
    let mut source_map = None;
    let document = match input {
        Input::Dsl(source) => {
            if let Some(d) = too_large(source) {
                return Compiled {
                    document: None,
                    resolved: None,
                    diagnostics: vec![d],
                };
            }
            let parsed = parse(source);
            diagnostics.extend(parsed.diagnostics.iter().cloned());
            let external: Vec<(String, SyntaxNode)> = reg
                .templates
                .iter()
                .filter_map(|(name, text)| {
                    Some((
                        name.clone(),
                        dedalo_syntax::parse(text).syntax().first_child()?,
                    ))
                })
                .collect();
            let lowered =
                dedalo_model::lower::lower_with(&parsed.syntax(), source, &reg.catalog, &external);
            diagnostics.extend(lowered.diagnostics);
            source_map = Some((lowered.source_map, source));
            lowered.document
        }
        Input::Json(text) => {
            if let Some(d) = too_large(text) {
                return Compiled {
                    document: None,
                    resolved: None,
                    diagnostics: vec![d],
                };
            }
            match serde_json::from_str::<Document>(text) {
                Ok(doc) => Some(doc),
                Err(e) => {
                    let code = if e.is_syntax() || e.is_eof() {
                        Code::SyntaxError
                    } else {
                        Code::InvalidValue
                    };
                    let span = Span {
                        file: None,
                        start: [e.line() as u32, e.column() as u32],
                        end: [e.line() as u32, e.column() as u32],
                    };
                    diagnostics.push(Diagnostic::new(code, format!("The document JSON is not valid: {e}. `dedalo schema document` prints the schema.")).span(span));
                    None
                }
            }
        }
    };
    let mut resolved = None;
    if let Some(doc) = &document {
        let mut later = dedalo_model::check::check(doc, &reg.catalog);
        let active = plugins::active(doc, reg, &mut later);
        let shapes = |call: &model::ShapeCall| plugins::shape(&active, call);
        let (model, geometry) = dedalo_geom::resolve_with(doc, &reg.catalog, &shapes);
        later.extend(plugins::validate(&active, &model, doc));
        // An element that already has an error is likely to fail again in
        // geometry for the same reason; report the cause, not the echo.
        let broken: Vec<&String> = diagnostics
            .iter()
            .chain(&later)
            .filter(|d| d.severity == Severity::Error)
            .filter_map(|d| d.element.as_ref())
            .collect();
        let geometry: Vec<Diagnostic> = geometry
            .into_iter()
            .filter(|d| d.element.as_ref().is_none_or(|e| !broken.contains(&e)))
            .collect();
        later.extend(geometry);
        if let Some((map, source)) = &source_map {
            map.attach(&mut later, &LineIndex::new(source));
        }
        diagnostics.extend(later);
        resolved = Some(model);
    }
    // A syntax error can hide whole blocks, so what the later stages say
    // about a file that did not parse is mostly noise. The syntax errors are
    // reported alone; the partial model is still passed on for previews.
    if diagnostics.iter().any(|d| d.code == Code::SyntaxError) {
        diagnostics.retain(|d| d.code == Code::SyntaxError);
    }
    // Source order reads best; diagnostics without a span go last.
    diagnostics.sort_by_key(|d| {
        d.span
            .as_ref()
            .map_or((u32::MAX, u32::MAX), |s| (s.start[0], s.start[1]))
    });
    Compiled {
        document,
        resolved,
        diagnostics,
    }
}

/// The export configuration to use: the named `export` block, or the default.
pub fn export_spec(c: &Compiled, name: Option<&str>) -> Result<ExportSpec, Diagnostic> {
    let Some(doc) = &c.document else {
        return Ok(ExportSpec::preset(Preset::Dm));
    };
    ExportSpec::from_document(doc, name).ok_or_else(|| {
        let names: Vec<&str> = doc
            .dungeon
            .exports
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        let name = name.unwrap_or_default();
        Diagnostic::new(
            Code::UnknownReference,
            format!(
                "The document has no export named `{name}`.{} {}",
                dedalo_model::suggest::did_you_mean(name, names.iter().copied()),
                if names.is_empty() {
                    "It declares no `export` blocks; use a preset instead.".to_string()
                } else {
                    format!("Its exports are {}.", names.join(", "))
                }
            ),
        )
    })
}

/// Stages 4 to 6: filter for one audience, paint and serialise.
///
/// The view is built from the resolved model alone, and everything after it
/// works only on the view: this is the secrecy boundary.
pub fn render(c: &Compiled, export: &ExportSpec, reg: &Registry) -> Result<Rendered, Diagnostic> {
    let Some(resolved) = &c.resolved else {
        return Err(Diagnostic::new(
            Code::InvalidValue,
            "There is nothing to render: the input has no dungeon.",
        ));
    };
    let Some(theme) = reg.theme(export.theme.as_deref()) else {
        let name = export.theme.as_deref().unwrap_or_default();
        let known: Vec<&str> = reg.themes.keys().map(String::as_str).collect();
        return Err(if name.contains('.') {
            Diagnostic::new(
                Code::PluginNotFound,
                format!("The theme `{name}` comes from a plugin that is not installed."),
            )
        } else {
            Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "`{name}` is not a theme.{} The themes are {}.",
                    dedalo_model::suggest::did_you_mean(name, known.iter().copied()),
                    known.join(", ")
                ),
            )
        });
    };
    let view = build_view(resolved, export);
    let active: Vec<&std::sync::Arc<dedalo_plugin::LoadedPlugin>> = export
        .plugins
        .iter()
        .filter_map(|id| reg.plugin(id))
        .collect();
    let scene = plugins::paint(&active, &view, theme, export)?;
    let (bytes, media_type, extension) = match export.format.as_str() {
        "svg" => (
            dedalo_scene::to_svg(&scene).into_bytes(),
            "image/svg+xml",
            "svg",
        ),
        "txt" => (
            text_map(&view).into_bytes(),
            "text/plain; charset=utf-8",
            "txt",
        ),
        #[cfg(not(feature = "raster"))]
        "png" | "pdf" => {
            return Err(Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "This build of the core has no `{}` output; the export module provides it.",
                    export.format
                ),
            ));
        }
        #[cfg(feature = "raster")]
        "png" | "pdf" => {
            let svg = dedalo_scene::to_svg(&scene);
            if export.format == "png" {
                // Spec 9.6: refuse before drawing, and say what would fit.
                let [_, _, w, h] = scene.bounds;
                let longest = w.max(h);
                let px = (longest * scene.cell_px as f64).round() as u32;
                if px > dedalo_export::MAX_PNG_SIDE {
                    let fits = (dedalo_export::MAX_PNG_SIDE as f64 / longest).floor() as u32;
                    let advice = if fits >= 16 {
                        format!("The largest valid `cell_px` is {fits}.")
                    } else {
                        "No `cell_px` from 16 up fits; export SVG or PDF instead.".to_string()
                    };
                    return Err(Diagnostic::new(
                        Code::ExportTooLarge,
                        format!(
                            "The PNG would be {px} pixels on its longest side at {} pixels per cell; the limit is {}. {advice}",
                            scene.cell_px,
                            dedalo_export::MAX_PNG_SIDE
                        ),
                    ));
                }
                let bytes = dedalo_export::png(&svg)
                    .map_err(|e| Diagnostic::new(Code::ExportTooLarge, e))?;
                (bytes, "image/png", "png")
            } else {
                let bytes = dedalo_export::pdf(
                    &svg,
                    export.page,
                    export.orientation,
                    export.tabletop.then_some(scene.cell_px),
                )
                .map_err(|e| Diagnostic::new(Code::InvalidValue, e))?;
                (bytes, "application/pdf", "pdf")
            }
        }
        other if other.contains('.') => {
            if let Some(bytes) = plugins::export(&active, other, &view, &scene, export)? {
                let (_, decl) =
                    plugins::exporter_decl(&active, other).expect("the exporter exists");
                return Ok(Rendered {
                    view,
                    scene,
                    bytes,
                    media_type: decl.media_type.clone(),
                    extension: decl.extension.clone(),
                });
            }
            return Err(Diagnostic::new(
                Code::PluginNotFound,
                format!("The format `{other}` comes from a plugin that is not installed."),
            ));
        }
        other => {
            return Err(Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "`{other}` is not an output format. The formats are svg, png, pdf and txt."
                ),
            ));
        }
    };
    Ok(Rendered {
        view,
        scene,
        bytes,
        media_type: media_type.to_string(),
        extension: extension.to_string(),
    })
}

/// Formats DSL text canonically (spec 5.11). Refuses text with syntax errors.
pub fn format(source: &str, reg: &Registry) -> Result<String, Vec<Diagnostic>> {
    let parsed = parse(source);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }
    let lowered = lower(&parsed.syntax(), source, &reg.catalog);
    Ok(dedalo_syntax::fmt::format(
        &parsed.syntax(),
        &lowered.rewrites,
    ))
}

/// The document model as canonical DSL text.
pub fn to_dsl(doc: &Document, reg: &Registry) -> String {
    dedalo_model::write::to_dsl(doc, &reg.catalog)
}

/// The document model as JSON text, with a trailing line break.
pub fn to_json(doc: &Document) -> String {
    let mut text = serde_json::to_string_pretty(doc).expect("the document model serialises");
    text.push('\n');
    text
}

/// A stage output as JSON text, for `--emit` and `inspect --stage`.
pub fn stage_json<T: serde::Serialize>(value: &T) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("stage outputs serialise");
    text.push('\n');
    text
}

/// Every output the golden corpus records for one input, by file name
/// (spec 15.1). The native test runner and the WebAssembly build both call
/// this, so the two are compared on exactly the same bytes.
/// With `raster`, PNG and PDF hashes are included; the WebAssembly core
/// module leaves them to the export module.
pub fn golden_outputs(
    c: &Compiled,
    reg: &Registry,
    raster: bool,
) -> std::collections::BTreeMap<String, String> {
    let _ = raster;
    let mut files = std::collections::BTreeMap::new();
    files.insert("diagnostics.json".to_string(), stage_json(&c.report()));
    if let Some(doc) = &c.document {
        files.insert("document.json".to_string(), to_json(doc));
    }
    if c.has_errors() {
        return files;
    }
    if let Some(resolved) = &c.resolved {
        files.insert("resolved.json".to_string(), stage_json(resolved));
    }
    // The exports the document declares, else both presets.
    let names: Vec<String> = c
        .document
        .iter()
        .flat_map(|d| d.dungeon.exports.iter().map(|e| e.name.clone()))
        .collect();
    let specs: Vec<ExportSpec> = if names.is_empty() {
        vec![
            ExportSpec::preset(Preset::Dm),
            ExportSpec::preset(Preset::Player),
        ]
    } else {
        names
            .iter()
            .filter_map(|n| export_spec(c, Some(n)).ok())
            .collect()
    };
    for mut spec in specs {
        for format in ["svg", "txt"] {
            spec.format = format.to_string();
            if let Ok(rendered) = render(c, &spec, reg) {
                files.insert(
                    format!("{}.{format}", spec.name),
                    String::from_utf8_lossy(&rendered.bytes).into_owned(),
                );
            }
        }
        // A plugin format is recorded by hash; it may embed raster output.
        let own = export_spec(c, Some(&spec.name))
            .map(|s| s.format)
            .unwrap_or_default();
        if raster && own.contains('.') {
            let mut plugin_spec = spec.clone();
            plugin_spec.format = own.clone();
            if let Ok(rendered) = render(c, &plugin_spec, reg) {
                files.insert(
                    format!("{}.{own}.sha256", spec.name),
                    format!(
                        "{}
",
                        sha256_hex(&rendered.bytes)
                    ),
                );
            }
        }
        // PNG and PDF are recorded by hash, derived from the SVG above.
        #[cfg(feature = "raster")]
        if raster && let Some(svg) = files.get(&format!("{}.svg", spec.name)).cloned() {
            if let Ok(png) = dedalo_export::png(&svg) {
                files.insert(
                    format!("{}.png.sha256", spec.name),
                    format!(
                        "{}
",
                        sha256_hex(&png)
                    ),
                );
            }
            if let Ok(pdf) = dedalo_export::pdf(
                &svg,
                spec.page,
                spec.orientation,
                spec.tabletop.then_some(spec.cell_px),
            ) {
                files.insert(
                    format!("{}.pdf.sha256", spec.name),
                    format!(
                        "{}
",
                        sha256_hex(&pdf)
                    ),
                );
            }
        }
    }
    files
}

/// The SHA-256 of some bytes, in lowercase hexadecimal.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The result of an edit batch (spec 13.3).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EditResult {
    pub status: dedalo_model::Status,
    /// Whether the new text should be written: the batch applied and
    /// introduced no new error (or `force` was given).
    pub applied: bool,
    /// The IDs the batch touched.
    pub changed: Vec<String>,
    /// Content hash of the text the file holds after this call.
    pub hash: String,
    /// Unified diff from the old text to the new one.
    pub diff: String,
    /// Diagnostics of the new text, or the reason the batch failed.
    pub diagnostics: Vec<Diagnostic>,
    /// The new text. Not part of the JSON result.
    #[serde(skip)]
    pub source: String,
}

fn error_keys(c: &Compiled) -> Vec<(Code, Option<String>, String)> {
    c.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code, d.element.clone(), d.message.clone()))
        .collect()
}

/// Applies a batch of edit operations to DSL text (spec 3.5 and 13.3).
///
/// Operations patch the syntax tree in order. The result is validated, and
/// is marked applied only if the batch introduces no new error, unless
/// `force` is set. `name` labels the diff.
pub fn edit(
    source: &str,
    batch: &dedalo_ops::Batch,
    reg: &Registry,
    force: bool,
    name: &str,
) -> EditResult {
    let old_hash = dedalo_ops::content_hash(source);
    let failed = |d: Diagnostic| EditResult {
        status: dedalo_model::Status::Error,
        applied: false,
        changed: Vec::new(),
        hash: old_hash.clone(),
        diff: String::new(),
        diagnostics: vec![d],
        source: source.to_string(),
    };
    if let Some(base) = &batch.base_hash
        && *base != old_hash
    {
        return failed(Diagnostic::new(
            Code::StaleSource,
            format!(
                "The file changed since the edit was prepared: its hash is {old_hash}, not {base}. Read it again and recompute the edit."
            ),
        ));
    }
    let applied = match dedalo_ops::apply_ops(source, &batch.ops, &reg.catalog) {
        Ok(applied) => applied,
        Err(d) => return failed(d),
    };
    let before = error_keys(&compile(Input::Dsl(source), reg));
    let after = compile(Input::Dsl(&applied.source), reg);
    let mut diagnostics = after.diagnostics.clone();
    let introduced: Vec<usize> = diagnostics
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            d.severity == Severity::Error
                && !before.contains(&(d.code, d.element.clone(), d.message.clone()))
        })
        .map(|(i, _)| i)
        .collect();
    let ok = introduced.is_empty() || force;
    if !ok {
        // The new errors first: they are why the batch was refused.
        let mut new: Vec<Diagnostic> = introduced.iter().map(|i| diagnostics[*i].clone()).collect();
        let rest = diagnostics
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !introduced.contains(i))
            .map(|(_, d)| d);
        new.extend(rest);
        diagnostics = new;
    }
    let status = if ok {
        Report::new(diagnostics.clone()).status
    } else {
        dedalo_model::Status::Error
    };
    EditResult {
        status,
        applied: ok,
        changed: applied.changed,
        hash: if ok {
            dedalo_ops::content_hash(&applied.source)
        } else {
            old_hash.clone()
        },
        diff: dedalo_ops::unified(source, &applied.source, name),
        diagnostics,
        source: applied.source,
    }
}

/// Applies every diagnostic fix that removes its diagnostic without adding
/// an error, repeating while fixes keep applying. Returns the new text and
/// the codes fixed, in order.
pub fn fix_all(source: &str, reg: &Registry) -> (String, Vec<Code>) {
    let mut text = source.to_string();
    let mut fixed = Vec::new();
    for _ in 0..16 {
        let compiled = compile(Input::Dsl(&text), reg);
        let mut progress = false;
        for d in compiled.diagnostics.iter().filter(|d| d.fix.is_some()) {
            let batch = dedalo_ops::Batch {
                base_hash: None,
                ops: vec![d.fix.clone().expect("filtered")],
            };
            let result = edit(&text, &batch, reg, false, "");
            let gone = !result
                .diagnostics
                .iter()
                .any(|x| x.code == d.code && x.element == d.element);
            if result.applied && gone {
                text = result.source;
                fixed.push(d.code);
                progress = true;
                break;
            }
        }
        if !progress {
            break;
        }
    }
    (text, fixed)
}

/// Whether a plugin version satisfies a document's requirement.
pub fn version_ok(requirement: &str, version: &str) -> Result<bool, String> {
    dedalo_plugin::version::matches(requirement, version)
}
