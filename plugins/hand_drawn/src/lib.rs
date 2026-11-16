//! Reference plugin `hand_drawn` (spec 11.9): proves that the default output
//! can be restyled completely from outside the core.
//!
//! - Scene pass `wobble`: every straight stroke becomes a slightly bent ink
//!   line, and walls get a second, fainter pen stroke.
//! - Painter `door`: a sketched door leaf with cross-hatching.
//! - Theme `sepia`: brown ink on parchment.
//!
//! All randomness comes from the host's seeded stream, so the same map
//! always gets the same wobble.

use dedalo_pdk::{
    Cmd, Node, Paint, Request, Response, Scene, Stroke, TextAnchor, Value, json, plugin, rand_f64,
};

fn handle(request: Request) -> Response {
    match (request.point.as_str(), request.name.as_str()) {
        ("scene_pass", "wobble") => wobble(&request),
        ("painter", "door") => door(&request),
        (point, name) => Err(format!("hand_drawn has no {point} named `{name}`").into()),
    }
}

plugin!(handle);

/// A random offset in `[-amount, amount]`.
fn jitter(amount: f64) -> f64 {
    (rand_f64() * 2.0 - 1.0) * amount
}

/// Bends each straight segment of a path into a gentle cubic.
fn bend(d: &[Cmd], amount: f64) -> Vec<Cmd> {
    let mut out = Vec::with_capacity(d.len());
    let mut at = (0.0, 0.0);
    let mut start = (0.0, 0.0);
    for cmd in d {
        match *cmd {
            Cmd::Move(x, y) => {
                at = (x + jitter(amount * 0.3), y + jitter(amount * 0.3));
                start = at;
                out.push(Cmd::Move(at.0, at.1));
            }
            Cmd::Line(x, y) => {
                let (dx, dy) = (x - at.0, y - at.1);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-9 {
                    continue;
                }
                // Control points pushed sideways, more on longer lines.
                let (nx, ny) = (-dy / len, dx / len);
                let k = amount * len.sqrt().min(3.0) * 0.5;
                let (a, b) = (jitter(k), jitter(k));
                let end = (x + jitter(amount * 0.3), y + jitter(amount * 0.3));
                out.push(Cmd::Cubic(
                    at.0 + dx / 3.0 + nx * a,
                    at.1 + dy / 3.0 + ny * a,
                    at.0 + 2.0 * dx / 3.0 + nx * b,
                    at.1 + 2.0 * dy / 3.0 + ny * b,
                    end.0,
                    end.1,
                ));
                at = end;
            }
            Cmd::Close => {
                // Close with a bent line back to the start.
                let (dx, dy) = (start.0 - at.0, start.1 - at.1);
                if dx.abs() + dy.abs() > 1e-9 {
                    out.push(Cmd::Cubic(
                        at.0 + dx / 3.0 + jitter(amount * 0.2),
                        at.1 + dy / 3.0 + jitter(amount * 0.2),
                        at.0 + 2.0 * dx / 3.0,
                        at.1 + 2.0 * dy / 3.0,
                        start.0,
                        start.1,
                    ));
                }
                out.push(Cmd::Close);
                at = start;
            }
            other => {
                if let Some(end) = dedalo_pdk::geometry::end(&other) {
                    at = end;
                }
                out.push(other);
            }
        }
    }
    out
}

fn wobble_nodes(nodes: Vec<Node>, amount: f64, layer: &str, out: &mut Vec<Node>) {
    for node in nodes {
        match node {
            Node::Path {
                d,
                fill,
                stroke,
                source,
            } => {
                let bent = bend(&d, amount);
                // Walls get a second, fainter pen stroke.
                if layer == "walls"
                    && let Some(s) = &stroke
                {
                    let mut faint = s.clone();
                    faint.width *= 0.45;
                    out.push(Node::Group {
                        transform: None,
                        clip: None,
                        opacity: Some(0.55),
                        children: vec![Node::Path {
                            d: bend(&d, amount * 1.4),
                            fill: None,
                            stroke: Some(faint),
                            source: None,
                        }],
                        source: None,
                    });
                }
                out.push(Node::Path {
                    d: bent,
                    fill,
                    stroke,
                    source,
                });
            }
            Node::Group {
                transform,
                clip,
                opacity,
                children,
                source,
            } => {
                let mut inner = Vec::new();
                wobble_nodes(children, amount, layer, &mut inner);
                out.push(Node::Group {
                    transform,
                    clip,
                    opacity,
                    children: inner,
                    source,
                });
            }
            text @ Node::Text { .. } => out.push(text),
        }
    }
}

