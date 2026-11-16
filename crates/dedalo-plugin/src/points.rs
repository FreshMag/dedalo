//! The extension points (spec 11.1): adapters that turn plugin calls into
//! the same traits and values the built-ins use, so the rest of the
//! pipeline cannot tell the two apart (spec 11.2).

use std::sync::{Arc, Mutex};

use dedalo_model::{Code, Diagnostic, Severity};
use dedalo_render::{Audience, Cx, Item, Painter, Theme, View};
use dedalo_scene::{Cmd, Node, Scene};
use serde_json::{Value, json};

use crate::manifest::{ExporterDecl, PainterDecl};
use crate::runtime::CallError;
use crate::{LoadedPlugin, base64};

/// Most nodes a single response may hold.
pub const MAX_NODES: usize = 200_000;
/// Largest coordinate a plugin may emit, in cells.
pub const MAX_COORDINATE: f64 = 1_000_000.0;

/// A stable hash of a request, mixed into the seed.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Sends one request and returns the `ok` value of the response.
pub fn call(plugin: &LoadedPlugin, request: &Value, seed: u64) -> Result<Value, Diagnostic> {
    let id = &plugin.manifest.plugin.id;
    let Some(runtime) = &plugin.runtime else {
        return Err(Diagnostic::new(
            Code::PluginFailed,
            format!("The plugin `{id}` has no code, but a document asked it to run."),
        ));
    };
    let bytes = serde_json::to_vec(request).expect("requests serialise");
    let response = runtime
        .call(&bytes, seed ^ hash(&bytes))
        .map_err(|e| match e {
            CallError::Timeout(m) => Diagnostic::new(
                Code::PluginTimeout,
                format!("The plugin `{id}` exceeded its time or memory budget: {m}."),
            ),
            CallError::Failed(m) => Diagnostic::new(
                Code::PluginFailed,
                format!("The plugin `{id}` failed: {m}."),
            ),
        })?;
    let value: Value = serde_json::from_slice(&response).map_err(|e| {
        Diagnostic::new(
            Code::PluginFailed,
            format!("The plugin `{id}` answered with invalid JSON: {e}."),
        )
    })?;
    if let Some(ok) = value.get("ok") {
        return Ok(ok.clone());
    }
    let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("no `ok` or `error` in the response");
    let mut d = Diagnostic::new(
        Code::PluginFailed,
        format!("The plugin `{id}` reported an error: {message}"),
    );
    if let Some(element) = value.pointer("/error/element").and_then(Value::as_str) {
        d = d.element(element);
    }
    Err(d)
}

fn bad(plugin: &LoadedPlugin, what: &str) -> Diagnostic {
    Diagnostic::new(
        Code::PluginFailed,
        format!(
            "The plugin `{}` returned {what}.",
            plugin.manifest.plugin.id
        ),
    )
}

fn check_nodes(plugin: &LoadedPlugin, nodes: &[Node]) -> Result<(), Diagnostic> {
    fn count(nodes: &[Node], n: &mut usize, ok: &mut bool) {
        for node in nodes {
            *n += 1;
            let cmds: &[Cmd] = match node {
                Node::Path { d, .. } => d,
                Node::Group { children, clip, .. } => {
                    count(children, n, ok);
                    clip.as_deref().unwrap_or(&[])
                }
                Node::Text { x, y, .. } => {
                    *ok &= x.is_finite()
                        && y.is_finite()
                        && x.abs() < MAX_COORDINATE
                        && y.abs() < MAX_COORDINATE;
                    &[]
                }
            };
            for c in cmds {
                let values: Vec<f64> = match *c {
                    Cmd::Move(a, b) | Cmd::Line(a, b) => vec![a, b],
                    Cmd::Arc { r, x, y, .. } => vec![r, x, y],
                    Cmd::Cubic(a, b, c, d, e, f) => vec![a, b, c, d, e, f],
                    Cmd::Close => vec![],
                };
                *ok &= values
                    .iter()
                    .all(|v| v.is_finite() && v.abs() < MAX_COORDINATE);
            }
        }
    }
    let (mut n, mut ok) = (0, true);
    count(nodes, &mut n, &mut ok);
    if n > MAX_NODES {
        return Err(bad(plugin, &format!("{n} nodes; the limit is {MAX_NODES}")));
    }
    if !ok {
        return Err(bad(
            plugin,
            "a coordinate that is not finite or is out of range",
        ));
    }
    Ok(())
}

