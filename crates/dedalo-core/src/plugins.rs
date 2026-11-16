//! Plugins in the pipeline (spec 11.3).
//!
//! A document activates the plugins its `dedalo` header names, in that
//! order. Loaded plugins it does not name have no effect on its output.

use std::sync::{Arc, Mutex};

use dedalo_geom::Resolved;
use dedalo_plugin::LoadedPlugin;
use dedalo_plugin::manifest::ExporterDecl;
use dedalo_plugin::points;
use dedalo_render::{ExportSpec, Painters, Theme, View};
use dedalo_scene::Scene;
use serde_json::Value;

use crate::model::{Code, Diagnostic, Document, ShapeCall};
use crate::{Registry, version_ok};

type Active<'a> = Vec<&'a Arc<LoadedPlugin>>;

/// The plugins a document requires, checked for presence and version.
pub fn active<'a>(doc: &Document, reg: &'a Registry, out: &mut Vec<Diagnostic>) -> Active<'a> {
    let mut active = Vec::new();
    for req in doc.dedalo.iter().flat_map(|h| &h.plugins) {
        let Some(plugin) = reg.plugin(&req.id) else {
            out.push(
                Diagnostic::new(Code::PluginNotFound, format!("The document requires the plugin `{}`, which is not installed.", req.id))
                    .suggest(format!("Install it with `dedalo plugin add <path>`, pass `--plugin <path>`, or remove the `plugin \"{}\"` block.", req.id)),
            );
            continue;
        };
        if let Some(wanted) = &req.version {
            let have = &plugin.manifest.plugin.version;
            match version_ok(wanted, have) {
                Ok(true) => {}
                Ok(false) => {
                    out.push(Diagnostic::new(Code::PluginNotFound, format!("The document requires `{}` version {wanted}, but version {have} is installed.", req.id)));
                    continue;
                }
                Err(e) => {
                    out.push(Diagnostic::new(
                        Code::InvalidValue,
                        format!(
                            "The version requirement of plugin `{}` is not valid: {e}.",
                            req.id
                        ),
                    ));
                    continue;
                }
            }
        }
        active.push(plugin);
    }
    let renderers: Vec<&str> = active
        .iter()
        .filter(|p| p.manifest.renderer.is_some())
        .map(|p| p.id())
        .collect();
    if renderers.len() > 1 {
        out.push(Diagnostic::new(Code::PluginConflict, format!("Two active plugins replace the renderer: {}. Keep one of them in the `dedalo` header.", renderers.join(" and "))));
    }
    active
}

/// A plugin's options: the manifest's defaults, then the document's values.
fn options(plugin: &LoadedPlugin, spec: Option<&ExportSpec>) -> Value {
    let mut map = plugin.manifest.default_options();
    if let Some(given) = spec.and_then(|s| s.plugin_options.get(plugin.id())) {
        for (k, v) in given {
            map.insert(k.clone(), serde_json::to_value(v).unwrap_or_default());
        }
    }
    Value::Object(map)
}

/// A dotted shape function, such as `caves.cave(7, 12, 9)`.
pub fn shape(active: &Active, call: &ShapeCall) -> Option<Result<Vec<[f64; 2]>, String>> {
    let (id, name) = call.func.split_once('.')?;
    let plugin = active.iter().find(|p| p.id() == id)?;
    if !plugin.manifest.shape.iter().any(|s| s.name == name) {
        return Some(Err(format!(
            "the plugin `{id}` has no shape function `{name}`."
        )));
    }
    let args = serde_json::to_value(&call.args).unwrap_or_default();
    Some(points::shape(plugin, name, &args, 0))
}

/// Runs every validator of the active plugins.
pub fn validate(active: &Active, model: &Resolved, doc: &Document) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let has_validators = active.iter().any(|p| !p.manifest.validator.is_empty());
    if !has_validators {
        return out;
    }
    let spec = ExportSpec::from_document(doc, None);
    let resolved = serde_json::to_value(model).unwrap_or_default();
    for plugin in active {
        for v in &plugin.manifest.validator {
            match points::validator(plugin, &v.name, &resolved, &options(plugin, spec.as_ref())) {
                Ok(found) => out.extend(found),
                Err(d) => out.push(d),
            }
        }
    }
    out
}

/// Paints a view: the built-in painters with the active plugins' painters
/// on top, or a plugin renderer; then the plugins' scene passes in order.
pub fn paint(
    active: &Active,
    view: &View,
    theme: &Theme,
    spec: &ExportSpec,
) -> Result<Scene, Diagnostic> {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let renderer = active.iter().find(|p| p.manifest.renderer.is_some());
    let mut scene = match renderer {
        Some(plugin) => points::renderer(plugin, view, theme, &options(plugin, Some(spec)))?,
        None => {
            let mut painters = Painters::builtin();
            for plugin in active {
                for decl in &plugin.manifest.painter {
                    let m = &decl.matches;
                    let painter = points::WasmPainter {
                        plugin: (*plugin).clone(),
                        decl: decl.clone(),
                        options: options(plugin, Some(spec)),
                        seed: view.seed,
                        errors: errors.clone(),
                    };
                    painters.register(
                        &m.kind,
                        m.ty.as_deref().unwrap_or(""),
                        m.subtype.as_deref().unwrap_or(""),
                        painter,
                    );
                }
            }
            dedalo_render::render_scene(view, theme, &painters)
        }
    };
    if let Some(first) = errors.lock().expect("an error list").first() {
        return Err(first.clone());
    }
    for plugin in active {
        for pass in &plugin.manifest.scene_pass {
            scene = points::scene_pass(
                plugin,
                &pass.name,
                scene,
                view,
                &options(plugin, Some(spec)),
            )?;
        }
    }
    Ok(scene)
}

/// The exporter declaration behind a dotted format.
pub fn exporter_decl<'a>(
    active: &'a Active,
    format: &str,
) -> Option<(&'a Arc<LoadedPlugin>, &'a ExporterDecl)> {
    let (id, name) = format.split_once('.')?;
    let plugin = active.iter().find(|p| p.id() == id)?;
    plugin
        .manifest
        .exporter
        .iter()
        .find(|e| e.format == name)
        .map(|e| (*plugin, e))
}

/// Runs a plugin exporter. `None` when no active plugin has the format.
pub fn export(
    active: &Active,
    format: &str,
    view: &View,
    scene: &Scene,
    spec: &ExportSpec,
) -> Result<Option<Vec<u8>>, Diagnostic> {
    let Some((plugin, decl)) = exporter_decl(active, format) else {
        return Ok(None);
    };
    let wants = |x: &str| decl.needs.iter().any(|n| n == x);
    let svg = (wants("svg") || wants("png")).then(|| dedalo_scene::to_svg(scene));
    #[cfg(feature = "raster")]
    let png = match (&svg, wants("png") && spec.host_png.is_none()) {
        (Some(svg), true) => {
            Some(dedalo_export::png(svg).map_err(|e| Diagnostic::new(Code::ExportTooLarge, e))?)
        }
        _ => None,
    };
    #[cfg(not(feature = "raster"))]
    let png: Option<Vec<u8>> = None;
    let png = png.or_else(|| spec.host_png.clone());
    if wants("png") && png.is_none() {
        return Err(Diagnostic::new(
            Code::PluginFailed,
            format!("The exporter `{format}` needs a PNG rendering, which this build cannot make."),
        ));
    }
    let bytes = points::exporter(
        plugin,
        decl,
        view,
        scene,
        svg.as_deref(),
        png.as_deref(),
        &options(plugin, Some(spec)),
    )?;
    Ok(Some(bytes))
}
