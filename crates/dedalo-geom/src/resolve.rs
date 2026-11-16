//! Stage 3 of the pipeline: document to resolved model (spec section 7).
//!
//! Layout is deterministic and rule-based: rooms are polygons on a cell
//! grid, walls are derived from space boundaries, openings are cut by
//! connections. Every problem is reported; whatever did resolve is passed on.

use std::collections::{BTreeMap, BTreeSet};

use dedalo_catalog::{Catalog, title_case};
use dedalo_model::ops::Op;
use dedalo_model::{
    Align, Anchor, Code, Connection, Diagnostic, Direction, Document, Dungeon, Element, Num, Space,
    SpaceKind, VERTICAL_TYPES, Value, Visibility,
};

use crate::model::*;
use crate::overlap::interiors_intersect;
use crate::prim::*;
use crate::shape::{self, Local};
use crate::walls::derive_walls;

/// One side of a space that an opening or a wall-mounted element can sit on.
#[derive(Clone, Debug)]
struct Side {
    /// The wall's name in the space's own frame, before rotation.
    name: Direction,
    /// The outward normal on the map.
    gdir: Direction,
    /// Start and end on the map. The start is the west or north end in the
    /// space's own frame, which is where offsets are measured from.
    start: Pt,
    end: Pt,
    /// Edge index as the author counted it.
    index: usize,
    /// For the virtual sides of a circle or an ellipse: the radius along
    /// the side and the radius across it.
    curve: Option<(C, C)>,
}

impl Side {
    fn len(&self) -> C {
        (self.end.x - self.start.x).abs() + (self.end.y - self.start.y).abs()
    }

    fn unit(&self) -> Pt {
        Pt::new(
            (self.end.x - self.start.x).signum(),
            (self.end.y - self.start.y).signum(),
        )
    }

    /// How far a chord of width `w` lies inside the tangent of a curve.
    fn sagitta(&self, w: C) -> C {
        self.curve.map_or(0, |(along, across)| {
            let (a, b, half) = (along as i128, across as i128, (w / 2) as i128);
            // On the ellipse x^2/a^2 + y^2/b^2 = 1 the chord at half-width
            // `half` lies at y = b * sqrt(1 - half^2 / a^2).
            across - (isqrt(b * b * (a * a - half * half)) / a) as C
        })
    }

    /// The radius along the side, for a curved side.
    fn along(&self) -> Option<C> {
        self.curve.map(|c| c.0)
    }
}

/// Maps a space's own frame onto the map: a quarter-turn rotation about the
/// centre of the bounding box, then a translation.
#[derive(Clone, Copy, Debug)]
struct Xf {
    rot: u8,
    w: C,
    h: C,
    origin: Pt,
}

impl Xf {
    fn apply(&self, p: Pt) -> Pt {
        let q = match self.rot % 4 {
            0 => p,
            1 => Pt::new(self.h - p.y, p.x),
            2 => Pt::new(self.w - p.x, self.h - p.y),
            _ => Pt::new(p.y, self.w - p.x),
        };
        q.plus(self.origin)
    }
}

/// The absolute geometry of a space.
#[derive(Clone, Debug)]
struct Geo {
    poly: Vec<Pt>,
    bbox: Rect,
    sides: Vec<Side>,
    circle: Option<Circle>,
    xf: Xf,
}

fn make_geo(local: &Local, rot: u8, origin: Pt) -> Geo {
    let xf = Xf {
        rot,
        w: local.w,
        h: local.h,
        origin,
    };
    let poly: Vec<Pt> = local.poly.iter().map(|p| xf.apply(*p)).collect();
    let mut sides = Vec::new();
    let mut side = |name: Direction, a: Pt, b: Pt, index: usize, curve: Option<(C, C)>| {
        sides.push(Side {
            name,
            gdir: rotate_dir(name, rot),
            start: xf.apply(a),
            end: xf.apply(b),
            index,
            curve,
        });
    };
    if let Some((rx, ry)) = local.radius {
        // A circle or an ellipse has four virtual sides, each tangent at a
        // compass point.
        let (w, h) = (2 * rx, 2 * ry);
        side(
            Direction::North,
            Pt::new(0, 0),
            Pt::new(w, 0),
            0,
            Some((rx, ry)),
        );
        side(
            Direction::East,
            Pt::new(w, 0),
            Pt::new(w, h),
            1,
            Some((ry, rx)),
        );
        side(
            Direction::South,
            Pt::new(0, h),
            Pt::new(w, h),
            2,
            Some((rx, ry)),
        );
        side(
            Direction::West,
            Pt::new(0, 0),
            Pt::new(0, h),
            3,
            Some((ry, rx)),
        );
    } else {
        for (i, (a, b)) in edges(&local.poly).enumerate() {
            let index = local.edge_index[i];
            // The outline is clockwise, so the outside is on the left of travel.
            if a.y == b.y && a.x < b.x {
                side(Direction::North, a, b, index, None);
            } else if a.y == b.y {
                side(Direction::South, b, a, index, None);
            } else if a.x == b.x && a.y < b.y {
                side(Direction::East, a, b, index, None);
            } else if a.x == b.x {
                side(Direction::West, b, a, index, None);
            }
        }
    }
    let circle = local.radius.map(|(rx, ry)| {
        let c = Circle {
            center: xf.apply(Pt::new(rx, ry)),
            radius: rx,
            radius_y: (rx != ry).then_some(ry),
        };
        c.turned(rot as i64)
    });
    Geo {
        bbox: Rect::of(&poly),
        poly,
        sides,
        circle,
        xf,
    }
}

/// The geometry of a space given directly as a map polygon (corridors).
fn geo_of_polygon(poly: Vec<Pt>) -> Geo {
    let bbox = Rect::of(&poly);
    let origin = Pt::new(bbox.x0, bbox.y0);
    let local = Local {
        poly: poly.iter().map(|p| p.minus(origin)).collect(),
        w: bbox.w(),
        h: bbox.h(),
        radius: None,
        edge_index: (0..poly.len()).collect(),
    };
    make_geo(&local, 0, origin)
}

struct Sp<'a> {
    src: &'a Space,
    local: Option<Local>,
    rot: u8,
    geo: Option<Geo>,
    /// Corridors: the opening at each end, as (space ID, segment).
    ends: Vec<(String, [Pt; 2])>,
}

fn find(sps: &[Sp], id: &str) -> Option<usize> {
    sps.iter().position(|s| s.src.id == id)
}

fn num(n: Option<Num>, default: C) -> C {
    n.map_or(default, |n| n.0)
}

