//! The resolved model: the document with absolute geometry (spec 6.5).
//!
//! This is what the view stage receives, and what `dedalo inspect --stage
//! resolved` prints. Coordinates serialise in cells.

use dedalo_model::{Class, Direction, Facing, Num, Props, SpaceKind, Visibility};
use serde::{Serialize, Serializer};

use crate::prim::{C, Pt, Rect};

fn cells<S: Serializer>(value: &C, s: S) -> Result<S::Ok, S::Error> {
    Num(*value).serialize(s)
}

fn opt_cells<S: Serializer>(value: &Option<C>, s: S) -> Result<S::Ok, S::Error> {
    value.map(Num).serialize(s)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resolved {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_unit: Option<String>,
    /// The effective seed of the random stream (spec 7.10).
    pub seed: u64,
    /// Bounding box of everything that resolved.
    pub bounds: Rect,
    /// The floors of a multi-level dungeon, laid out side by side on the
    /// map. Empty for a dungeon on one level.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<RLevel>,
    pub spaces: Vec<RSpace>,
    pub walls: Vec<Wall>,
    pub openings: Vec<Opening>,
    pub elements: Vec<RElement>,
    pub notes: Vec<RNote>,
    pub reach: Reach,
}

/// One floor of a multi-level dungeon and the part of the map it takes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RLevel {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub bounds: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Circle {
    pub center: Pt,
    /// The horizontal radius; the only one of a circle.
    #[serde(serialize_with = "cells")]
    pub radius: C,
    /// The vertical radius of an ellipse; absent for a circle.
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "opt_cells")]
    pub radius_y: Option<C>,
}

impl Circle {
    pub fn rx(&self) -> C {
        self.radius
    }

    pub fn ry(&self) -> C {
        self.radius_y.unwrap_or(self.radius)
    }

    /// Whether a point lies inside or on the outline. Exact.
    pub fn contains(&self, p: Pt) -> bool {
        let (dx, dy) = ((p.x - self.center.x) as i128, (p.y - self.center.y) as i128);
        let (rx, ry) = (self.rx() as i128, self.ry() as i128);
        dx * dx * ry * ry + dy * dy * rx * rx <= rx * rx * ry * ry
    }

    /// Whether a point lies on the outline, within rounding.
    pub fn on_outline(&self, p: Pt) -> bool {
        let (dx, dy) = (
            (p.x - self.center.x) as f64 / self.rx() as f64,
            (p.y - self.center.y) as f64 / self.ry() as f64,
        );
        (dx * dx + dy * dy - 1.0).abs() <= 0.005
    }

    /// The angle of a point on the outline, clockwise from north, in the
    /// circle the ellipse is stretched from.
    pub fn angle(&self, p: Pt) -> f64 {
        let a = libm::atan2(
            (p.x - self.center.x) as f64 / self.rx() as f64,
            -(p.y - self.center.y) as f64 / self.ry() as f64,
        );
        if a < 0.0 {
            a + 2.0 * std::f64::consts::PI
        } else {
            a
        }
    }

