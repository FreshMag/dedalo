//! Stage 5 of the pipeline: view plus theme to scene (spec 9.4 and 9.5).
//!
//! The default renderer runs a painter over every view item, then adds the
//! grid, room labels, legend, notes panel and frame.

use dedalo_catalog::CATEGORIES;
use dedalo_geom::{Loc, Opening, Pt, RElement, Rect, locate};
use dedalo_model::{Class, LegendMode, LegendPosition, Props, Value, Visibility};
use dedalo_scene::shapes::{line, polygon};
use dedalo_scene::{Cmd, Node, Scene, Stroke, TextAnchor};

use crate::paint::{Cx, Item, Painters, f, outline, xy};
use crate::theme::Theme;
use crate::view::View;

/// Space around the map, in thousandths of a cell.
const PAD: i64 = 600;

/// A rough text width in cells. Exact metrics need the bundled fonts of
/// milestone M3; until then layout only needs to be stable and roomy.
fn text_width(text: &str, size: f64) -> f64 {
    text.chars().count() as f64 * size * 0.56
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

enum Sample {
    Opening {
        ty: String,
        secret: bool,
        locked: bool,
    },
    Element {
        ty: String,
        subtype: Option<String>,
        properties: Props,
    },
}

struct Entry {
    key: String,
    label: String,
    sample: Sample,
}

const OPENING_KEYS: [(&str, &str); 7] = [
    ("door", "Door"),
    ("locked_door", "Locked door"),
    ("secret_door", "Secret door"),
    ("archway", "Archway"),
    ("stairs", "Stairs"),
    ("ladder", "Ladder"),
    ("trapdoor", "Trapdoor"),
];

fn opening_sample(key: &str) -> Sample {
    let ty = if key.ends_with("door") { "door" } else { key };
    Sample::Opening {
        ty: ty.to_string(),
        secret: key == "secret_door",
        locked: key == "locked_door",
    }
}

/// The legend lists only what is present in the view (spec 9.2).
fn legend_entries(view: &View) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    if view.legend.mode == Some(LegendMode::Custom) {
        for custom in &view.legend.entries {
            let sample = if OPENING_KEYS.iter().any(|(k, _)| *k == custom.key) {
                opening_sample(&custom.key)
            } else {
                let (ty, subtype) = custom
                    .key
                    .split_once('.')
                    .map_or((custom.key.as_str(), None), |(t, s)| {
                        (t, Some(s.to_string()))
                    });
                Sample::Element {
                    ty: ty.to_string(),
                    subtype,
                    properties: Props::new(),
                }
            };
            entries.push(Entry {
                key: custom.key.clone(),
                label: custom.label.clone().unwrap_or_else(|| custom.key.clone()),
                sample,
            });
        }
        return entries;
    }
    for (key, label) in OPENING_KEYS {
        let present = view.openings.iter().filter(|o| !o.implicit).any(|o| {
            let own = if o.ty != "door" {
                o.ty.as_str()
            } else if o.secret {
                "secret_door"
            } else if o.locked {
                "locked_door"
            } else {
                "door"
            };
            own == key
        });
        if present {
            entries.push(Entry {
                key: key.to_string(),
                label: label.to_string(),
                sample: opening_sample(key),
            });
        }
    }
    for category in CATEGORIES {
        for e in view.elements.iter().filter(|e| e.category == *category) {
            let symbol = e
                .properties
                .get("symbol")
                .and_then(Value::as_str)
                .filter(|_| e.ty == "custom");
            let key = match (symbol, &e.subtype) {
                (Some(s), _) => format!("custom.{s}"),
                (None, Some(subtype)) => format!("{}.{subtype}", e.ty),
                (None, None) => e.ty.clone(),
            };
            if !entries.iter().any(|x| x.key == key) {
                let sample = Sample::Element {
                    ty: e.ty.clone(),
                    subtype: e.subtype.clone(),
                    properties: e.properties.clone(),
                };
                entries.push(Entry {
                    key,
                    label: e.label.clone(),
                    sample,
                });
            }
        }
    }
    // Entry blocks override labels, which is also how a map is localised.
    for entry in &mut entries {
        let ty = entry.key.split('.').next().unwrap_or_default();
        let over = view
            .legend
            .entries
            .iter()
            .find(|x| x.key == entry.key)
            .or_else(|| view.legend.entries.iter().find(|x| x.key == ty));
        if let Some(label) = over.and_then(|x| x.label.clone()) {
            entry.label = label;
        }
    }
    entries
}