fn quote_list(ids: &[&str]) -> String {
    ids.iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolves a document. Returns whatever resolved, and the diagnostics.
pub fn resolve(doc: &Document, cat: &Catalog) -> (Resolved, Vec<Diagnostic>) {
    resolve_with(doc, cat, &|_| None)
}

/// A shape function provided by a plugin: the call, to outline points in
/// cells. `None` when no loaded plugin provides the function.
pub type ShapeHook<'a> =
    &'a dyn Fn(&dedalo_model::ShapeCall) -> Option<Result<Vec<[f64; 2]>, String>>;

/// Resolves a document with plugin shape functions available.
pub fn resolve_with(
    doc: &Document,
    cat: &Catalog,
    shapes: ShapeHook,
) -> (Resolved, Vec<Diagnostic>) {
    let d = &doc.dungeon;
    let mut diags = Vec::new();
    if !dedalo_model::check::geometry_limits(d).is_empty() {
        // Numbers outside the supported range are reported by the check
        // pass; geometry does not run on them.
        let resolved = Resolved {
            id: d.id.clone(),
            name: d.name.clone(),
            grid_unit: None,
            seed: fnv1a(&d.id),
            bounds: Rect::default(),
            spaces: Vec::new(),
            walls: Vec::new(),
            openings: Vec::new(),
            elements: Vec::new(),
            notes: Vec::new(),
            reach: Reach::default(),
        };
        return (resolved, diags);
    }
    let max_stub = num(d.settings.as_ref().and_then(|s| s.max_stub), 2 * CELL);

    let mut sps: Vec<Sp> = d
        .spaces
        .iter()
        .map(|s| Sp {
            src: s,
            local: None,
            rot: 0,
            geo: None,
            ends: Vec::new(),
        })
        .collect();
    for sp in sps.iter_mut().filter(|s| s.src.kind == SpaceKind::Room) {
        let id = &sp.src.id;
        match &sp.src.shape {
            None => diags.push(
                Diagnostic::new(
                    Code::InvalidShape,
                    format!(
                        "Room `{id}` has no shape. Add one, for example `shape = rectangle(8, 6)`."
                    ),
                )
                .element(id.clone()),
            ),
            Some(call) => match build_shape(call, shapes) {
                Ok(local) => sp.local = Some(local),
                Err(reason) => diags.push(
                    Diagnostic::new(
                        Code::InvalidShape,
                        format!("The shape of room `{id}` is invalid: {reason}"),
                    )
                    .element(id.clone())
                    .at("shape"),
                ),
            },
        }
        sp.rot = (sp.src.rotation.unwrap_or(0).rem_euclid(360) / 90) as u8;
    }

    place_rooms(d, &mut sps, &mut diags);
    resolve_corridors(&mut sps, &mut diags);
    check_overlaps(d, &sps, &mut diags);
    let mut openings = resolve_connections(d, &sps, max_stub, &mut diags);
    openings.extend(corridor_ends(d, &sps));

    let default_wall = d
        .settings
        .as_ref()
        .and_then(|s| s.default_wall.clone())
        .unwrap_or_else(|| "stone".to_string());
    let default_floor = d
        .settings
        .as_ref()
        .and_then(|s| s.default_floor.clone())
        .unwrap_or_else(|| "stone".to_string());
    let material = |s: &Space, key: &str, default: &str| {
        s.properties
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_string()
    };
    let spaces: Vec<RSpace> = sps
        .iter()
        .filter_map(|sp| {
            let geo = sp.geo.as_ref()?;
            Some(RSpace {
                id: sp.src.id.clone(),
                kind: sp.src.kind,
                name: sp.src.name.clone(),
                polygon: geo.poly.clone(),
                circle: geo.circle,
                bbox: geo.bbox,
                level: "default".to_string(),
                visibility: sp.src.visibility.unwrap_or(Visibility::Public),
                reveal_when: sp.src.reveal_when.clone(),
                tags: sp.src.tags.clone(),
                wall: material(sp.src, "wall", &default_wall),
                floor: material(sp.src, "floor", &default_floor),
                connects: sp.src.connects.clone(),
            })
        })
        .collect();

    let elements = resolve_elements(d, cat, &sps, &mut diags);
    let notes = resolve_notes(d, &sps, &elements, &openings);
    let reach = reachability(d, &spaces, &openings, &mut diags);
    let walls = derive_walls(&spaces, &openings);

    let mut bounds: Option<Rect> = None;
    let stubs = openings
        .iter()
        .filter_map(|o| o.stub.as_deref())
        .map(Rect::of);
    for r in spaces.iter().map(|s| s.bbox).chain(stubs) {
        bounds = Some(bounds.map_or(r, |b| b.union(&r)));
    }
    let seed = d
        .settings
        .as_ref()
        .and_then(|s| s.seed)
        .map_or_else(|| fnv1a(&d.id), |s| s as u64);
    let resolved = Resolved {
        id: d.id.clone(),
        name: d.name.clone(),
        grid_unit: d.grid.as_ref().and_then(|g| g.unit.clone()),
        seed,
        bounds: bounds.unwrap_or_default(),
        spaces,
        walls,
        openings,
        elements,
        notes,
        reach,
    };
    (resolved, diags)
}

/// Builds a shape: a built-in function, or a dotted plugin function.
fn build_shape(call: &dedalo_model::ShapeCall, shapes: ShapeHook) -> Result<Local, String> {
    if !call.func.contains('.') {
        return shape::build(call);
    }
    match shapes(call) {
        None => Err(format!(
            "the shape `{}` comes from a plugin that is not loaded.",
            call.func
        )),
        Some(Err(reason)) => Err(reason),
        Some(Ok(points)) => {
            if points.iter().any(|p| {
                !p[0].is_finite() || !p[1].is_finite() || p[0].abs() > 1e6 || p[1].abs() > 1e6
            }) {
                return Err(format!(
                    "the plugin shape `{}` returned a point out of range.",
                    call.func
                ));
            }
            let points = points
                .iter()
                .map(|p| Pt::new((p[0] * 1000.0).round() as C, (p[1] * 1000.0).round() as C))
                .collect();
            shape::from_points(points)
        }
    }
}

/// A stable 64-bit hash, the default seed of a dungeon (spec 7.10).
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

// ----- room placement (spec 7.3) ---------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Plan {
    Fixed(Pt),
    Relative(usize, Direction),
    Automatic,
    Skip,
}

/// Where a room of `w` by `h` goes when it lies `dir` of the box `t`.
fn relative_origin(t: &Rect, w: C, h: C, dir: Direction, align: Align, offset: C, gap: C) -> Pt {
    let cross = |start: C, target_len: C, len: C| {
        offset
            + match align {
                Align::Start => start,
                // Centred, snapped to a whole cell toward the start.
                Align::Center => start + floor_cell((target_len - len).div_euclid(2)),
                Align::End => start + target_len - len,
            }
    };
    match dir {
        Direction::North => Pt::new(cross(t.x0, t.w(), w), t.y0 - gap - h),
        Direction::South => Pt::new(cross(t.x0, t.w(), w), t.y1 + gap),
        Direction::East => Pt::new(t.x1 + gap, cross(t.y0, t.h(), h)),
        Direction::West => Pt::new(t.x0 - gap - w, cross(t.y0, t.h(), h)),
    }
}

fn rotated_size(local: &Local, rot: u8) -> (C, C) {
    if rot % 2 == 1 {
        (local.h, local.w)
    } else {
        (local.w, local.h)
    }
}

/// The gap a corridor asks for between the two rooms it joins (rule 4).
fn corridor_gap(d: &Dungeon, a: &str, b: &str) -> Option<C> {
    d.spaces
        .iter()
        .find(|s| {
            s.kind == SpaceKind::Corridor
                && s.connects.iter().any(|c| c == a)
                && s.connects.iter().any(|c| c == b)
        })
        .map(|s| num(s.length, 2 * CELL))
}