    /// The circle after quarter turns: the radii swap on odd turns.
    pub fn turned(mut self, quarters: i64) -> Circle {
        if quarters.rem_euclid(2) == 1 && self.radius_y.is_some() {
            let (rx, ry) = (self.rx(), self.ry());
            self.radius = ry;
            self.radius_y = Some(rx);
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RSpace {
    pub id: String,
    pub kind: SpaceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Absolute outline, clockwise. For a circle, the polygonal approximation.
    pub polygon: Vec<Pt>,
    /// The exact outline when the space is a circle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circle: Option<Circle>,
    pub bbox: Rect,
    /// The level the space is on; `default` in a dungeon without levels.
    pub level: String,
    pub visibility: Visibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reveal_when: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub wall: String,
    pub floor: String,
    /// Corridors: the two spaces it joins.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub connects: Vec<String>,
}

/// A boundary between a space and another space or the outside.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Wall {
    /// The full segment of a straight wall, before openings are cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<[Pt; 2]>,
    /// The circle of a curved wall.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circle: Option<Circle>,
    /// The one or two spaces it bounds.
    pub spaces: Vec<String>,
    pub material: String,
    /// The openings cut into it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<String>,
    /// Straight walls: the pieces that remain after cutting the openings.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub solid: Vec<[Pt; 2]>,
    /// Curved walls: the gaps, each clockwise from its first point.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<[Pt; 2]>,
}

/// A connection resolved to a place on a wall, or to a cell for stairs.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Opening {
    /// The source connection ID; `corridor/space` for a corridor's own end.
    pub id: String,
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(rename = "type")]
    pub ty: String,
    pub secret: bool,
    pub locked: bool,
    pub visibility: Visibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reveal_when: Option<String>,
    /// True for the open end of a corridor, which no connection declares.
    pub implicit: bool,
    /// The opening on the wall of `from`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<[Pt; 2]>,
    /// With a stub passage: the opening on the wall of `to`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub far: Option<[Pt; 2]>,
    #[serde(serialize_with = "cells")]
    pub width: C,
    /// The stub passage that bridges a short gap (spec 7.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub: Option<Vec<Pt>>,
    /// Stairs, ladders and trapdoors: the cell they occupy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The level it is drawn on, in a dungeon with levels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// True for the far end of stairs, a ladder or a trapdoor: the marker
    /// in the room they lead to, where `from` is that room.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub arrival: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RElement {
    pub id: String,
    pub class: Class,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    /// Legend group and visibility toggle, from the catalog.
    pub category: String,
    /// Legend label, from the catalog.
    pub label: String,
    /// The type's own name in lower case, such as `light` or `trap`.
    pub type_label: String,
    /// Text-map character, from the catalog.
    pub glyph: String,
    pub layer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The owning space. Absent only for an item inside a container whose
    /// own space is unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space: Option<String>,
    /// Absolute footprint. Empty for an item held in a container.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub footprint: Vec<Pt>,
    pub bbox: Rect,
    pub rotation: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facing: Option<Facing>,
    /// The wall the element is mounted on, in map directions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall: Option<Direction>,
    pub visibility: Visibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reveal_when: Option<String>,
    pub blocking: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contained_in: Option<String>,
    /// Light sources: radius in cells.
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "opt_cells")]
    pub light_radius: Option<C>,
    #[serde(skip_serializing_if = "Props::is_empty")]
    pub properties: Props,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The level it is drawn on, in a dungeon with levels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RNote {
    pub id: String,
    /// Running number, from 1, in declaration order.
    pub number: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Where the numbered marker goes, when the note has a place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Pt>,
    pub text: String,
    pub visibility: Visibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reveal_when: Option<String>,
    /// The level of its marker, in a dungeon with levels. A note with no
    /// place belongs to every level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

impl Pt {
    fn shift(&mut self, by: Pt) {
        *self = self.plus(by);
    }
}

impl Rect {
    fn shift(&mut self, by: Pt) {
        self.x0 += by.x;
        self.x1 += by.x;
        self.y0 += by.y;
        self.y1 += by.y;
    }
}

impl Resolved {
    /// Moves everything by `by`. Walls are derived again by the caller.
    pub(crate) fn shift(&mut self, by: Pt) {
        self.bounds.shift(by);
        for s in &mut self.spaces {
            s.polygon.iter_mut().for_each(|p| p.shift(by));
            s.bbox.shift(by);
            if let Some(c) = &mut s.circle {
                c.center.shift(by);
            }
        }
        for o in &mut self.openings {
            for seg in [&mut o.segment, &mut o.far].into_iter().flatten() {
                seg.iter_mut().for_each(|p| p.shift(by));
            }
            o.stub.iter_mut().flatten().for_each(|p| p.shift(by));
            if let Some(cell) = &mut o.cell {
                cell.shift(by);
            }
        }
        for e in &mut self.elements {
            e.footprint.iter_mut().for_each(|p| p.shift(by));
            e.bbox.shift(by);
        }
        for n in &mut self.notes {
            if let Some(a) = &mut n.anchor {
                a.shift(by);
            }
        }
    }
}

/// The reachability graph of spaces (spec 7.9).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Reach {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrance: Option<String>,
    /// Pairs of spaces joined by an opening, with the opening's ID.
    pub links: Vec<[String; 3]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unreachable: Vec<String>,
}
