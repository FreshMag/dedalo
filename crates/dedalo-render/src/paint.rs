//! Painters: one view item to scene nodes on one layer (spec 9.4).
//!
//! The registry picks the most specific painter registered for an item:
//! kind, type and subtype; then kind and type; then kind. Built-in painters
//! register through the same calls a plugin's painters will.

use std::collections::BTreeMap;

use dedalo_geom::{Opening, Pt, RElement, RNote, RSpace, Wall};
use dedalo_model::{Direction, Facing, Value};
use dedalo_scene::shapes::{circle, ellipse, line, polygon, rect};
use dedalo_scene::{Cmd, Node, Paint, Scene, Source, Stroke, TextAnchor};

use crate::theme::Theme;
use crate::view::{Audience, View};

/// One thing to paint.
#[derive(Clone, Copy, Debug)]
pub enum Item<'a> {
    Space(&'a RSpace),
    Wall(&'a Wall),
    Opening(&'a Opening),
    Element(&'a RElement),
    Note(&'a RNote),
}

/// What a painter may consult besides its item.
pub struct Cx<'a> {
    pub view: &'a View,
    pub theme: &'a Theme,
    /// True while drawing a legend sample: no halos, no annotations.
    pub legend: bool,
}

impl Cx<'_> {
    /// The source annotation of an item. Emitted only for the DM audience,
    /// so a player map carries no element IDs (spec 9.2).
    pub fn source(&self, id: &str, role: &str) -> Option<Source> {
        (self.view.audience == Audience::Dm && !self.legend && !id.is_empty()).then(|| Source {
            id: id.to_string(),
            role: role.to_string(),
        })
    }
}

pub trait Painter: Send + Sync {
    fn paint(&self, item: &Item, cx: &Cx, scene: &mut Scene);
}

impl<F: Fn(&Item, &Cx, &mut Scene) + Send + Sync> Painter for F {
    fn paint(&self, item: &Item, cx: &Cx, scene: &mut Scene) {
        self(item, cx, scene)
    }
}

/// The painter registry.
#[derive(Default)]
pub struct Painters {
    map: BTreeMap<(String, String, String), Box<dyn Painter>>,
}

impl Painters {
    /// Registers a painter for a kind (`space`, `wall`, `opening`,
    /// `element`, `note`), optionally narrowed to a type and subtype.
    pub fn register(
        &mut self,
        kind: &str,
        ty: &str,
        subtype: &str,
        painter: impl Painter + 'static,
    ) {
        self.map.insert(
            (kind.to_string(), ty.to_string(), subtype.to_string()),
            Box::new(painter),
        );
    }

    /// The most specific painter for an item.
    pub fn find(&self, kind: &str, ty: &str, subtype: &str) -> Option<&dyn Painter> {
        [(ty, subtype), (ty, ""), ("", "")]
            .iter()
            .find_map(|(t, s)| {
                self.map
                    .get(&(kind.to_string(), t.to_string(), s.to_string()))
            })
            .map(|p| p.as_ref())
    }

    pub fn paint(&self, item: &Item, cx: &Cx, scene: &mut Scene) {
        let (kind, ty, subtype) = match item {
            Item::Space(_) => ("space", "", ""),
            Item::Wall(_) => ("wall", "", ""),
            Item::Opening(o) => ("opening", o.ty.as_str(), ""),
            Item::Element(e) => ("element", e.ty.as_str(), e.subtype.as_deref().unwrap_or("")),
            Item::Note(_) => ("note", "", ""),
        };
        if let Some(painter) = self.find(kind, ty, subtype) {
            painter.paint(item, cx, scene);
        }
    }

    /// The painters of the built-in style.
    pub fn builtin() -> Painters {
        let mut p = Painters::default();
        p.register("space", "", "", floor);
        p.register("wall", "", "", wall);
        p.register("opening", "", "", passage);
        p.register("opening", "door", "", door);
        p.register("opening", "archway", "", archway);
        for ty in ["stairs", "ladder", "trapdoor"] {
            p.register("opening", ty, "", exit);
        }
        p.register("note", "", "", note_marker);
        p.register("element", "", "", marker);
        p.register("element", "architecture", "", column);
        p.register("element", "furniture", "", furniture);
        p.register("element", "lighting", "", light);
        p.register("element", "terrain", "", terrain);
        p.register("element", "container", "", chest);
        p.register("element", "creature", "", token);
        p.register("element", "npc", "", token);
        p
    }
}

// ----- helpers ------------------------------------------------------------------

/// Thousandths of a cell to cells.
pub(crate) fn f(v: i64) -> f64 {
    v as f64 / 1000.0
}

pub(crate) fn xy(p: Pt) -> (f64, f64) {
    (f(p.x), f(p.y))
}

pub(crate) fn outline(space: &RSpace) -> Vec<Cmd> {
    match space.circle {
        Some(c) => ellipse(f(c.center.x), f(c.center.y), f(c.rx()), f(c.ry())),
        None => polygon(&space.polygon.iter().map(|p| xy(*p)).collect::<Vec<_>>()),
    }
}

/// The frame of an element's footprint: x, y, width, height.
fn frame(e: &RElement) -> (f64, f64, f64, f64) {
    let (mut x, mut y) = (f(e.bbox.x0), f(e.bbox.y0));
    // A wall-mounted light hugs its wall.
    if let Some(wall) = e.wall.filter(|_| e.ty == "lighting") {
        let shift = 0.3;
        match wall {
            Direction::North => y -= shift,
            Direction::South => y += shift,
            Direction::East => x += shift,
            Direction::West => x -= shift,
        }
    }
    (x, y, f(e.bbox.w()), f(e.bbox.h()))
}

fn ink(cx: &Cx) -> Stroke {
    Stroke::new(cx.theme.color("ink"), cx.theme.stroke("symbol"))
}

fn white_fill(cx: &Cx) -> Option<Paint> {
    Some(Paint::solid(cx.theme.color("floor")))
}

fn element<'a>(item: &Item<'a>) -> Option<&'a RElement> {
    match item {
        Item::Element(e) => Some(e),
        _ => None,
    }
}