fn wobble(request: &Request) -> Response {
    let mut scene: Scene = dedalo_pdk::serde_json::from_value(request.input["scene"].clone())
        .map_err(|e| format!("the scene is not valid: {e}"))?;
    // In cells: 0.6 is a light tremor, 2 is a shaky hand.
    let amount = request.option_f64("wobble", 0.6).clamp(0.0, 2.0) * 0.06;
    for layer in &mut scene.layers {
        // The background and the grid stay straight; text stays legible.
        if matches!(layer.name.as_str(), "background" | "grid" | "labels") {
            continue;
        }
        let nodes = std::mem::take(&mut layer.nodes);
        let mut out = Vec::with_capacity(nodes.len());
        wobble_nodes(nodes, amount, &layer.name, &mut out);
        layer.nodes = out;
    }
    Ok(json!({ "scene": scene }))
}

fn color(theme: &Value, name: &str, default: &str) -> String {
    theme["palette"][name]
        .as_str()
        .unwrap_or(default)
        .to_string()
}

/// A door: a leaf across the opening, cross-hatched; `S` for a secret door.
fn door(request: &Request) -> Response {
    let item = &request.input["item"];
    let theme = &request.input["theme"];
    let ink = color(theme, "ink", "#000000");
    let floor = color(theme, "floor", "#ffffff");
    let mut nodes = Vec::new();
    // A stub passage gets a floor.
    if let Some(stub) = item["stub"].as_array() {
        let pts: Vec<(f64, f64)> = stub
            .iter()
            .filter_map(|p| Some((p[0].as_f64()?, p[1].as_f64()?)))
            .collect();
        nodes.push(Node::fill(dedalo_pdk::scene::shapes::polygon(&pts), &floor));
    }
    let seg = item["segment"]
        .as_array()
        .ok_or("a door without a segment")?;
    let p = |i: usize| -> Option<(f64, f64)> {
        Some((seg.get(i)?[0].as_f64()?, seg.get(i)?[1].as_f64()?))
    };
    let (a, b) = (p(0).ok_or("bad segment")?, p(1).ok_or("bad segment")?);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    let (u, n) = ((dx / len, dy / len), (-dy / len, dx / len));
    let at = |along: f64, across: f64| {
        (
            a.0 + u.0 * along + n.0 * across,
            a.1 + u.1 * along + n.1 * across,
        )
    };
    if item["secret"].as_bool() == Some(true) {
        let c = at(len / 2.0, 0.0);
        nodes.push(Node::stroke(
            dedalo_pdk::scene::shapes::line(at(0.0, 0.0), at(len, 0.0)),
            Stroke::new(&color(theme, "wall", "#000000"), 0.12).round(),
        ));
        nodes.push(
            Node::text(c.0, c.1, "S", 0.5, TextAnchor::Middle, &ink)
                .bold()
                .halo(&floor, 0.14),
        );
    } else {
        let half = 0.17;
        let leaf = dedalo_pdk::scene::shapes::polygon(&[
            at(0.0, -half),
            at(len, -half),
            at(len, half),
            at(0.0, half),
        ]);
        nodes.push(Node::path(
            leaf,
            Some(Paint::solid(&floor)),
            Some(Stroke::new(&ink, 0.05).round()),
        ));
        // Hatching, every quarter cell.
        let mut d = Vec::new();
        let mut t = 0.15;
        while t < len - 0.05 {
            let (p0, p1) = (at(t, -half), at((t + 0.2).min(len), half));
            d.push(Cmd::Move(p0.0, p0.1));
            d.push(Cmd::Line(p1.0, p1.1));
            t += 0.25;
        }
        nodes.push(Node::stroke(d, Stroke::new(&ink, 0.025).round()));
        if item["locked"].as_bool() == Some(true) {
            let c = at(len / 2.0, 0.0);
            nodes.push(Node::fill(
                dedalo_pdk::scene::shapes::circle(c.0, c.1, 0.08),
                &ink,
            ));
        }
    }
    // Element IDs go on DM maps only.
    let dm = request.input["audience"] == "dm" && request.input["legend"] != json!(true);
    let mut group = Node::group(nodes);
    if let Some(id) = item["id"].as_str().filter(|id| dm && !id.is_empty()) {
        group = group.with_source(Some(dedalo_pdk::scene::Source {
            id: id.to_string(),
            role: "door".to_string(),
        }));
    }
    Ok(json!({ "nodes": [group] }))
}