fn check_scene(plugin: &LoadedPlugin, scene: &Scene) -> Result<(), Diagnostic> {
    let nodes: Vec<Node> = scene
        .layers
        .iter()
        .flat_map(|l| l.nodes.iter().cloned())
        .collect();
    check_nodes(plugin, &nodes)?;
    if scene.layers.len() != dedalo_scene::LAYERS.len()
        || scene
            .layers
            .iter()
            .zip(dedalo_scene::LAYERS)
            .any(|(l, n)| l.name != n)
    {
        return Err(bad(
            plugin,
            "a scene whose layers are not the standard ones",
        ));
    }
    Ok(())
}

/// Removes source annotations: player maps carry no element IDs (spec 9.2),
/// whatever a plugin emits.
pub fn strip_sources(node: &mut Node) {
    match node {
        Node::Group {
            source, children, ..
        } => {
            *source = None;
            children.iter_mut().for_each(strip_sources);
        }
        Node::Path { source, .. } | Node::Text { source, .. } => *source = None,
    }
}

/// A painter implemented by a plugin.
pub struct WasmPainter {
    pub plugin: Arc<LoadedPlugin>,
    pub decl: PainterDecl,
    pub options: Value,
    pub seed: u64,
    /// Failures, collected for the render to report.
    pub errors: Arc<Mutex<Vec<Diagnostic>>>,
}

fn item_json(item: &Item) -> Value {
    let (kind, value) = match item {
        Item::Space(x) => ("space", serde_json::to_value(x)),
        Item::Wall(x) => ("wall", serde_json::to_value(x)),
        Item::Opening(x) => ("opening", serde_json::to_value(x)),
        Item::Element(x) => ("element", serde_json::to_value(x)),
        Item::Note(x) => ("note", serde_json::to_value(x)),
    };
    let mut value = value.unwrap_or_default();
    value["kind"] = json!(kind);
    value
}

impl Painter for WasmPainter {
    fn paint(&self, item: &Item, cx: &Cx, scene: &mut Scene) {
        let request = json!({
            "point": "painter",
            "name": self.decl.name.clone().unwrap_or_default(),
            "input": { "item": item_json(item), "theme": cx.theme, "cell_px": cx.view.cell_px, "legend": cx.legend, "audience": cx.view.audience },
            "options": self.options,
        });
        let result = call(&self.plugin, &request, self.seed).and_then(|ok| {
            let nodes: Vec<Node> = serde_json::from_value(
                ok.get("nodes").cloned().unwrap_or(json!([])),
            )
            .map_err(|e| {
                bad(
                    &self.plugin,
                    &format!("nodes that are not scene nodes: {e}"),
                )
            })?;
            check_nodes(&self.plugin, &nodes)?;
            Ok(nodes)
        });
        match result {
            Ok(mut nodes) => {
                if cx.view.audience != Audience::Dm || cx.legend {
                    nodes.iter_mut().for_each(strip_sources);
                }
                for node in nodes {
                    scene.push(&self.decl.layer, node);
                }
            }
            Err(d) => self.errors.lock().expect("an error list").push(d),
        }
    }
}

/// Player maps carry no element IDs, whatever a plugin emits.
fn redact(mut scene: Scene, view: &View) -> Scene {
    if view.audience != Audience::Dm {
        scene
            .layers
            .iter_mut()
            .flat_map(|l| l.nodes.iter_mut())
            .for_each(strip_sources);
    }
    scene
}