fn push(scene: &mut Scene, cx: &Cx, e: &RElement, nodes: Vec<Node>) {
    scene.push(
        &e.layer,
        Node::group(nodes).with_source(cx.source(&e.id, &e.ty)),
    );
}

// ----- structure -----------------------------------------------------------------

fn floor(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Item::Space(space) = item else { return };
    scene.push(
        "floor",
        Node::fill(outline(space), cx.theme.color("floor"))
            .with_source(cx.source(&space.id, "floor")),
    );
}

fn wall(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Item::Wall(wall) = item else { return };
    let stroke = Stroke::new(cx.theme.color("wall"), cx.theme.stroke("wall")).round();
    let mut d: Vec<Cmd> = Vec::new();
    for [a, b] in &wall.solid {
        d.extend(line(xy(*a), xy(*b)));
    }
    if let Some(c) = wall.circle {
        let (cx0, cy0, rx, ry) = (f(c.center.x), f(c.center.y), f(c.rx()), f(c.ry()));
        if wall.gaps.is_empty() {
            d.extend(ellipse(cx0, cy0, rx, ry));
        }
        // An arc from the end of each gap to the start of the next.
        for (i, gap) in wall.gaps.iter().enumerate() {
            let next = wall.gaps[(i + 1) % wall.gaps.len()];
            let mut sweep = c.angle(next[0]) - c.angle(gap[1]);
            if sweep <= 0.0 {
                sweep += 2.0 * std::f64::consts::PI;
            }
            let (from, to) = (xy(gap[1]), xy(next[0]));
            d.push(Cmd::Move(from.0, from.1));
            d.push(Cmd::Arc {
                r: rx,
                ry: (rx != ry).then_some(ry),
                large: sweep > std::f64::consts::PI,
                clockwise: true,
                x: to.0,
                y: to.1,
            });
        }
    }
    if !d.is_empty() {
        scene.push("walls", Node::stroke(d, stroke));
    }
}

/// The floor of a stub passage. Every opening type starts with this.
fn stub_floor(o: &Opening, cx: &Cx, scene: &mut Scene) {
    if let Some(stub) = &o.stub {
        scene.push(
            "floor",
            Node::fill(
                polygon(&stub.iter().map(|p| xy(*p)).collect::<Vec<_>>()),
                cx.theme.color("floor"),
            ),
        );
    }
}

/// A point or direction in cells.
type Xy = (f64, f64);

