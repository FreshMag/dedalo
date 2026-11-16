//! Walls derived from space boundaries (spec 7.4).
//!
//! Every boundary edge of a space is a wall unless an opening is cut into
//! it. Where two spaces share a boundary segment there is one wall. The view
//! stage calls this again after filtering, so hidden rooms and secret doors
//! leave solid walls behind.

use std::collections::BTreeMap;

use crate::model::{Opening, RSpace, Wall};
use crate::prim::{C, Pt, edges};

/// A line through lattice points: reduced direction and offset.
type LineKey = (C, C, i128);

fn gcd(a: C, b: C) -> C {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

/// The line through `a` and `b`, and the positions of both along it.
fn line_of(a: Pt, b: Pt) -> Option<(LineKey, C, C)> {
    let (mut dx, mut dy) = (b.x - a.x, b.y - a.y);
    let g = gcd(dx, dy);
    if g == 0 {
        return None;
    }
    dx /= g;
    dy /= g;
    if dx < 0 || (dx == 0 && dy < 0) {
        dx = -dx;
        dy = -dy;
    }
    let offset = dx as i128 * a.y as i128 - dy as i128 * a.x as i128;
    let along = |p: Pt| dx * p.x + dy * p.y;
    Some(((dx, dy, offset), along(a), along(b)))
}

#[derive(Default)]
struct Line {
    /// Edge intervals with the index of the outline they belong to.
    edges: Vec<(C, C, usize)>,
    /// Opening intervals with the opening ID.
    cuts: Vec<(C, C, String)>,
    points: BTreeMap<C, Pt>,
}

/// Derives the walls of a set of spaces, cut by a set of openings.
pub fn derive_walls(spaces: &[RSpace], openings: &[Opening]) -> Vec<Wall> {
    // Outlines: the straight-edged spaces, then the stub passages.
    let mut outlines: Vec<(Option<&RSpace>, &[Pt])> = spaces
        .iter()
        .filter(|s| s.circle.is_none())
        .map(|s| (Some(s), s.polygon.as_slice()))
        .collect();
    outlines.extend(
        openings
            .iter()
            .filter_map(|o| o.stub.as_deref())
            .map(|poly| (None, poly)),
    );

    let mut lines: BTreeMap<LineKey, Line> = BTreeMap::new();
    for (index, (_, poly)) in outlines.iter().enumerate() {
        for (a, b) in edges(poly) {
            let Some((key, ta, tb)) = line_of(a, b) else {
                continue;
            };
            let line = lines.entry(key).or_default();
            line.edges.push((ta.min(tb), ta.max(tb), index));
            line.points.insert(ta, a);
            line.points.insert(tb, b);
        }
    }
    let cut_segments = |o: &Opening| [o.segment, o.far].into_iter().flatten();
    for o in openings {
        for [a, b] in cut_segments(o) {
            let Some((key, ta, tb)) = line_of(a, b) else {
                continue;
            };
            if let Some(line) = lines.get_mut(&key) {
                line.cuts.push((ta.min(tb), ta.max(tb), o.id.clone()));
                line.points.insert(ta, a);
                line.points.insert(tb, b);
            }
        }
    }

    let mut walls = Vec::new();
    for line in lines.values() {
        let stops: Vec<C> = line.points.keys().copied().collect();
        // The run of consecutive intervals bounded by the same outlines.
        let mut run: Option<(Vec<usize>, C, C)> = None;
        let mut flush = |run: &mut Option<(Vec<usize>, C, C)>| {
            if let Some((cover, start, end)) = run.take() {
                walls.push(straight_wall(line, &outlines, &cover, start, end));
            }
        };
        for pair in stops.windows(2) {
            let (ta, tb) = (pair[0], pair[1]);
            let mut cover: Vec<usize> = line
                .edges
                .iter()
                .filter(|(t0, t1, _)| *t0 <= ta && tb <= *t1)
                .map(|e| e.2)
                .collect();
            cover.sort_unstable();
            cover.dedup();
            match &mut run {
                Some((current, _, end)) if *current == cover && *end == ta => *end = tb,
                _ => {
                    flush(&mut run);
                    if !cover.is_empty() {
                        run = Some((cover, ta, tb));
                    }
                }
            }
        }
        flush(&mut run);
    }

    for space in spaces {
        let Some(circle) = space.circle else { continue };
        let on_circle = |p: Pt| circle.on_outline(p);
        let clockwise_from_north = |p: Pt| circle.angle(p);
        let mut gaps: Vec<([Pt; 2], &str)> = Vec::new();
        for o in openings
            .iter()
            .filter(|o| o.from == space.id || o.to.as_deref() == Some(space.id.as_str()))
        {
            for [a, b] in cut_segments(o) {
                if on_circle(a) && on_circle(b) {
                    // Order each gap clockwise along the minor arc.
                    let cross = (a.x - circle.center.x) as i128 * (b.y - circle.center.y) as i128
                        - (a.y - circle.center.y) as i128 * (b.x - circle.center.x) as i128;
                    gaps.push((if cross >= 0 { [a, b] } else { [b, a] }, &o.id));
                }
            }
        }
        gaps.sort_by(|x, y| clockwise_from_north(x.0[0]).total_cmp(&clockwise_from_north(y.0[0])));
        walls.push(Wall {
            line: None,
            circle: Some(circle),
            spaces: vec![space.id.clone()],
            material: space.wall.clone(),
            openings: gaps.iter().map(|g| g.1.to_string()).collect(),
            solid: Vec::new(),
            gaps: gaps.iter().map(|g| g.0).collect(),
        });
    }
    walls
}

fn straight_wall(
    line: &Line,
    outlines: &[(Option<&RSpace>, &[Pt])],
    cover: &[usize],
    start: C,
    end: C,
) -> Wall {
    let bounded: Vec<&RSpace> = cover.iter().filter_map(|i| outlines[*i].0).collect();
    let mut cuts: Vec<&(C, C, String)> = line
        .cuts
        .iter()
        .filter(|(t0, t1, _)| *t0 < end && start < *t1)
        .collect();
    cuts.sort_by_key(|c| (c.0, c.1));
    let mut solid = Vec::new();
    let mut cursor = start;
    for (t0, t1, _) in &cuts {
        if *t0 > cursor {
            solid.push([line.points[&cursor], line.points[t0]]);
        }
        cursor = cursor.max((*t1).min(end));
    }
    if cursor < end {
        solid.push([line.points[&cursor], line.points[&end]]);
    }
    let mut openings: Vec<String> = cuts.iter().map(|c| c.2.clone()).collect();
    openings.dedup();
    Wall {
        line: Some([line.points[&start], line.points[&end]]),
        circle: None,
        spaces: bounded.iter().map(|s| s.id.clone()).collect(),
        material: bounded.first().map(|s| s.wall.clone()).unwrap_or_default(),
        openings,
        solid,
        gaps: Vec::new(),
    }
}