/// Runs a scene pass.
pub fn scene_pass(
    plugin: &LoadedPlugin,
    name: &str,
    scene: Scene,
    view: &View,
    options: &Value,
) -> Result<Scene, Diagnostic> {
    let request = json!({ "point": "scene_pass", "name": name, "input": { "scene": scene, "cell_px": view.cell_px, "audience": view.audience }, "options": options });
    let ok = call(plugin, &request, view.seed)?;
    let out: Scene = serde_json::from_value(ok.get("scene").cloned().unwrap_or_default())
        .map_err(|e| bad(plugin, &format!("a scene that is not valid: {e}")))?;
    check_scene(plugin, &out)?;
    Ok(redact(out, view))
}

/// Runs a renderer: the whole view to a scene.
pub fn renderer(
    plugin: &LoadedPlugin,
    view: &View,
    theme: &Theme,
    options: &Value,
) -> Result<Scene, Diagnostic> {
    let request = json!({ "point": "renderer", "name": plugin.manifest.renderer.as_ref().map(|r| r.name.clone()), "input": { "view": view, "theme": theme }, "options": options });
    let ok = call(plugin, &request, view.seed)?;
    let out: Scene = serde_json::from_value(ok.get("scene").cloned().unwrap_or_default())
        .map_err(|e| bad(plugin, &format!("a scene that is not valid: {e}")))?;
    check_scene(plugin, &out)?;
    Ok(redact(out, view))
}

/// Runs an exporter. `svg` and `png` are the host's own renderings, given
/// when the manifest asks for them.
pub fn exporter(
    plugin: &LoadedPlugin,
    decl: &ExporterDecl,
    view: &View,
    scene: &Scene,
    svg: Option<&str>,
    png: Option<&[u8]>,
    options: &Value,
) -> Result<Vec<u8>, Diagnostic> {
    let mut input = json!({ "view": view, "scene": scene });
    if let Some(svg) = svg {
        input["svg"] = json!(svg);
    }
    if let Some(png) = png {
        input["png"] = json!(base64::encode(png));
    }
    let request =
        json!({ "point": "exporter", "name": decl.format, "input": input, "options": options });
    let ok = call(plugin, &request, view.seed)?;
    if let Some(text) = ok.get("text").and_then(Value::as_str) {
        return Ok(text.as_bytes().to_vec());
    }
    let encoded = ok
        .get("base64")
        .and_then(Value::as_str)
        .ok_or_else(|| bad(plugin, "neither `text` nor `base64` output"))?;
    base64::decode(encoded).ok_or_else(|| bad(plugin, "output that is not valid base64"))
}

/// Runs a validator on the resolved model.
pub fn validator(
    plugin: &LoadedPlugin,
    name: &str,
    resolved: &Value,
    options: &Value,
) -> Result<Vec<Diagnostic>, Diagnostic> {
    let request = json!({ "point": "validator", "name": name, "input": { "resolved": resolved }, "options": options });
    let ok = call(plugin, &request, 0)?;
    let list = ok
        .get("diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let id = &plugin.manifest.plugin.id;
    let mut out = Vec::new();
    for d in list.iter().take(1000) {
        let message = d
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| bad(plugin, "a diagnostic without a message"))?;
        let mut diagnostic = Diagnostic::new(Code::PluginRule, format!("{message} ({id}.{name})"));
        if d.get("severity").and_then(Value::as_str) == Some("error") {
            diagnostic.severity = Severity::Error;
        }
        if let Some(element) = d.get("element").and_then(Value::as_str) {
            diagnostic = diagnostic.element(element);
        }
        if let Some(s) = d.get("suggestion").and_then(Value::as_str) {
            diagnostic = diagnostic.suggest(s);
        }
        out.push(diagnostic);
    }
    Ok(out)
}

/// Runs a shape function: arguments to outline points, in cells.
pub fn shape(
    plugin: &LoadedPlugin,
    name: &str,
    args: &Value,
    seed: u64,
) -> Result<Vec<[f64; 2]>, String> {
    let request = json!({ "point": "shape", "name": name, "input": { "args": args } });
    let ok = call(plugin, &request, seed).map_err(|d| d.message)?;
    serde_json::from_value(ok.get("points").cloned().unwrap_or_default())
        .map_err(|e| format!("the plugin returned points that are not [x, y] pairs: {e}"))
}