/// The opening's segment as start, unit direction, unit normal and length.
fn span(o: &Opening) -> Option<(Xy, Xy, Xy, f64)> {
    let [a, b] = o.segment?;
    let (a, b) = (xy(a), xy(b));
    let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    if len == 0.0 {
        return None;
    }
    let u = ((b.0 - a.0) / len, (b.1 - a.1) / len);
    Some((a, u, (-u.1, u.0), len))
}

/// An open passage: a plain gap.
fn passage(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Item::Opening(o) = item else { return };
    stub_floor(o, cx, scene);
}

fn door(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Item::Opening(o) = item else { return };
    stub_floor(o, cx, scene);
    let Some((a, u, n, len)) = span(o) else {
        return;
    };
    let at = |along: f64, across: f64| {
        (
            a.0 + u.0 * along + n.0 * across,
            a.1 + u.1 * along + n.1 * across,
        )
    };
    let mut nodes = Vec::new();
    if o.secret {
        // The letter S across an otherwise continuous wall.
        nodes.push(Node::stroke(
            line(at(0.0, 0.0), at(len, 0.0)),
            Stroke::new(cx.theme.color("wall"), cx.theme.stroke("wall")).round(),
        ));
        let c = at(len / 2.0, 0.0);
        nodes.push(
            Node::text(
                c.0,
                c.1,
                "S",
                cx.theme.size("marker"),
                TextAnchor::Middle,
                cx.theme.color("ink"),
            )
            .bold()
            .halo(cx.theme.color("floor"), 0.14),
        );
    } else {
        let half = 0.16;
        let leaf = polygon(&[at(0.0, -half), at(len, -half), at(len, half), at(0.0, half)]);
        nodes.push(Node::path(
            leaf,
            white_fill(cx),
            Some(Stroke::new(cx.theme.color("ink"), cx.theme.stroke("door"))),
        ));
        if o.locked {
            let c = at(len / 2.0, 0.0);
            nodes.push(Node::fill(circle(c.0, c.1, 0.07), cx.theme.color("ink")));
        }
    }
    scene.push(
        "openings",
        Node::group(nodes).with_source(cx.source(&o.id, "door")),
    );
}

/// A gap with two posts.
fn archway(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Item::Opening(o) = item else { return };
    stub_floor(o, cx, scene);
    let Some((a, u, _, len)) = span(o) else {
        return;
    };
    let post = 0.11;
    let nodes = [0.0, len]
        .iter()
        .map(|t| {
            Node::fill(
                rect(
                    a.0 + u.0 * t - post,
                    a.1 + u.1 * t - post,
                    2.0 * post,
                    2.0 * post,
                ),
                cx.theme.color("wall"),
            )
        })
        .collect();
    scene.push(
        "openings",
        Node::group(nodes).with_source(cx.source(&o.id, "archway")),
    );
}

/// Stairs, ladders and trapdoors: marked exits on a cell.
fn exit(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Item::Opening(o) = item else { return };
    let Some(cell) = o.cell else { return };
    let (x, y, w, h) = (f(cell.x0), f(cell.y0), f(cell.w()), f(cell.h()));
    let mut nodes = Vec::new();
    match o.ty.as_str() {
        "stairs" => {
            // Parallel treads narrowing in the direction of descent.
            for i in 0..5 {
                let t = i as f64 / 4.0;
                let inset = 0.08 + 0.3 * t;
                let ty = y + h * (0.9 - 0.8 * t);
                nodes.push(Node::stroke(
                    line((x + w * inset, ty), (x + w * (1.0 - inset), ty)),
                    ink(cx),
                ));
            }
        }
        "ladder" => {
            for rail in [0.32, 0.68] {
                nodes.push(Node::stroke(
                    line((x + w * rail, y + h * 0.1), (x + w * rail, y + h * 0.9)),
                    ink(cx),
                ));
            }
            for rung in [0.25, 0.5, 0.75] {
                nodes.push(Node::stroke(
                    line((x + w * 0.32, y + h * rung), (x + w * 0.68, y + h * rung)),
                    ink(cx),
                ));
            }
        }
        _ => {
            nodes.push(Node::path(
                rect(x + w * 0.15, y + h * 0.15, w * 0.7, h * 0.7),
                white_fill(cx),
                Some(ink(cx).dashed(0.12, 0.08)),
            ));
            nodes.push(Node::stroke(
                line((x + w * 0.15, y + h * 0.15), (x + w * 0.85, y + h * 0.85)),
                ink(cx),
            ));
        }
    }
    scene.push(
        "openings",
        Node::group(nodes).with_source(cx.source(&o.id, &o.ty)),
    );
}

