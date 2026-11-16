//! Reference plugin `uvtt` (spec 11.9): non-image output built from the
//! semantic model. A Universal VTT file carries the map image together with
//! its walls (line of sight), doors (portals) and lights, so a virtual
//! tabletop can use them directly.
//!
//! Coordinates are in grid squares from the top-left corner of the image.
//! The view the plugin receives is already filtered for the export's
//! audience, so secret doors hidden from players are plain walls here too.

use dedalo_pdk::{Request, Response, Value, json, plugin};

fn handle(request: Request) -> Response {
    match (request.point.as_str(), request.name.as_str()) {
        ("exporter", "dd2vtt") => export(&request),
        (point, name) => Err(format!("uvtt has no {point} named `{name}`").into()),
    }
}

plugin!(handle);

fn xy(v: &Value) -> Option<(f64, f64)> {
    Some((v.get(0)?.as_f64()?, v.get(1)?.as_f64()?))
}

/// Rounds to 1/1000 of a square, so the output is stable.
fn r(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn export(request: &Request) -> Response {
    let view = &request.input["view"];
    let scene = &request.input["scene"];
    let png = request.input["png"]
        .as_str()
        .ok_or("the host did not provide the PNG rendering")?;
    let bounds = scene["bounds"]
        .as_array()
        .ok_or("the scene has no bounds")?;
    let (ox, oy) = (
        bounds[0].as_f64().unwrap_or(0.0),
        bounds[1].as_f64().unwrap_or(0.0),
    );
    let (w, h) = (
        bounds[2].as_f64().unwrap_or(0.0),
        bounds[3].as_f64().unwrap_or(0.0),
    );
    let point = |x: f64, y: f64| json!({ "x": r(x - ox), "y": r(y - oy) });

    let mut line_of_sight = Vec::new();
    for wall in view["walls"].as_array().into_iter().flatten() {
        for seg in wall["solid"].as_array().into_iter().flatten() {
            if let (Some(a), Some(b)) = (xy(&seg[0]), xy(&seg[1])) {
                line_of_sight.push(json!([point(a.0, a.1), point(b.0, b.1)]));
            }
        }
        // A curved wall: arcs between its gaps, as short straight pieces.
        if let Some(c) = wall["circle"].as_object() {
            let (cx, cy) = xy(&c["center"]).ok_or("a circle without a centre")?;
            let radius = c["radius"].as_f64().unwrap_or(0.0);
            let radius_y = c["radius_y"].as_f64().unwrap_or(radius);
            let angle = |p: (f64, f64)| {
                let a = ((p.0 - cx) / radius).atan2(-(p.1 - cy) / radius_y);
                if a < 0.0 {
                    a + std::f64::consts::TAU
                } else {
                    a
                }
            };
            let gaps: Vec<((f64, f64), (f64, f64))> = wall["gaps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|g| Some((xy(&g[0])?, xy(&g[1])?)))
                .collect();
            let arcs: Vec<(f64, f64)> = if gaps.is_empty() {
                vec![(0.0, std::f64::consts::TAU)]
            } else {
                (0..gaps.len())
                    .map(|i| {
                        let from = angle(gaps[i].1);
                        let mut to = angle(gaps[(i + 1) % gaps.len()].0);
                        if to <= from {
                            to += std::f64::consts::TAU;
                        }
                        (from, to)
                    })
                    .collect()
            };
            for (from, to) in arcs {
                let steps = ((to - from) / std::f64::consts::TAU * 48.0).ceil().max(2.0) as usize;
                let pts: Vec<Value> = (0..=steps)
                    .map(|k| {
                        let a = from + (to - from) * k as f64 / steps as f64;
                        point(cx + radius * a.sin(), cy - radius_y * a.cos())
                    })
                    .collect();
                line_of_sight.push(Value::Array(pts));
            }
        }
    }

    let mut portals = Vec::new();
    for o in view["openings"].as_array().into_iter().flatten() {
        let ty = o["type"].as_str().unwrap_or_default();
        if !matches!(ty, "door" | "archway") {
            continue;
        }
        let Some(seg) = o["segment"].as_array() else {
            continue;
        };
        let (Some(a), Some(b)) = (xy(&seg[0]), xy(&seg[1])) else {
            continue;
        };
        portals.push(json!({
            "position": point((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
            "bounds": [point(a.0, a.1), point(b.0, b.1)],
            "rotation": r((b.1 - a.1).atan2(b.0 - a.0)),
            "closed": ty == "door",
            "freestanding": false,
        }));
    }

    let mut lights = Vec::new();
    for e in view["elements"].as_array().into_iter().flatten() {
        if e["type"] != "lighting" {
            continue;
        }
        let Some(bbox) = e["bbox"].as_array() else {
            continue;
        };
        let (x0, y0, x1, y1) = (
            bbox[0].as_f64().unwrap_or(0.0),
            bbox[1].as_f64().unwrap_or(0.0),
            bbox[2].as_f64().unwrap_or(0.0),
            bbox[3].as_f64().unwrap_or(0.0),
        );
        let color = e["properties"]["color"]
            .as_str()
            .unwrap_or("ffd27a")
            .trim_start_matches('#')
            .to_string();
        let dim = e["properties"]["intensity"] == "dim";
        lights.push(json!({
            "position": point((x0 + x1) / 2.0, (y0 + y1) / 2.0),
            "range": e["light_radius"].as_f64().unwrap_or(4.0),
            "intensity": if dim { 0.5 } else { 1.0 },
            "color": format!("ff{color}"),
            "shadows": true,
        }));
    }

    let document = json!({
        "format": 0.3,
        "resolution": {
            "map_origin": { "x": 0, "y": 0 },
            "map_size": { "x": r(w), "y": r(h) },
            "pixels_per_grid": scene["cell_px"],
        },
        "line_of_sight": line_of_sight,
        "objects_line_of_sight": [],
        "portals": portals,
        "environment": { "baked_lighting": true, "ambient_light": "ffffffff" },
        "lights": lights,
        "image": png,
    });
    Ok(json!({ "text": document.to_string() }))
}
