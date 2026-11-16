//! The facade every host calls (spec 3.5).
//!
//! The binary, the web editor and the MCP server contain no map logic of
//! their own: they call `parse`, `compile` and `render` here, so they cannot
//! disagree. Every function is pure: no I/O, no clock, no ambient randomness.

use dedalo_model::diag::Span;
use dedalo_model::lower::lower;
use dedalo_syntax::text::LineIndex;
use dedalo_syntax::{Parse, SyntaxNode};

pub use dedalo_catalog as catalog;
pub use dedalo_export::text_map;
pub use dedalo_geom::Resolved;
pub use dedalo_model as model;
pub use dedalo_model::{Code, Diagnostic, Document, Preset, Report, Severity};
pub use dedalo_plugin::Registry;
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
    pub media_type: &'static str,
    pub extension: &'static str,
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
            let lowered = lower(&parsed.syntax(), source, &reg.catalog);
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
        for plugin in doc
            .dedalo
            .iter()
            .flat_map(|h| &h.plugins)
            .filter(|p| !reg.plugins.contains(&p.id))
        {
            later.push(
                Diagnostic::new(Code::PluginNotFound, format!("The document requires the plugin `{}`, which is not installed.", plugin.id))
                    .suggest(format!("Install it with `dedalo plugin add <path-or-url>` (available from release 0.2), or remove the `plugin \"{}\"` block.", plugin.id)),
            );
        }
        let (model, geometry) = dedalo_geom::resolve(doc, &reg.catalog);
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
    let scene = dedalo_render::render_scene(&view, theme, &reg.painters);
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
        "png" | "pdf" => {
            return Err(Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "The format `{}` is not available in this build: PNG and PDF output arrive with milestone M3. Use `svg` or `txt`.",
                    export.format
                ),
            ));
        }
        other if other.contains('.') => {
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
        media_type,
        extension,
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
pub fn golden_outputs(c: &Compiled, reg: &Registry) -> std::collections::BTreeMap<String, String> {
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
    }
    files
}