fn note_marker(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Item::Note(note) = item else { return };
    let Some(anchor) = note.anchor else { return };
    let (x, y) = xy(anchor);
    let nodes = vec![
        Node::path(
            circle(x, y, 0.24),
            Some(Paint::solid(cx.theme.color("note"))),
            Some(ink(cx)),
        ),
        Node::text(
            x,
            y,
            &note.number.to_string(),
            0.3,
            TextAnchor::Middle,
            cx.theme.color("ink"),
        )
        .bold(),
    ];
    scene.push(
        "markers",
        Node::group(nodes).with_source(cx.source(&note.id, "note")),
    );
}

// ----- elements ------------------------------------------------------------------

/// The fallback for every element: a short text in a frame, taken from the
/// theme's symbol map.
fn marker(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Some(e) = element(item) else { return };
    let (x, y, w, h) = frame(e);
    let (cx0, cy0, m) = (x + w / 2.0, y + h / 2.0, w.min(h));
    let symbol = cx.theme.symbol(&e.ty, e.subtype.as_deref());
    let custom = e
        .properties
        .get("symbol")
        .and_then(Value::as_str)
        .filter(|_| e.ty == "custom");
    let glyph: String = custom
        .or(symbol.map(|s| s.glyph.as_str()))
        .unwrap_or("?")
        .chars()
        .take(3)
        .collect();
    let r = 0.38 * m;
    let shape = match symbol.and_then(|s| s.frame.as_deref()).unwrap_or("circle") {
        "diamond" => Some(polygon(&[
            (cx0, cy0 - r * 1.15),
            (cx0 + r * 1.15, cy0),
            (cx0, cy0 + r * 1.15),
            (cx0 - r * 1.15, cy0),
        ])),
        "square" => Some(rect(cx0 - r, cy0 - r, 2.0 * r, 2.0 * r)),
        "none" => None,
        _ => Some(circle(cx0, cy0, r)),
    };
    let size = cx.theme.size("marker") * m * if glyph.chars().count() > 1 { 0.7 } else { 1.0 };
    let mut nodes: Vec<Node> = shape
        .into_iter()
        .map(|d| Node::path(d, white_fill(cx), Some(ink(cx))))
        .collect();
    nodes.push(
        Node::text(
            cx0,
            cy0,
            &glyph,
            size,
            TextAnchor::Middle,
            cx.theme.color("ink"),
        )
        .bold(),
    );
    push(scene, cx, e, nodes);
}

fn column(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Some(e) = element(item) else { return };
    let (x, y, w, h) = frame(e);
    let m = w.min(h);
    let d = if e.subtype.as_deref() == Some("pillar") {
        rect(
            x + w / 2.0 - 0.3 * m,
            y + h / 2.0 - 0.3 * m,
            0.6 * m,
            0.6 * m,
        )
    } else {
        circle(x + w / 2.0, y + h / 2.0, 0.32 * m)
    };
    push(scene, cx, e, vec![Node::fill(d, cx.theme.color("wall"))]);
}

fn furniture(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Some(e) = element(item) else { return };
    let (x, y, w, h) = frame(e);
    let (cx0, cy0, m) = (x + w / 2.0, y + h / 2.0, w.min(h));
    let boxed = |inset: f64| {
        Node::path(
            rect(x + inset, y + inset, w - 2.0 * inset, h - 2.0 * inset),
            white_fill(cx),
            Some(ink(cx)),
        )
    };
    let stroke = |d: Vec<Cmd>| Node::stroke(d, ink(cx));
    let nodes = match e.subtype.as_deref().unwrap_or("") {
        "chair" => vec![
            boxed(0.28),
            stroke(line((x + 0.28, y + 0.28), (x + w - 0.28, y + 0.28))),
        ],
        "wardrobe" => vec![boxed(0.1), stroke(line((cx0, y + 0.1), (cx0, y + h - 0.1)))],
        "altar" => vec![
            boxed(0.12),
            stroke(line((cx0, cy0 - 0.2 * m), (cx0, cy0 + 0.2 * m))),
            stroke(line(
                (cx0 - 0.14 * m, cy0 - 0.06 * m),
                (cx0 + 0.14 * m, cy0 - 0.06 * m),
            )),
        ],
        "statue" => vec![
            Node::path(circle(cx0, cy0, 0.34 * m), white_fill(cx), Some(ink(cx))),
            Node::fill(circle(cx0, cy0, 0.14 * m), cx.theme.color("ink")),
        ],
        "sarcophagus" => vec![boxed(0.12), boxed(0.26)],
        "crate" => vec![
            boxed(0.16),
            stroke(line((x + 0.16, y + 0.16), (x + w - 0.16, y + h - 0.16))),
            stroke(line((x + w - 0.16, y + 0.16), (x + 0.16, y + h - 0.16))),
        ],
        _ => vec![boxed(0.14)],
    };
    push(scene, cx, e, nodes);
}