fn sample_element(
    ty: &str,
    subtype: &Option<String>,
    properties: &Props,
    x: i64,
    y: i64,
) -> RElement {
    let bbox = Rect {
        x0: x,
        y0: y,
        x1: x + 1000,
        y1: y + 1000,
    };
    RElement {
        id: String::new(),
        class: Class::Feature,
        ty: ty.to_string(),
        subtype: subtype.clone(),
        category: String::new(),
        label: String::new(),
        type_label: String::new(),
        glyph: String::new(),
        layer: "frame".to_string(),
        name: None,
        space: None,
        footprint: bbox.corners().to_vec(),
        bbox,
        rotation: 0,
        facing: None,
        wall: None,
        visibility: Visibility::Public,
        reveal_when: None,
        blocking: false,
        reference: None,
        contained_in: None,
        light_radius: None,
        properties: properties
            .iter()
            .filter(|(k, _)| *k == "symbol")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        tags: Vec::new(),
    }
}

fn sample_opening(ty: &str, secret: bool, locked: bool, x: i64, y: i64) -> Opening {
    let on_wall = !matches!(ty, "stairs" | "ladder" | "trapdoor");
    Opening {
        id: String::new(),
        from: String::new(),
        to: None,
        ty: ty.to_string(),
        secret,
        locked,
        visibility: Visibility::Public,
        reveal_when: None,
        implicit: false,
        segment: on_wall.then(|| [Pt::new(x + 200, y + 500), Pt::new(x + 800, y + 500)]),
        far: None,
        width: 600,
        stub: None,
        cell: (!on_wall).then_some(Rect {
            x0: x,
            y0: y,
            x1: x + 1000,
            y1: y + 1000,
        }),
        name: None,
    }
}

enum Row {
    Header(String),
    Sample(usize),
    Text(String),
    Gap,
}

impl Row {
    fn height(&self) -> i64 {
        match self {
            Row::Header(_) => 800,
            Row::Sample(_) => 1100,
            Row::Text(_) => 480,
            Row::Gap => 300,
        }
    }
}