fn place_rooms(d: &Dungeon, sps: &mut [Sp], diags: &mut Vec<Diagnostic>) {
    let mut plans: Vec<Plan> = sps
        .iter()
        .map(|sp| {
            if sp.local.is_none() {
                return Plan::Skip;
            }
            let Some(layout) = &sp.src.layout else {
                return Plan::Automatic;
            };
            if layout.mode == Some(dedalo_model::LayoutMode::Automatic) {
                return Plan::Automatic;
            }
            if let Some(p) = layout.position {
                return Plan::Fixed(Pt::new(p[0].0, p[1].0));
            }
            match (&layout.adjacent_to, layout.direction) {
                (Some(target), Some(dir)) => match find(sps, target)
                    .filter(|t| sps[*t].src.kind == SpaceKind::Room && *target != sp.src.id)
                {
                    Some(t) => Plan::Relative(t, dir),
                    None => Plan::Skip, // an unknown reference, already reported
                },
                (None, None) => Plan::Automatic,
                _ => Plan::Skip, // half a relation, already reported
            }
        })
        .collect();
    // Rule 5: with no fixed room, the first unconstrained room is the origin.
    if !plans.iter().any(|p| matches!(p, Plan::Fixed(_)))
        && let Some(first) = plans.iter().position(|p| *p == Plan::Automatic)
    {
        plans[first] = Plan::Fixed(Pt::new(0, 0));
    }

    loop {
        let mut progress = false;
        for i in 0..sps.len() {
            if sps[i].geo.is_some() {
                continue;
            }
            let Some(local) = sps[i].local.clone() else {
                continue;
            };
            let (w, h) = rotated_size(&local, sps[i].rot);
            let origin = match plans[i] {
                Plan::Fixed(p) => Some(p),
                Plan::Relative(t, dir) => sps[t].geo.as_ref().map(|target| {
                    let layout = sps[i].src.layout.as_ref();
                    let gap = layout
                        .and_then(|l| l.gap)
                        .map(|g| g.0)
                        .or_else(|| corridor_gap(d, &sps[i].src.id, &sps[t].src.id))
                        .unwrap_or(0);
                    let align = layout.and_then(|l| l.align).unwrap_or(Align::Center);
                    relative_origin(
                        &target.bbox,
                        w,
                        h,
                        dir,
                        align,
                        num(layout.and_then(|l| l.offset), 0),
                        gap,
                    )
                }),
                Plan::Automatic => automatic_origin(d, sps, i, &local),
                Plan::Skip => None,
            };
            if let Some(origin) = origin {
                let limit = dedalo_model::check::MAX_COORDINATE;
                if origin.x.abs().max(origin.y.abs()) + w.max(h) > limit * CELL {
                    // A long chain of relative placements can walk off the map.
                    let id = sps[i].src.id.clone();
                    let message = format!(
                        "Room `{id}` would be placed at [{}, {}], outside the supported area of -{limit} to {limit} cells.",
                        cells(origin.x),
                        cells(origin.y)
                    );
                    diags.push(
                        Diagnostic::new(Code::InvalidValue, message)
                            .element(id)
                            .at("layout"),
                    );
                    plans[i] = Plan::Skip;
                    continue;
                }
                sps[i].geo = Some(make_geo(&local, sps[i].rot, origin));
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }

    for i in 0..sps.len() {
        if sps[i].geo.is_some() {
            continue;
        }
        let id = sps[i].src.id.clone();
        match plans[i] {
            Plan::Relative(mut t, _) => {
                // Follow the chain of targets; coming back here is a cycle.
                let mut chain = vec![id.as_str()];
                let mut cyclic = false;
                for _ in 0..sps.len() {
                    if t == i {
                        cyclic = true;
                        break;
                    }
                    chain.push(&sps[t].src.id);
                    match plans[t] {
                        Plan::Relative(next, _) if sps[t].geo.is_none() => t = next,
                        _ => break,
                    }
                }
                if cyclic {
                    diags.push(
                        Diagnostic::new(
                            Code::LayoutCycle,
                            format!("Room `{id}` is placed in a loop of relative positions: {} and back to `{id}`. No room of the loop has a fixed position.", quote_list(&chain)),
                        )
                        .element(id.clone())
                        .at("layout.adjacent_to")
                        .suggest(format!("Give one room of the loop an absolute position, for example `layout {{ position = [0, 0] }}` on `{id}`.")),
                    );
                }
            }
            Plan::Automatic => diags.push(
                Diagnostic::new(
                    Code::LayoutUnderconstrained,
                    format!("Room `{id}` has no position, and no connection or corridor links it to a room that is already placed."),
                )
                .element(id.clone())
                .at("layout")
                .suggest("Add `layout { position = [x, y] }` or `layout { adjacent_to = \"other_room\", direction = east }`, or connect the room to a placed room."),
            ),
            _ => {}
        }
    }
}

/// Rule 6: a room with no layout goes next to a placed space it is linked to.
fn automatic_origin(d: &Dungeon, sps: &[Sp], i: usize, local: &Local) -> Option<Pt> {
    let id = &sps[i].src.id;
    let placed = |other: &str| find(sps, other).filter(|j| sps[*j].geo.is_some());
    let mut link: Option<(usize, Option<Direction>, C)> = None;
    for c in &d.connections {
        let wall = c.placement.as_ref().and_then(|p| p.wall);
        if c.to.as_deref() == Some(id.as_str())
            && let Some(j) = placed(&c.from)
        {
            link = Some((j, wall, 0));
        } else if c.from == *id
            && let Some(j) = c.to.as_deref().and_then(placed)
        {
            link = Some((j, wall.map(opposite), 0));
        }
        if link.is_some() {
            break;
        }
    }
    if link.is_none() {
        for s in d.spaces.iter().filter(|s| {
            s.kind == SpaceKind::Corridor && s.connects.len() == 2 && s.connects.contains(id)
        }) {
            let other = if s.connects[0] == *id {
                &s.connects[1]
            } else {
                &s.connects[0]
            };
            if let Some(j) = placed(other) {
                link = Some((j, None, num(s.length, 2 * CELL)));
                break;
            }
        }
    }
    let (j, wall, gap) = link?;
    let target = sps[j].geo.as_ref()?;
    let (w, h) = rotated_size(local, sps[i].rot);
    let origin = |dir| relative_origin(&target.bbox, w, h, dir, Align::Center, 0, gap);
    if let Some(dir) = wall {
        return Some(origin(dir));
    }
    // Otherwise the first of east, south, west, north that causes no overlap.
    let sides = [
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::North,
    ];
    let free = sides.into_iter().find(|dir| {
        let geo = make_geo(local, sps[i].rot, origin(*dir));
        !sps.iter()
            .filter_map(|s| s.geo.as_ref())
            .any(|other| interiors_intersect(&geo.poly, &other.poly))
    });
    Some(origin(free.unwrap_or(Direction::East)))
}

fn check_overlaps(d: &Dungeon, sps: &[Sp], diags: &mut Vec<Diagnostic>) {
    for j in 0..sps.len() {
        let Some(gj) = &sps[j].geo else { continue };
        for i in 0..j {
            let Some(gi) = &sps[i].geo else { continue };
            // A corridor reaches slightly into a circular room by design.
            let joined = |c: &Sp, room: &Sp, g: &Geo| {
                c.src.kind == SpaceKind::Corridor
                    && g.circle.is_some()
                    && c.src.connects.contains(&room.src.id)
            };
            if joined(&sps[i], &sps[j], gj)
                || joined(&sps[j], &sps[i], gi)
                || !interiors_intersect(&gi.poly, &gj.poly)
            {
                continue;
            }
            let (a, b) = (&sps[i].src.id, &sps[j].src.id);
            let area = gi.bbox.intersection(&gj.bbox);
            let mut diag = Diagnostic::new(
                Code::Overlap,
                format!(
                    "`{b}` overlaps `{a}`: their interiors intersect within x {} to {}, y {} to {}. Spaces may touch along an edge but may not overlap.",
                    cells(area.x0),
                    cells(area.x1),
                    cells(area.y0),
                    cells(area.y1)
                ),
            )
            .element(b.clone())
            .at("layout")
            .related(a.clone(), format!("occupies x {} to {}, y {} to {}", cells(gi.bbox.x0), cells(gi.bbox.x1), cells(gi.bbox.y0), cells(gi.bbox.y1)));
            diag = match clearing_offset(d, sps, j) {
                Some(offset) => diag
                    .suggest(format!("Set `layout.offset = {}` on `{b}`, the smallest shift that clears the overlap; `align = start` or `align = end` may do the same.", cells(offset)))
                    .fix(Op::set(b, "layout.offset", Value::Num(Num(offset)))),
                None => diag.suggest(format!("Move `{b}` or `{a}` so that they no longer intersect.")),
            };
            diags.push(diag);
        }
    }
}

/// The smallest `layout.offset` that frees a relatively placed room from
/// every other space.
fn clearing_offset(d: &Dungeon, sps: &[Sp], j: usize) -> Option<C> {
    let sp = &sps[j];
    let (layout, local) = (sp.src.layout.as_ref()?, sp.local.as_ref()?);
    let (target, dir) = (find(sps, layout.adjacent_to.as_ref()?)?, layout.direction?);
    let target_box = sps[target].geo.as_ref()?.bbox;
    let (w, h) = rotated_size(local, sp.rot);
    let gap = layout
        .gap
        .map(|g| g.0)
        .or_else(|| corridor_gap(d, &sp.src.id, &sps[target].src.id))
        .unwrap_or(0);
    let base = num(layout.offset, 0);
    let reach = ((target_box.w().max(target_box.h()) + w.max(h)) / CELL + 1).min(64);
    for step in 1..=reach {
        for offset in [base - step * CELL, base + step * CELL] {
            let origin = relative_origin(
                &target_box,
                w,
                h,
                dir,
                layout.align.unwrap_or(Align::Center),
                offset,
                gap,
            );
            let poly = make_geo(local, sp.rot, origin).poly;
            let clear = sps
                .iter()
                .enumerate()
                .filter(|(k, _)| *k != j)
                .filter_map(|(_, s)| s.geo.as_ref())
                .all(|g| !interiors_intersect(&poly, &g.poly));
            if clear {
                return Some(offset);
            }
        }
    }
    None
}

// ----- facing walls ------------------------------------------------------------

/// A pair of facing sides an opening or corridor can use.
struct Cand {
    ia: usize,
    ib: usize,
    /// Distance between the two sides.
    d: C,
    /// The range the opening may occupy, measured from the start of side `ia`.
    lo: C,
    hi: C,
    /// True when a circle pins the opening to one position.
    fixed: bool,
}

/// Facing side pairs of `a` and `b` wide enough for `w`, nearest first.
/// Also returns the widest overlap that was too narrow.
fn candidates(a: &Geo, b: &Geo, w: C, pick: &dyn Fn(&Side) -> bool) -> (Vec<Cand>, Option<C>) {
    let mut out = Vec::new();
    let mut narrow: Option<C> = None;
    for (ia, sa) in a.sides.iter().enumerate().filter(|(_, s)| pick(s)) {
        let (n, u) = (dvec(sa.gdir), sa.unit());
        for (ib, sb) in b
            .sides
            .iter()
            .enumerate()
            .filter(|(_, s)| s.gdir == opposite(sa.gdir))
        {
            let d = sb.start.minus(sa.start).dot(n);
            if d < 0
                || sa.along().is_some_and(|r| w > 2 * r)
                || sb.along().is_some_and(|r| w > 2 * r)
            {
                continue;
            }
            let (mut lo, mut hi, mut fixed) = (0, sa.len(), false);
            if let Some(r) = sa.along() {
                (lo, hi, fixed) = (r - w / 2, r - w / 2 + w, true);
            }
            let (t0, t1) = (
                sb.start.minus(sa.start).dot(u),
                sb.end.minus(sa.start).dot(u),
            );
            let (mut blo, mut bhi) = (t0.min(t1), t0.max(t1));
            if sb.curve.is_some() {
                let centre = (blo + bhi) / 2;
                (blo, bhi, fixed) = (centre - w / 2, centre - w / 2 + w, true);
            }
            (lo, hi) = (lo.max(blo), hi.min(bhi));
            if hi - lo < w {
                if hi > lo {
                    narrow = Some(narrow.map_or(hi - lo, |x| x.max(hi - lo)));
                }
                continue;
            }
            out.push(Cand {
                ia,
                ib,
                d,
                lo,
                hi,
                fixed,
            });
        }
    }
    out.sort_by_key(|c| c.d);
    (out, narrow)
}

/// Centred on the usable range, snapped to a whole cell toward the start.
fn default_offset(c: &Cand, w: C) -> C {
    if c.fixed {
        c.lo
    } else {
        floor_cell(c.lo + (c.hi - c.lo - w) / 2).max(c.lo)
    }
}

struct Passage {
    /// The opening on side `a`.
    near: [Pt; 2],
    /// The opening on side `b`.
    far: [Pt; 2],
    /// The rectangle between them, when they are apart.
    poly: Option<Vec<Pt>>,
}

fn passage(a: &Geo, b: &Geo, c: &Cand, offset: C, w: C) -> Passage {
    let (sa, sb) = (&a.sides[c.ia], &b.sides[c.ib]);
    let (u, n) = (sa.unit(), dvec(sa.gdir));
    let (sag_a, sag_b) = (sa.sagitta(w), sb.sagitta(w));
    let p0 = sa.start.along(u, offset).along(n, -sag_a);
    let p1 = p0.along(u, w);
    let depth = c.d + sag_a + sag_b;
    let (q0, q1) = (p0.along(n, depth), p1.along(n, depth));
    Passage {
        near: [p0, p1],
        far: [q0, q1],
        poly: (depth > 0).then(|| clockwise(vec![p0, p1, q1, q0])),
    }
}

/// `x 1 to 7` or `y 2 to 5`: a range along a side, in map coordinates.
fn span_text(side: &Side, lo: C, hi: C) -> String {
    let (a, b) = (
        side.start.along(side.unit(), lo),
        side.start.along(side.unit(), hi),
    );
    if a.y == b.y {
        format!("x {} to {}", cells(a.x.min(b.x)), cells(a.x.max(b.x)))
    } else {
        format!("y {} to {}", cells(a.y.min(b.y)), cells(a.y.max(b.y)))
    }
}

// ----- corridors (spec 7.6) ------------------------------------------------------

fn resolve_corridors(sps: &mut [Sp], diags: &mut Vec<Diagnostic>) {
    let mut done: BTreeSet<usize> = BTreeSet::new();
    // The outline of a corridor with a path follows from the path alone, so
    // corridors with paths can check their ends against each other at once.
    let provisional: BTreeMap<usize, Geo> = (0..sps.len())
        .filter(|i| sps[*i].src.kind == SpaceKind::Corridor && !sps[*i].src.path.is_empty())
        .filter_map(|i| {
            path_shape(sps[i].src)
                .ok()
                .map(|(poly, _, _)| (i, geo_of_polygon(poly)))
        })
        .collect();
    loop {
        let mut progress = false;
        for i in 0..sps.len() {
            if sps[i].src.kind != SpaceKind::Corridor
                || done.contains(&i)
                || sps[i].src.connects.len() != 2
            {
                continue;
            }
            let ends: Vec<Option<usize>> = sps[i]
                .src
                .connects
                .iter()
                .map(|id| find(sps, id).filter(|j| *j != i))
                .collect();
            let (Some(a), Some(b)) = (ends[0], ends[1]) else {
                done.insert(i);
                continue;
            };
            // A corridor joined to another corridor waits for it, unless both
            // have paths and the outline is already known.
            let has_path = !sps[i].src.path.is_empty();
            let waiting = |j: usize| {
                sps[j].src.kind == SpaceKind::Corridor
                    && !done.contains(&j)
                    && !(has_path && provisional.contains_key(&j))
            };
            if waiting(a) || waiting(b) {
                continue;
            }
            done.insert(i);
            progress = true;
            let geo_of = |j: usize| sps[j].geo.clone().or_else(|| provisional.get(&j).cloned());
            let (Some(ga), Some(gb)) = (geo_of(a), geo_of(b)) else {
                continue;
            };
            let result = if sps[i].src.path.is_empty() {
                straight_corridor(sps[i].src, (&sps[a].src.id, &ga), (&sps[b].src.id, &gb))
            } else {
                path_corridor(sps[i].src, (&sps[a].src.id, &ga), (&sps[b].src.id, &gb))
            };
            match result {
                Ok((poly, ends)) => {
                    sps[i].geo = Some(geo_of_polygon(poly));
                    sps[i].ends = ends;
                }
                Err(diag) => diags.push(diag),
            }
        }
        if !progress {
            break;
        }
    }
    // Corridors that wait on each other without paths cannot be routed.
    for (i, sp) in sps.iter().enumerate() {
        let c = sp.src;
        if c.kind == SpaceKind::Corridor && c.connects.len() == 2 && !done.contains(&i) {
            diags.push(
                Diagnostic::new(
                    Code::CorridorDisconnected,
                    format!(
                        "Corridor `{}` joins another corridor that is itself waiting to be routed, so neither can be placed. Corridors that join corridors need an explicit `path`.",
                        c.id
                    ),
                )
                .element(c.id.clone())
                .at("connects")
                .suggest("Give the corridor a `path` of lattice points that starts or ends where the other corridor ends."),
            );
        }
    }
}

type Corridor = (Vec<Pt>, Vec<(String, [Pt; 2])>);

/// Automatic routing: straight across the gap between facing walls.
fn straight_corridor(c: &Space, a: (&str, &Geo), b: (&str, &Geo)) -> Result<Corridor, Diagnostic> {
    let id = &c.id;
    let w = num(c.width, CELL);
    let (cands, narrow) = candidates(a.1, b.1, w, &|_| true);
    let Some(best) = cands.iter().find(|k| k.d > 0) else {
        if !cands.is_empty() {
            return Err(Diagnostic::new(
                Code::InvalidShape,
                format!("Corridor `{id}` has zero length: `{}` and `{}` already share a wall.", a.0, b.0),
            )
            .element(id.clone())
            .at("connects")
            .suggest(format!("Remove the corridor and join the rooms with a connection: `connection \"{id}_door\" {{ from = \"{}\", to = \"{}\", type = door }}`.", a.0, b.0)));
        }
        let reason = match narrow {
            Some(n) => format!(
                "their facing walls overlap by only {} cells, less than the corridor's width of {}",
                cells(n),
                cells(w)
            ),
            None => "no wall of one faces a wall of the other".to_string(),
        };
        return Err(Diagnostic::new(
            Code::CorridorDisconnected,
            format!("Corridor `{id}` cannot run straight from `{}` to `{}`: {reason}. Automatic routing is straight only.", a.0, b.0),
        )
        .element(id.clone())
        .at("connects")
        .suggest("Move the rooms so that their walls face each other, or give the corridor an explicit `path` of lattice points."));
    };
    let offset = match c.offset {
        None => default_offset(best, w),
        Some(o) if o.0 >= best.lo && o.0 + w <= best.hi && (!best.fixed || o.0 == best.lo) => o.0,
        Some(o) => {
            let side = &a.1.sides[best.ia];
            return Err(Diagnostic::new(
                Code::CorridorDisconnected,
                format!(
                    "Corridor `{id}` at offset {o} on the {} wall of `{}` misses `{}`. Valid offsets are {} to {}.",
                    side.name.as_str(),
                    a.0,
                    b.0,
                    cells(ceil_cell(best.lo)),
                    cells(floor_cell(best.hi - w))
                ),
            )
            .element(id.clone())
            .at("offset")
            .suggest("Set `offset` inside that range, or remove it to centre the corridor.")
            .fix(Op::set(id, "offset", Value::Num(Num(default_offset(best, w))))));
        }
    };
    let p = passage(a.1, b.1, best, offset, w);
    let poly = p.poly.unwrap_or_default();
    Ok((
        poly,
        vec![(a.0.to_string(), p.near), (b.0.to_string(), p.far)],
    ))
}

/// Explicit routing: the centre line widened by half the width on each side.
/// A corridor outline and its two end segments.
type PathShape = (Vec<Pt>, [Pt; 2], [Pt; 2]);

/// The outline of a corridor with a path, and its two end segments.
fn path_shape(c: &Space) -> Result<PathShape, Diagnostic> {
    let id = &c.id;
    let w = num(c.width, CELL);
    let half = w / 2;
    let invalid = |reason: String| {
        Diagnostic::new(
            Code::InvalidShape,
            format!("The path of corridor `{id}` is invalid: {reason}"),
        )
        .element(id.clone())
        .at("path")
    };
    let pts: Vec<Pt> = c.path.iter().map(|p| Pt::new(p[0].0, p[1].0)).collect();
    if pts.len() < 2 {
        return Err(invalid("a path needs at least two points.".to_string()));
    }
    let mut dirs: Vec<Pt> = Vec::new();
    for (k, pair) in pts.windows(2).enumerate() {
        let dir = Pt::new(
            (pair[1].x - pair[0].x).signum(),
            (pair[1].y - pair[0].y).signum(),
        );
        if dir.x != 0 && dir.y != 0 || dir == Pt::new(0, 0) {
            return Err(invalid(format!(
                "the segment from [{}, {}] to [{}, {}] is not horizontal or vertical. Segments must be orthogonal before release 1.0.",
                cells(pair[0].x),
                cells(pair[0].y),
                cells(pair[1].x),
                cells(pair[1].y)
            )));
        }
        if dirs
            .last()
            .is_some_and(|prev| prev.x == -dir.x && prev.y == -dir.y)
        {
            return Err(invalid(format!("it doubles back on itself at point {k}.")));
        }
        dirs.push(dir);
    }
    // The left-hand normal of each segment; the outline is both offset lines.
    let normal = |dir: Pt| Pt::new(dir.y, -dir.x);
    let corner = |k: usize| {
        let before = if k == 0 { dirs[0] } else { dirs[k - 1] };
        let after = if k == dirs.len() {
            dirs[k - 1]
        } else {
            dirs[k]
        };
        if before == after {
            normal(before)
        } else {
            normal(before).plus(normal(after))
        }
    };
    let left: Vec<Pt> = (0..pts.len())
        .map(|k| pts[k].along(corner(k), half))
        .collect();
    let right: Vec<Pt> = (0..pts.len())
        .map(|k| pts[k].along(corner(k), -half))
        .collect();
    let (start, end) = (
        [left[0], right[0]],
        [left[pts.len() - 1], right[pts.len() - 1]],
    );
    let mut poly = left;
    poly.extend(right.into_iter().rev());
    // Drop vertices in the middle of a straight run.
    let n = poly.len();
    let poly: Vec<Pt> = (0..n)
        .filter(|k| orient(poly[(k + n - 1) % n], poly[*k], poly[(k + 1) % n]) != 0)
        .map(|k| poly[k])
        .collect();
    let all: Vec<(Pt, Pt)> = edges(&poly).collect();
    for i in 0..all.len() {
        for j in i + 2..all.len() {
            if !(i == 0 && j == all.len() - 1)
                && segments_touch(all[i].0, all[i].1, all[j].0, all[j].1)
            {
                return Err(invalid("the corridor crosses itself.".to_string()));
            }
        }
    }
    Ok((clockwise(poly), start, end))
}

/// Explicit routing: the centre line widened by half the width on each side.
fn path_corridor(c: &Space, a: (&str, &Geo), b: (&str, &Geo)) -> Result<Corridor, Diagnostic> {
    let id = &c.id;
    let w = num(c.width, CELL);
    let (poly, start, end) = path_shape(c)?;
    let pts: Vec<Pt> = c.path.iter().map(|p| Pt::new(p[0].0, p[1].0)).collect();
    let touches = |geo: &Geo, seg: [Pt; 2]| {
        geo.sides.iter().any(|s| {
            s.curve.is_none()
                && on_segment(s.start, s.end, seg[0])
                && on_segment(s.start, s.end, seg[1])
        })
    };
    let ends = if touches(a.1, start) && touches(b.1, end) {
        vec![(a.0.to_string(), start), (b.0.to_string(), end)]
    } else if touches(b.1, start) && touches(a.1, end) {
        vec![(b.0.to_string(), start), (a.0.to_string(), end)]
    } else {
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        let loose = if !touches(a.1, start) && !touches(b.1, start) {
            ("starts", first)
        } else {
            ("ends", last)
        };
        return Err(Diagnostic::new(
            Code::CorridorDisconnected,
            format!(
                "Corridor `{id}` {} at [{}, {}], where its {}-cell-wide end does not lie on a wall of `{}` or `{}`. Both ends of a path must touch the spaces named in `connects`.",
                loose.0,
                cells(loose.1.x),
                cells(loose.1.y),
                cells(w),
                a.0,
                b.0
            ),
        )
        .element(id.clone())
        .at("path")
        .related(a.0, format!("occupies x {} to {}, y {} to {}", cells(a.1.bbox.x0), cells(a.1.bbox.x1), cells(a.1.bbox.y0), cells(a.1.bbox.y1)))
        .related(b.0, format!("occupies x {} to {}, y {} to {}", cells(b.1.bbox.x0), cells(b.1.bbox.x1), cells(b.1.bbox.y0), cells(b.1.bbox.y1)))
        .suggest("Start and end the path on the walls of the two spaces. Path points are lattice points along the centre line; with an odd width, use half-cell coordinates such as 3.5 to stay on the grid."));
    };
    Ok((poly, ends))
}

/// Each end of a corridor opens into its space unless a connection between
/// the two says otherwise.
fn corridor_ends(d: &Dungeon, sps: &[Sp]) -> Vec<Opening> {
    let mut out = Vec::new();
    for sp in sps.iter().filter(|s| s.geo.is_some()) {
        for (space, segment) in &sp.ends {
            let id = &sp.src.id;
            let explicit = d.connections.iter().any(|c| {
                (c.from == *id && c.to.as_ref() == Some(space))
                    || (c.from == *space && c.to.as_ref() == Some(id))
            });
            if explicit {
                continue;
            }
            let width = (segment[1].x - segment[0].x).abs() + (segment[1].y - segment[0].y).abs();
            out.push(Opening {
                id: format!("{id}/{space}"),
                from: id.clone(),
                to: Some(space.clone()),
                ty: "open".to_string(),
                secret: false,
                locked: false,
                visibility: Visibility::Public,
                reveal_when: None,
                implicit: true,
                segment: Some(*segment),
                far: None,
                width,
                stub: None,
                cell: None,
                name: None,
            });
        }
    }
    out
}

// ----- connections (spec 7.5) ----------------------------------------------------

fn resolve_connections(
    d: &Dungeon,
    sps: &[Sp],
    max_stub: C,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Opening> {
    let mut out = Vec::new();
    for c in &d.connections {
        let Some(from) = find(sps, &c.from).and_then(|i| sps[i].geo.as_ref()) else {
            continue;
        };
        let secret = c.secret.unwrap_or(false);
        let mut opening = Opening {
            id: c.id.clone(),
            from: c.from.clone(),
            to: c.to.clone(),
            ty: c.ty.clone(),
            secret,
            locked: c
                .properties
                .get("locked")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            // A secret door is hidden from players unless stated otherwise.
            visibility: c.visibility.unwrap_or(if secret {
                Visibility::Dm
            } else {
                Visibility::Public
            }),
            reveal_when: c.reveal_when.clone(),
            implicit: false,
            segment: None,
            far: None,
            width: num(c.placement.as_ref().and_then(|p| p.width), CELL),
            stub: None,
            cell: None,
            name: c.name.clone(),
        };
        if VERTICAL_TYPES.contains(&c.ty.as_str()) {
            // Stairs, ladders and trapdoors are marked exits on a cell.
            let Some(p) = c.position else {
                diags.push(
                    Diagnostic::new(
                        Code::InvalidValue,
                        format!("Connection `{}` of type `{}` has no `position`. Stairs, ladders and trapdoors take a cell inside `{}` instead of a wall placement.", c.id, c.ty, c.from),
                    )
                    .element(c.id.clone())
                    .suggest("Add `position = [x, y]`, counting cells from the top-left corner of the room."),
                );
                continue;
            };
            let cell = Rect {
                x0: p[0].0,
                y0: p[1].0,
                x1: p[0].0 + CELL,
                y1: p[1].0 + CELL,
            };
            let corners: Vec<Pt> = cell.corners().iter().map(|q| from.xf.apply(*q)).collect();
            if !corners.iter().all(|q| contains(from, *q)) {
                diags.push(
                    Diagnostic::new(
                        Code::OutOfBounds,
                        format!("Connection `{}` at position [{}, {}] lies outside `{}`, which is {} by {} cells.", c.id, p[0], p[1], c.from, cells(from.xf.w), cells(from.xf.h)),
                    )
                    .element(c.id.clone())
                    .at("position"),
                );
                continue;
            }
            opening.cell = Some(Rect::of(&corners));
            out.push(opening);
            continue;
        }
        let Some(to_id) = &c.to else { continue };
        let Some(to) = find(sps, to_id).and_then(|i| sps[i].geo.as_ref()) else {
            continue;
        };
        match place_connection(c, from, to, max_stub) {
            Ok(p) => {
                opening.segment = Some(p.near);
                if p.poly.is_some() {
                    opening.far = Some(p.far);
                }
                opening.stub = p.poly;
                out.push(opening);
            }
            Err(diag) => diags.push(diag),
        }
    }
    out
}

fn kind_word(c: &Connection) -> &'static str {
    match c.ty.as_str() {
        "door" => "Door",
        "archway" => "Archway",
        _ => "Opening",
    }
}

fn place_connection(
    c: &Connection,
    from: &Geo,
    to: &Geo,
    max_stub: C,
) -> Result<Passage, Diagnostic> {
    let (id, to_id) = (&c.id, c.to.as_deref().unwrap_or_default());
    let placement = c.placement.clone().unwrap_or_default();
    let w = num(placement.width, CELL);
    if w <= 0 {
        return Err(Diagnostic::new(
            Code::InvalidValue,
            format!(
                "Connection `{id}` has width {}, but the width must be positive.",
                cells(w)
            ),
        )
        .element(id.clone())
        .at("placement.width"));
    }
    if placement.bearing.is_some() {
        return Err(Diagnostic::new(
            Code::InvalidValue,
            format!("Connection `{id}` uses `bearing`, which this release does not support yet. Use `wall` with north, south, east or west."),
        )
        .element(id.clone())
        .at("placement.bearing"));
    }
    let usable = |k: &Cand| k.d <= max_stub;
    let pick = |s: &Side| match (placement.wall, placement.edge) {
        (Some(wall), _) => s.name == wall,
        (None, Some(edge)) => s.index as i64 == edge,
        (None, None) => true,
    };
    let (cands, narrow) = candidates(from, to, w, &pick);
    let Some(best) = cands.iter().find(|k| usable(k)) else {
        let chosen = match (placement.wall, placement.edge) {
            (Some(wall), _) => Some((format!("the {} wall", wall.as_str()), "placement.wall")),
            (None, Some(edge)) => Some((format!("edge {edge}"), "placement.edge")),
            (None, None) => None,
        };
        if let Some(n) = narrow {
            return Err(Diagnostic::new(
                Code::DoorNotPlaceable,
                format!("{} `{id}` is {} cells wide, but `{}` and `{to_id}` share only {} cells of wall.", kind_word(c), cells(w), c.from, cells(n)),
            )
            .element(id.clone())
            .at("placement")
            .suggest("Reduce `placement.width`, or move the rooms so that they share more wall."));
        }
        // The chosen wall may be the wrong one: look at all of them.
        let (anywhere, _) = candidates(from, to, w, &|_| true);
        if let (Some((what, at)), Some(other)) = (&chosen, anywhere.iter().find(|k| usable(k))) {
            let side = &from.sides[other.ia];
            let mut diag = Diagnostic::new(
                Code::DoorNotPlaceable,
                format!(
                    "{} `{id}` is placed on {what} of `{}`, which does not face `{to_id}`. The two spaces meet on the {} wall of `{}`.",
                    kind_word(c),
                    c.from,
                    side.name.as_str(),
                    c.from
                ),
            )
            .element(id.clone())
            .at(*at)
            .suggest(format!("Set `placement.wall = {}`, or remove `placement` to place the opening automatically.", side.name.as_str()));
            if placement.edge.is_none() && placement.offset.is_none() {
                diag = diag.fix(Op::set(
                    id,
                    "placement.wall",
                    Value::Str(side.name.as_str().to_string()),
                ));
            }
            return Err(diag);
        }
        let nearest = anywhere.first().map(|k| format!(" Their nearest facing walls are {} cells apart; a stub passage bridges at most {}.", cells(k.d), cells(max_stub)));
        return Err(Diagnostic::new(
            Code::UnresolvableConnection,
            format!("Connection `{id}` cannot join `{}` and `{to_id}`: they share no wall and no walls face each other closely.{}", c.from, nearest.unwrap_or_default()),
        )
        .element(id.clone())
        .suggest(format!(
            "Make the rooms adjacent with `layout {{ adjacent_to = \"{}\", direction = ... }}`, or join them with a corridor: `corridor \"{id}_hall\" {{ connects = [\"{}\", \"{to_id}\"] }}`.",
            c.from, c.from
        )));
    };
    let offset = match placement.offset {
        None => default_offset(best, w),
        Some(o) if o.0 >= best.lo && o.0 + w <= best.hi && (!best.fixed || o.0 == best.lo) => o.0,
        Some(o) => {
            let side = &from.sides[best.ia];
            let centred = default_offset(best, w);
            let (lo, hi) = (ceil_cell(best.lo), floor_cell(best.hi - w));
            let range = if best.fixed {
                format!(
                    "On a circular wall the opening sits at the compass point, at offset {}.",
                    cells(best.lo)
                )
            } else if lo <= hi {
                format!("Valid offsets are {} to {}.", cells(lo), cells(hi))
            } else {
                format!(
                    "The only valid offsets are {} to {}.",
                    cells(best.lo),
                    cells(best.hi - w)
                )
            };
            return Err(Diagnostic::new(
                Code::DoorNotPlaceable,
                format!(
                    "{} `{id}` at offset {o} on the {} wall of `{}` lies outside the wall shared with `{to_id}`. {range}",
                    kind_word(c),
                    side.name.as_str(),
                    c.from
                ),
            )
            .element(id.clone())
            .at("placement.offset")
            .related(to_id, format!("shares {} of that wall", span_text(side, best.lo, best.hi)))
            .suggest(format!(
                "Set placement.offset between {} and {}, or remove it to centre the {}.",
                cells(if lo <= hi { lo } else { best.lo }),
                cells(if lo <= hi { hi } else { best.hi - w }),
                kind_word(c).to_lowercase()
            ))
            .fix(Op::set(id, "placement.offset", Value::Num(Num(centred)))));
        }
    };
    Ok(passage(from, to, best, offset, w))
}

// ----- elements (spec 7.7) -------------------------------------------------------

/// Whether a point lies inside or on the boundary of a space.
fn contains(geo: &Geo, p: Pt) -> bool {
    match geo.circle {
        Some(c) => c.contains(p),
        None => locate(p, &geo.poly) != Loc::Outside,
    }
}

fn anchor_fraction(anchor: Anchor) -> (C, C) {
    // Halves of the free space: 0 is the start, 1 the centre, 2 the end.
    match anchor {
        Anchor::Center => (1, 1),
        Anchor::North => (1, 0),
        Anchor::NorthEast => (2, 0),
        Anchor::East => (2, 1),
        Anchor::SouthEast => (2, 2),
        Anchor::South => (1, 2),
        Anchor::SouthWest => (0, 2),
        Anchor::West => (0, 1),
        Anchor::NorthWest => (0, 0),
    }
}

fn resolve_elements(
    d: &Dungeon,
    cat: &Catalog,
    sps: &[Sp],
    diags: &mut Vec<Diagnostic>,
) -> Vec<RElement> {
    let mut out: Vec<RElement> = Vec::new();
    for (space, el) in d.elements() {
        let Some(entry) = cat.get(&el.ty) else {
            continue;
        };
        let owner = match space {
            Some(s) => match find(sps, &s.id).filter(|i| sps[*i].geo.is_some()) {
                Some(i) => Some(i),
                None => continue, // the space did not resolve; already reported
            },
            None => None,
        };
        let size = el.size.map_or_else(
            || {
                let [w, h] = entry.footprint_of(el.subtype.as_deref());
                (w as C * CELL, h as C * CELL)
            },
            |s| (s[0].0, s[1].0),
        );
        let rotation = el.rotation.unwrap_or(0);
        let (fw, fh) = if rotation % 180 == 90 {
            (size.1, size.0)
        } else {
            size
        };
        let mut resolved = RElement {
            id: el.id.clone(),
            class: el.class,
            ty: el.ty.clone(),
            subtype: el.subtype.clone(),
            category: entry.category.clone(),
            label: match (
                &el.subtype,
                el.properties.get("label").and_then(Value::as_str),
            ) {
                (_, Some(label)) if el.ty == "custom" => label.to_string(),
                (Some(subtype), _) => title_case(subtype),
                _ => entry.label.clone(),
            },
            type_label: entry.label.to_lowercase(),
            glyph: entry.glyph.clone(),
            layer: entry.layer.clone(),
            name: el.name.clone(),
            space: space.map(|s| s.id.clone()),
            footprint: Vec::new(),
            bbox: Rect::default(),
            rotation,
            facing: el.facing,
            wall: None,
            visibility: el.visibility.unwrap_or_else(|| {
                Visibility::parse(&entry.default_visibility).unwrap_or(Visibility::Public)
            }),
            reveal_when: el.reveal_when.clone(),
            blocking: entry.blocking,
            reference: el.reference.clone(),
            contained_in: el.contained_in.clone(),
            light_radius: (el.ty == "lighting").then(|| {
                el.properties
                    .get("radius")
                    .and_then(Value::as_num)
                    .map_or(4 * CELL, |n| n.0)
            }),
            properties: el.properties.clone(),
            tags: el.tags.clone(),
        };
        if el.contained_in.is_some() {
            // An item inside a container has no position of its own.
            out.push(resolved);
            continue;
        }
        match place_element(
            el,
            owner.map(|i| (&sps[i].src.id, sps[i].geo.as_ref().expect("resolved"))),
            sps,
            (fw, fh),
        ) {
            Ok((footprint, space_id, wall)) => {
                resolved.bbox = Rect::of(&footprint);
                resolved.footprint = footprint;
                resolved.space = Some(space_id);
                resolved.wall = wall;
                out.push(resolved);
            }
            Err(diag) => diags.push(diag),
        }
    }
    // Two blocking features may not share a cell.
    for j in 0..out.len() {
        for i in 0..j {
            let (a, b) = (&out[i], &out[j]);
            if a.blocking
                && b.blocking
                && a.space == b.space
                && !a.footprint.is_empty()
                && !b.footprint.is_empty()
                && a.bbox.overlaps(&b.bbox)
            {
                diags.push(
                    Diagnostic::new(
                        Code::ElementOverlap,
                        format!(
                            "`{}` and `{}` are both blocking features and share a cell in `{}`.",
                            b.id,
                            a.id,
                            b.space.as_deref().unwrap_or_default()
                        ),
                    )
                    .element(b.id.clone())
                    .at("position")
                    .related(a.id.clone(), "occupies the same cell")
                    .suggest("Move one of the two to a free cell."),
                );
            }
        }
    }
    out
}

type Placed = (Vec<Pt>, String, Option<Direction>);

fn place_element(
    el: &Element,
    owner: Option<(&String, &Geo)>,
    sps: &[Sp],
    (fw, fh): (C, C),
) -> Result<Placed, Diagnostic> {
    let id = &el.id;
    let rect = |x: C, y: C| Rect {
        x0: x,
        y0: y,
        x1: x + fw,
        y1: y + fh,
    };
    let Some((space_id, geo)) = owner else {
        // A dungeon-level element: the position is on the map, and the
        // space is the one that holds the centre of the footprint.
        let Some(p) = el.position else {
            return Err(Diagnostic::new(
                Code::InvalidValue,
                format!("`{id}` is declared outside any room and has no `position`. Nest it in a room, set `room = \"...\"`, or give it a position on the map."),
            )
            .element(id.clone()));
        };
        let r = rect(p[0].0, p[1].0);
        let centre = ((r.x0 + r.x1) as i128, (r.y0 + r.y1) as i128);
        let holder = sps.iter().find(|s| {
            s.geo
                .as_ref()
                .is_some_and(|g| locate_scaled(centre.0, centre.1, 2, &g.poly) != Loc::Outside)
        });
        return match holder {
            Some(s) => Ok((r.corners().to_vec(), s.src.id.clone(), None)),
            None => Err(Diagnostic::new(
                Code::OutOfBounds,
                format!("`{id}` at map position [{}, {}] is not inside any room or corridor.", p[0], p[1]),
            )
            .element(id.clone())
            .at("position")
            .suggest("Nest the element in its room and use a position counted from that room's top-left corner.")),
        };
    };
    let (w, h) = (geo.xf.w, geo.xf.h);
    let inside = |corners: &[Pt]| corners.iter().all(|p| contains(geo, *p));
    let to_map = |r: Rect| -> Vec<Pt> { r.corners().iter().map(|p| geo.xf.apply(*p)).collect() };
    let placement = el.placement.as_ref();
    if let Some(wall) = placement.and_then(|p| p.wall) {
        let Some(side) = geo.sides.iter().find(|s| s.name == wall) else {
            return Err(Diagnostic::new(Code::OutOfBounds, format!("`{id}` is placed against the {} wall of `{space_id}`, which has no straight wall facing {}.", wall.as_str(), wall.as_str()))
                .element(id.clone())
                .at("placement.wall"));
        };
        // Along the wall the element takes its width for a north or south
        // wall, its height for an east or west wall.
        let horizontal = matches!(wall, Direction::North | Direction::South);
        let (run, depth) = if horizontal { (fw, fh) } else { (fh, fw) };
        let offset = placement
            .and_then(|p| p.offset)
            .map_or_else(|| floor_cell((side.len() - run) / 2).max(0), |o| o.0);
        let (u, inward) = (side.unit(), dvec(opposite(side.gdir)));
        let a = side.start.along(u, offset).along(inward, side.sagitta(run));
        let corners = clockwise(vec![
            a,
            a.along(u, run),
            a.along(u, run).along(inward, depth),
            a.along(inward, depth),
        ]);
        if !inside(&corners) {
            return Err(Diagnostic::new(
                Code::OutOfBounds,
                format!(
                    "`{id}` at offset {} on the {} wall of `{space_id}` does not fit: the wall is {} cells long and the element takes {}.",
                    cells(offset),
                    wall.as_str(),
                    cells(side.len()),
                    cells(run)
                ),
            )
            .element(id.clone())
            .at("placement.offset")
            .suggest(format!("Set placement.offset between 0 and {}, or remove it to centre the element on the wall.", cells(floor_cell(side.len() - run).max(0)))));
        }
        return Ok((corners, space_id.clone(), Some(side.gdir)));
    }
    let local = if let Some(anchor) = placement.and_then(|p| p.anchor) {
        let (fx, fy) = anchor_fraction(anchor);
        rect((w - fw) * fx / 2, (h - fh) * fy / 2)
    } else if let Some(p) = el.position {
        rect(p[0].0, p[1].0)
    } else {
        return Err(Diagnostic::new(
            Code::InvalidValue,
            format!("`{id}` has no place in `{space_id}`. Give it `position = [x, y]`, `placement {{ wall = north, offset = 2 }}` or `placement {{ anchor = center }}`."),
        )
        .element(id.clone())
        .fix(Op::set(id, "placement.anchor", Value::Str("center".to_string()))));
    };
    let corners = to_map(local);
    if inside(&corners) {
        return Ok((corners, space_id.clone(), None));
    }
    let mut diag = Diagnostic::new(
        Code::OutOfBounds,
        format!(
            "`{id}` at position [{}, {}] with a footprint of {} by {} cells lies outside `{space_id}`, which is {} by {} cells.",
            cells(local.x0),
            cells(local.y0),
            cells(fw),
            cells(fh),
            cells(w),
            cells(h)
        ),
    )
    .element(id.clone())
    .at("position");
    // Propose the nearest whole cell where the footprint fits.
    // The search starts where the footprint is pushed back inside the
    // bounding box, and looks at the whole cells around that point.
    let mut nearest: Option<(i128, C, C)> = None;
    let base_x = floor_cell(local.x0.clamp(0, (w - fw).max(0)));
    let base_y = floor_cell(local.y0.clamp(0, (h - fh).max(0)));
    for dy in -16..=16 {
        for dx in -16..=16 {
            let (cx, cy) = (base_x + dx * CELL, base_y + dy * CELL);
            let fits = cx >= 0 && cy >= 0 && cx + fw <= w && cy + fh <= h;
            if fits && inside(&to_map(rect(cx, cy))) {
                let dist = (cx - local.x0) as i128 * (cx - local.x0) as i128
                    + (cy - local.y0) as i128 * (cy - local.y0) as i128;
                if nearest.is_none_or(|n| dist < n.0) {
                    nearest = Some((dist, cx, cy));
                }
            }
        }
    }
    if let (Some((_, x, y)), true) = (nearest, el.position.is_some()) {
        diag = diag
            .suggest(format!("The nearest position that fits is [{}, {}]. Positions count cells from 0 at the room's top-left corner.", cells(x), cells(y)))
            .fix(Op::set(id, "position", Value::List(vec![Value::Num(Num(x)), Value::Num(Num(y))])));
    }
    Err(diag)
}

fn resolve_notes(
    d: &Dungeon,
    sps: &[Sp],
    elements: &[RElement],
    openings: &[Opening],
) -> Vec<RNote> {
    let mut out = Vec::new();
    for (k, note) in d.notes.iter().enumerate() {
        let target_geo = note
            .target
            .as_ref()
            .and_then(|t| find(sps, t))
            .and_then(|i| sps[i].geo.as_ref());
        let cell_centre = |p: &[Num; 2]| Pt::new(p[0].0 + CELL / 2, p[1].0 + CELL / 2);
        let anchor = match (&note.position, target_geo, &note.target) {
            (Some(p), Some(geo), _) => Some(geo.xf.apply(cell_centre(p))),
            (Some(p), None, _) => Some(cell_centre(p)),
            (None, Some(geo), _) => {
                // The top-left cell, or top centre where that is outside.
                let corner = geo.xf.apply(Pt::new(CELL / 2, CELL / 2));
                Some(if contains(geo, corner) {
                    corner
                } else {
                    Pt::new((geo.bbox.x0 + geo.bbox.x1) / 2, geo.bbox.y0 + CELL / 2)
                })
            }
            (None, None, Some(target)) => elements
                .iter()
                .find(|e| e.id == *target && !e.footprint.is_empty())
                .map(|e| Pt::new(e.bbox.x1, e.bbox.y0))
                .or_else(|| {
                    openings
                        .iter()
                        .find(|o| o.id == *target)
                        .and_then(|o| o.segment)
                        .map(|s| Pt::new((s[0].x + s[1].x) / 2, (s[0].y + s[1].y) / 2))
                }),
            (None, None, None) => None,
        };
        out.push(RNote {
            id: note.id.clone(),
            number: k as u32 + 1,
            target: note.target.clone(),
            anchor,
            text: note.text.clone(),
            visibility: note.visibility.unwrap_or(Visibility::Dm),
            reveal_when: note.reveal_when.clone(),
        });
    }
    out
}

// ----- reachability (spec 7.9) ---------------------------------------------------

fn join<'a>(graph: &mut BTreeMap<&'a str, Vec<&'a str>>, a: &'a str, b: &'a str) {
    graph.entry(a).or_default().push(b);
    graph.entry(b).or_default().push(a);
}

fn reachability<'a>(
    d: &'a Dungeon,
    spaces: &'a [RSpace],
    openings: &'a [Opening],
    diags: &mut Vec<Diagnostic>,
) -> Reach {
    let rooms = || spaces.iter().filter(|s| s.kind == SpaceKind::Room);
    let entrance = rooms()
        .find(|s| s.tags.iter().any(|t| t == "entrance"))
        .or_else(|| rooms().next())
        .map(|s| s.id.clone());
    let mut links: Vec<[String; 3]> = Vec::new();
    let mut graph: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for o in openings {
        let Some(to) = &o.to else { continue };
        if o.segment.is_none() || !spaces.iter().any(|s| s.id == *to) {
            continue;
        }
        links.push([o.from.clone(), to.clone(), o.id.clone()]);
        join(&mut graph, &o.from, to);
    }
    // A declared link that failed to resolve has its own error. It still
    // counts here, so that one mistake is not reported twice.
    for c in &d.connections {
        if let Some(to) = &c.to {
            join(&mut graph, &c.from, to);
        }
    }
    for s in d.spaces.iter().filter(|s| s.kind == SpaceKind::Corridor) {
        for end in &s.connects {
            join(&mut graph, &s.id, end);
        }
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut queue: Vec<&str> = entrance.as_deref().into_iter().collect();
    while let Some(current) = queue.pop() {
        if seen.insert(current) {
            queue.extend(graph.get(current).into_iter().flatten().copied());
        }
    }
    let unreachable: Vec<String> = rooms()
        .filter(|s| !seen.contains(s.id.as_str()))
        .map(|s| s.id.clone())
        .collect();
    for id in &unreachable {
        diags.push(
            Diagnostic::new(
                Code::UnreachableRoom,
                format!("Room `{id}` cannot be reached from the entrance `{}`: no connection or corridor leads to it.", entrance.as_deref().unwrap_or_default()),
            )
            .element(id.clone())
            .suggest(format!("Join it to the dungeon, for example `connection \"{id}_door\" {{ from = \"...\", to = \"{id}\", type = door }}`.")),
        );
    }
    Reach {
        entrance,
        links,
        unreachable,
    }
}