fn light(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Some(e) = element(item) else { return };
    let (x, y, w, h) = frame(e);
    let (cx0, cy0, m) = (x + w / 2.0, y + h / 2.0, w.min(h));
    // A radial gradient of `radius` cells, clipped to the room.
    let room = e
        .space
        .as_ref()
        .and_then(|id| cx.view.spaces.iter().find(|s| s.id == *id));
    if let (false, Some(radius), Some(room)) = (cx.legend, e.light_radius.filter(|r| *r > 0), room)
    {
        let r = f(radius);
        let color = cx.theme.color("light").to_string();
        let glow = Paint::Radial {
            cx: cx0,
            cy: cy0,
            r,
            stops: vec![(0.0, color.clone(), 0.6), (1.0, color, 0.0)],
        };
        let halo = Node::Group {
            transform: None,
            clip: Some(outline(room)),
            opacity: None,
            children: vec![Node::path(circle(cx0, cy0, r), Some(glow), None)],
            source: None,
        };
        scene.push("lighting", halo);
    }
    let flame = Some(Paint::solid(cx.theme.color("light")));
    let nodes = match e.subtype.as_deref().unwrap_or("") {
        "brazier" => vec![
            Node::path(circle(cx0, cy0, 0.28 * m), white_fill(cx), Some(ink(cx))),
            Node::path(circle(cx0, cy0, 0.14 * m), flame, Some(ink(cx))),
        ],
        "candle" => vec![Node::path(circle(cx0, cy0, 0.1 * m), flame, Some(ink(cx)))],
        "glowing_crystal" => {
            let r = 0.26 * m;
            vec![Node::path(
                polygon(&[
                    (cx0, cy0 - r),
                    (cx0 + 0.6 * r, cy0),
                    (cx0, cy0 + r),
                    (cx0 - 0.6 * r, cy0),
                ]),
                flame,
                Some(ink(cx)),
            )]
        }
        _ => {
            // A torch: a flame with four short rays.
            let mut nodes: Vec<Node> = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)]
                .iter()
                .map(|(dx, dy)| {
                    Node::stroke(
                        line(
                            (cx0 + dx * 0.2 * m, cy0 + dy * 0.2 * m),
                            (cx0 + dx * 0.3 * m, cy0 + dy * 0.3 * m),
                        ),
                        ink(cx),
                    )
                })
                .collect();
            nodes.push(Node::path(circle(cx0, cy0, 0.14 * m), flame, Some(ink(cx))));
            nodes
        }
    };
    push(scene, cx, e, nodes);
}

fn terrain(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Some(e) = element(item) else { return };
    let subtype = e.subtype.as_deref().unwrap_or("rubble");
    let area = polygon(&e.footprint.iter().map(|p| xy(*p)).collect::<Vec<_>>());
    let color = cx.theme.color(subtype).to_string();
    let mut nodes = Vec::new();
    if matches!(subtype, "water" | "lava") {
        nodes.push(Node::path(
            area,
            Some(Paint::Solid { color, alpha: 0.85 }),
            None,
        ));
    } else {
        // Loose stones: a fixed pattern of dots in every cell.
        let (x, y, w, h) = (f(e.bbox.x0), f(e.bbox.y0), f(e.bbox.w()), f(e.bbox.h()));
        const DOTS: [(f64, f64, f64); 5] = [
            (0.22, 0.28, 0.09),
            (0.62, 0.2, 0.06),
            (0.75, 0.62, 0.1),
            (0.36, 0.7, 0.07),
            (0.5, 0.45, 0.05),
        ];
        for row in 0..h.ceil() as i64 {
            for col in 0..w.ceil() as i64 {
                for (dx, dy, r) in DOTS {
                    nodes.push(Node::fill(
                        circle(x + col as f64 + dx, y + row as f64 + dy, r),
                        &color,
                    ));
                }
            }
        }
        nodes = vec![Node::Group {
            transform: None,
            clip: Some(area),
            opacity: None,
            children: nodes,
            source: None,
        }];
    }
    push(scene, cx, e, nodes);
}