/// Paints a view. Replacing the painters replaces the look.
pub fn render_scene(view: &View, theme: &Theme, painters: &Painters) -> Scene {
    let cx = Cx {
        view,
        theme,
        legend: false,
    };
    let ink = theme.color("ink");
    let map = view.bounds;

    // The panel beside the map: legend, then notes.
    let entries = if view.show_legend {
        legend_entries(view)
    } else {
        Vec::new()
    };
    let (legend_size, note_size) = (theme.size("legend"), theme.size("note"));
    let mut rows: Vec<Row> = Vec::new();
    if !entries.is_empty() {
        rows.push(Row::Header(
            view.legend
                .title
                .clone()
                .unwrap_or_else(|| "Legend".to_string()),
        ));
        rows.extend((0..entries.len()).map(Row::Sample));
    }
    let has_notes = view.show_frame && (!view.notes.is_empty() || !view.contents.is_empty());
    let widest = entries
        .iter()
        .map(|e| text_width(&e.label, legend_size))
        .fold(0.0, f64::max);
    let panel_w = (((1.5 + widest + 0.3).max(if has_notes { 8.0 } else { 3.0 }) * 1000.0).ceil()
        as i64)
        .min(14_000);
    if has_notes {
        if !rows.is_empty() {
            rows.push(Row::Gap);
        }
        rows.push(Row::Header("Notes".to_string()));
        let columns = ((f(panel_w) - 0.2) / (note_size * 0.56)) as usize;
        for note in &view.notes {
            rows.extend(
                wrap(&format!("{}. {}", note.number, note.text), columns.max(8))
                    .into_iter()
                    .map(Row::Text),
            );
        }
        for (holder, items) in &view.contents {
            rows.extend(
                wrap(&format!("{holder}: {}", items.join(", ")), columns.max(8))
                    .into_iter()
                    .map(Row::Text),
            );
        }
    }
    let panel_h: i64 = rows.iter().map(Row::height).sum();

    // Canvas layout, in thousandths of a cell.
    let scale_text = view
        .grid_unit
        .as_ref()
        .filter(|_| view.show_frame)
        .map(|unit| format!("1 square = {unit}"));
    let scale_h = if scale_text.is_some() { 700 } else { 0 };
    let mut canvas = Rect {
        x0: map.x0 - PAD,
        y0: map.y0 - PAD,
        x1: map.x1 + PAD,
        y1: map.y1 + PAD + scale_h,
    };
    let panel = (!rows.is_empty()).then(|| {
        let (x, y) = match view.legend.position.unwrap_or(LegendPosition::Right) {
            LegendPosition::Right => (map.x1 + PAD + 400, map.y0),
            LegendPosition::Left => (map.x0 - PAD - 400 - panel_w, map.y0),
            LegendPosition::Bottom => (map.x0, map.y1 + PAD + scale_h + 300),
            LegendPosition::Top => (map.x0, map.y0 - PAD - 300 - panel_h),
        };
        Rect {
            x0: x,
            y0: y,
            x1: x + panel_w,
            y1: y + panel_h,
        }
    });
    if let Some(p) = panel {
        canvas = canvas.union(&Rect {
            x0: p.x0 - 300,
            y0: p.y0 - 300,
            x1: p.x1 + 300,
            y1: p.y1 + 300,
        });
    }
    let north_arrow = view.show_frame && view.rotation != 0;
    let title_h = if view.title.is_some() || north_arrow {
        1200
    } else {
        0
    };
    canvas.y0 -= title_h;

    let mut scene = Scene::new(
        [f(canvas.x0), f(canvas.y0), f(canvas.w()), f(canvas.h())],
        view.cell_px,
    );
    scene.push(
        "background",
        Node::fill(
            polygon(&canvas.corners().map(xy)),
            theme.color("background"),
        ),
    );

    for space in &view.spaces {
        painters.paint(&Item::Space(space), &cx, &mut scene);
    }
    for opening in &view.openings {
        painters.paint(&Item::Opening(opening), &cx, &mut scene);
    }
    for wall in &view.walls {
        painters.paint(&Item::Wall(wall), &cx, &mut scene);
    }
    for element in &view.elements {
        painters.paint(&Item::Element(element), &cx, &mut scene);
    }
    for note in &view.notes {
        painters.paint(&Item::Note(note), &cx, &mut scene);
    }

    if view.show_grid {
        // A light grid, clipped to floors.
        let stroke = Stroke::new(theme.color("grid"), theme.stroke("grid"));
        let stubs = view
            .openings
            .iter()
            .filter_map(|o| o.stub.as_ref())
            .map(|stub| {
                (
                    Rect::of(stub),
                    polygon(&stub.iter().map(|p| xy(*p)).collect::<Vec<_>>()),
                )
            });
        let areas: Vec<(Rect, Vec<Cmd>)> = view
            .spaces
            .iter()
            .map(|s| (s.bbox, outline(s)))
            .chain(stubs)
            .collect();
        for (bbox, clip) in areas {
            let mut d = Vec::new();
            for x in bbox.x0.div_euclid(1000) + 1..=(bbox.x1 - 1).div_euclid(1000) {
                d.extend(line((x as f64, f(bbox.y0)), (x as f64, f(bbox.y1))));
            }
            for y in bbox.y0.div_euclid(1000) + 1..=(bbox.y1 - 1).div_euclid(1000) {
                d.extend(line((f(bbox.x0), y as f64), (f(bbox.x1), y as f64)));
            }
            if !d.is_empty() {
                scene.push(
                    "grid",
                    Node::Group {
                        transform: None,
                        clip: Some(clip),
                        opacity: None,
                        children: vec![Node::stroke(d, stroke.clone())],
                        source: None,
                    },
                );
            }
        }
    }

    if view.show_labels {
        let size = theme.size("room");
        for space in &view.spaces {
            let text = if view.room_ids {
                Some(&space.id)
            } else {
                space.name.as_ref()
            };
            let Some(text) = text else { continue };
            let (half_w, half_h) = (text_width(text, size) / 2.0, size / 2.0);
            let (x, centre) = (
                f(space.bbox.x0 + space.bbox.x1) / 2.0,
                f(space.bbox.y0 + space.bbox.y1) / 2.0,
            );
            // Keep the label inside the room and off whatever stands in it.
            let obstacles: Vec<Rect> = view
                .elements
                .iter()
                .filter(|e| e.space.as_ref() == Some(&space.id))
                .map(|e| e.bbox)
                .chain(
                    view.openings
                        .iter()
                        .filter(|o| o.from == space.id)
                        .filter_map(|o| o.cell),
                )
                .collect();
            let inside = |px: f64, py: f64| {
                let p = Pt::new((px * 1000.0).round() as i64, (py * 1000.0).round() as i64);
                locate(p, &space.polygon) != Loc::Outside
            };
            let free = |y: f64| {
                [-1.0, 0.0, 1.0].iter().all(|k| {
                    inside(x + k * half_w, y - half_h) && inside(x + k * half_w, y + half_h)
                }) && !obstacles.iter().any(|o| {
                    x - half_w < f(o.x1)
                        && f(o.x0) < x + half_w
                        && y - half_h < f(o.y1)
                        && f(o.y0) < y + half_h
                })
            };
            // Try the centre, then rows of half a cell above and below it.
            let rows = (f(space.bbox.h()) as i64).clamp(1, 40);
            let y = (0..=rows)
                .flat_map(|k| [centre - 0.5 * k as f64, centre + 0.5 * k as f64])
                .find(|y| free(*y))
                .unwrap_or(centre);
            scene.push(
                "labels",
                Node::text(x, y, text, size, TextAnchor::Middle, ink)
                    .halo(theme.color("floor"), 0.12)
                    .with_source(cx.source(&space.id, "label")),
            );
        }
    }

    if let Some(p) = panel {
        let legend_cx = Cx {
            view,
            theme,
            legend: true,
        };
        let mut y = p.y0;
        for row in &rows {
            let middle = f(y) + f(row.height()) / 2.0;
            match row {
                Row::Header(text) => scene.push(
                    "frame",
                    Node::text(
                        f(p.x0),
                        middle,
                        text,
                        legend_size * 1.1,
                        TextAnchor::Start,
                        ink,
                    )
                    .bold(),
                ),
                Row::Text(text) => scene.push(
                    "frame",
                    Node::text(f(p.x0), middle, text, note_size, TextAnchor::Start, ink),
                ),
                Row::Gap => {}
                Row::Sample(i) => {
                    let entry = &entries[*i];
                    let (sx, sy) = (p.x0, y + 50);
                    match &entry.sample {
                        Sample::Element {
                            ty,
                            subtype,
                            properties,
                        } => {
                            let element = sample_element(ty, subtype, properties, sx, sy);
                            painters.paint(&Item::Element(&element), &legend_cx, &mut scene);
                        }
                        Sample::Opening { ty, secret, locked } => {
                            let opening = sample_opening(ty, *secret, *locked, sx, sy);
                            if let Some([a, b]) = opening.segment {
                                // A short piece of wall on each side of the opening.
                                let wall =
                                    Stroke::new(theme.color("wall"), theme.stroke("wall")).round();
                                let mut d = line((f(sx), f(a.y)), xy(a));
                                d.extend(line(xy(b), (f(sx + 1000), f(b.y))));
                                scene.push("frame", Node::stroke(d, wall));
                            }
                            painters.paint(&Item::Opening(&opening), &legend_cx, &mut scene);
                        }
                    }
                    scene.push(
                        "frame",
                        Node::text(
                            f(sx) + 1.4,
                            f(sy) + 0.5,
                            &entry.label,
                            legend_size,
                            TextAnchor::Start,
                            ink,
                        ),
                    );
                }
            }
            y += row.height();
        }
    }

    if let Some(title) = &view.title {
        scene.push(
            "frame",
            Node::text(
                f(canvas.x0 + PAD),
                f(canvas.y0) + 0.75,
                title,
                theme.size("title"),
                TextAnchor::Start,
                ink,
            )
            .bold(),
        );
    }
    if let Some(text) = scale_text {
        scene.push(
            "frame",
            Node::text(
                f(map.x0),
                f(map.y1 + PAD) + 0.3,
                &text,
                legend_size * 0.9,
                TextAnchor::Start,
                theme.color("muted"),
            ),
        );
    }
    if north_arrow {
        // North points where the unrotated page's top went.
        let (cx0, cy0) = (f(map.x1) - 0.4, f(canvas.y0) + 0.65);
        let (dx, dy) = match view.rotation {
            90 => (1.0, 0.0),
            180 => (0.0, 1.0),
            270 => (-1.0, 0.0),
            _ => (0.0, -1.0),
        };
        let tip = (cx0 + dx * 0.4, cy0 + dy * 0.4);
        let tail = |side: f64| {
            (
                cx0 - dx * 0.3 - dy * 0.22 * side,
                cy0 - dy * 0.3 + dx * 0.22 * side,
            )
        };
        scene.push(
            "frame",
            Node::fill(
                polygon(&[
                    tip,
                    tail(1.0),
                    (cx0 - dx * 0.12, cy0 - dy * 0.12),
                    tail(-1.0),
                ]),
                ink,
            ),
        );
        scene.push(
            "frame",
            Node::text(
                cx0 - dx * 0.62,
                cy0 - dy * 0.62,
                "N",
                legend_size * 0.9,
                TextAnchor::Middle,
                ink,
            )
            .bold(),
        );
    }
    scene
}