fn chest(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Some(e) = element(item) else { return };
    let (x, y, w, h) = frame(e);
    let (cx0, cy0) = (x + w / 2.0, y + h / 2.0);
    let nodes = vec![
        Node::path(
            rect(x + 0.14, y + 0.24, w - 0.28, h - 0.48),
            white_fill(cx),
            Some(ink(cx)),
        ),
        Node::stroke(
            line((x + 0.14, cy0 - 0.06), (x + w - 0.14, cy0 - 0.06)),
            ink(cx),
        ),
        Node::fill(circle(cx0, cy0 + 0.04, 0.05), cx.theme.color("ink")),
    ];
    push(scene, cx, e, nodes);
}

/// A creature or NPC: a token with an initial, a count badge for a group,
/// and a tick on the rim for its facing.
fn token(item: &Item, cx: &Cx, scene: &mut Scene) {
    let Some(e) = element(item) else { return };
    let (x, y, w, h) = frame(e);
    let (cx0, cy0, m) = (x + w / 2.0, y + h / 2.0, w.min(h));
    let r = 0.38 * m;
    let named = e.name.as_deref().or(e
        .reference
        .as_deref()
        .map(|r| r.rsplit([':', '/']).next().unwrap_or(r)));
    let default = if e.ty == "npc" { 'N' } else { 'M' };
    let initial = named
        .and_then(|n| n.chars().find(|c| c.is_alphabetic()))
        .unwrap_or(default)
        .to_uppercase()
        .to_string();
    let fill = if e.ty == "npc" {
        Some(Paint::solid(cx.theme.color("npc")))
    } else {
        white_fill(cx)
    };
    let mut nodes = vec![
        Node::path(
            circle(cx0, cy0, r),
            fill,
            Some(Stroke::new(cx.theme.color("ink"), 0.06)),
        ),
        Node::text(
            cx0,
            cy0,
            &initial,
            cx.theme.size("marker") * m,
            TextAnchor::Middle,
            cx.theme.color("ink"),
        )
        .bold(),
    ];
    if let Some(facing) = e.facing {
        let (dx, dy) = match facing {
            Facing::North => (0.0, -1.0),
            Facing::NorthEast => (0.707, -0.707),
            Facing::East => (1.0, 0.0),
            Facing::SouthEast => (0.707, 0.707),
            Facing::South => (0.0, 1.0),
            Facing::SouthWest => (-0.707, 0.707),
            Facing::West => (-1.0, 0.0),
            Facing::NorthWest => (-0.707, -0.707),
        };
        let tip = (cx0 + dx * (r + 0.12), cy0 + dy * (r + 0.12));
        let base = |side: f64| {
            (
                cx0 + dx * r - dy * 0.09 * side,
                cy0 + dy * r + dx * 0.09 * side,
            )
        };
        nodes.push(Node::fill(
            polygon(&[tip, base(1.0), base(-1.0)]),
            cx.theme.color("ink"),
        ));
    }
    let count = e
        .properties
        .get("count")
        .and_then(Value::as_num)
        .and_then(|n| n.as_int())
        .unwrap_or(1);
    if count > 1 {
        let (bx, by) = (cx0 + 0.3 * m, cy0 - 0.3 * m);
        nodes.push(Node::fill(circle(bx, by, 0.17 * m), cx.theme.color("ink")));
        nodes.push(
            Node::text(
                bx,
                by,
                &count.to_string(),
                0.24 * m,
                TextAnchor::Middle,
                cx.theme.color("floor"),
            )
            .bold(),
        );
    }
    push(scene, cx, e, nodes);
}
