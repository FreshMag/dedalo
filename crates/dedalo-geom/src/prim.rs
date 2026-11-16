//! Integer geometry primitives.
//!
//! All coordinates are integers in thousandths of a cell (spec 7.1), so every
//! predicate here is exact and gives the same answer on every platform.

use dedalo_model::{Direction, Num};
use serde::ser::SerializeTuple;
use serde::{Serialize, Serializer};

/// A coordinate in thousandths of a cell.
pub type C = i64;
/// One cell.
pub const CELL: C = 1000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pt {
    pub x: C,
    pub y: C,
}

impl Pt {
    pub const fn new(x: C, y: C) -> Pt {
        Pt { x, y }
    }

    pub fn plus(self, o: Pt) -> Pt {
        Pt::new(self.x + o.x, self.y + o.y)
    }

    pub fn minus(self, o: Pt) -> Pt {
        Pt::new(self.x - o.x, self.y - o.y)
    }

    pub fn scale(self, k: C) -> Pt {
        Pt::new(self.x * k, self.y * k)
    }

    pub fn dot(self, o: Pt) -> C {
        self.x * o.x + self.y * o.y
    }

    /// This point moved `k` thousandths along the unit vector `dir`.
    pub fn along(self, dir: Pt, k: C) -> Pt {
        self.plus(dir.scale(k))
    }
}

/// Points serialise as `[x, y]` in cells.
impl Serialize for Pt {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut t = s.serialize_tuple(2)?;
        t.serialize_element(&Num(self.x))?;
        t.serialize_element(&Num(self.y))?;
        t.end()
    }
}

/// An axis-aligned rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x0: C,
    pub y0: C,
    pub x1: C,
    pub y1: C,
}

impl Rect {
    pub fn of(points: &[Pt]) -> Rect {
        let mut r = Rect {
            x0: C::MAX,
            y0: C::MAX,
            x1: C::MIN,
            y1: C::MIN,
        };
        for p in points {
            r.x0 = r.x0.min(p.x);
            r.y0 = r.y0.min(p.y);
            r.x1 = r.x1.max(p.x);
            r.y1 = r.y1.max(p.y);
        }
        if points.is_empty() {
            Rect::default()
        } else {
            r
        }
    }

    pub fn w(&self) -> C {
        self.x1 - self.x0
    }

    pub fn h(&self) -> C {
        self.y1 - self.y0
    }

    pub fn union(&self, o: &Rect) -> Rect {
        Rect {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }

    /// Whether the interiors intersect; touching edges do not count.
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1
    }

    pub fn intersection(&self, o: &Rect) -> Rect {
        Rect {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        }
    }

    pub fn corners(&self) -> [Pt; 4] {
        [
            Pt::new(self.x0, self.y0),
            Pt::new(self.x1, self.y0),
            Pt::new(self.x1, self.y1),
            Pt::new(self.x0, self.y1),
        ]
    }
}

/// Rectangles serialise as `[x0, y0, x1, y1]` in cells.
impl Serialize for Rect {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut t = s.serialize_tuple(4)?;
        for v in [self.x0, self.y0, self.x1, self.y1] {
            t.serialize_element(&Num(v))?;
        }
        t.end()
    }
}

/// The unit vector of a direction. Y grows south.
pub fn dvec(d: Direction) -> Pt {
    match d {
        Direction::North => Pt::new(0, -1),
        Direction::South => Pt::new(0, 1),
        Direction::East => Pt::new(1, 0),
        Direction::West => Pt::new(-1, 0),
    }
}

pub fn opposite(d: Direction) -> Direction {
    match d {
        Direction::North => Direction::South,
        Direction::South => Direction::North,
        Direction::East => Direction::West,
        Direction::West => Direction::East,
    }
}

/// `d` turned clockwise by `quarters` quarter turns.
pub fn rotate_dir(d: Direction, quarters: u8) -> Direction {
    const ORDER: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];
    let i = ORDER.iter().position(|x| *x == d).unwrap_or(0);
    ORDER[(i + quarters as usize) % 4]
}

/// Twice the signed area of triangle `a b c`.
pub fn orient(a: Pt, b: Pt, c: Pt) -> i128 {
    (b.x - a.x) as i128 * (c.y - a.y) as i128 - (b.y - a.y) as i128 * (c.x - a.x) as i128
}

/// Whether `p` lies on the closed segment `a b`.
pub fn on_segment(a: Pt, b: Pt, p: Pt) -> bool {
    orient(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

/// Whether two segments cross at a single point interior to both.
pub fn properly_cross(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let (o1, o2, o3, o4) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    ((o1 > 0 && o2 < 0) || (o1 < 0 && o2 > 0)) && ((o3 > 0 && o4 < 0) || (o3 < 0 && o4 > 0))
}

/// Whether two closed segments share any point.
pub fn segments_touch(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    properly_cross(a, b, c, d)
        || on_segment(a, b, c)
        || on_segment(a, b, d)
        || on_segment(c, d, a)
        || on_segment(c, d, b)
}

/// The edges of a closed polygon.
pub fn edges(poly: &[Pt]) -> impl Iterator<Item = (Pt, Pt)> + '_ {
    (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()]))
}

/// Twice the signed area. Positive means clockwise on screen (Y grows south).
pub fn area2(poly: &[Pt]) -> i128 {
    edges(poly)
        .map(|(a, b)| a.x as i128 * b.y as i128 - b.x as i128 * a.y as i128)
        .sum()
}

/// The polygon with clockwise winding.
pub fn clockwise(mut poly: Vec<Pt>) -> Vec<Pt> {
    if area2(&poly) < 0 {
        poly.reverse();
    }
    poly
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loc {
    Inside,
    Boundary,
    Outside,
}

/// Where the rational point `(px / d, py / d)` lies relative to a polygon.
/// `d` must be positive. Exact.
pub fn locate_scaled(px: i128, py: i128, d: i128, poly: &[Pt]) -> Loc {
    let mut inside = false;
    for (a, b) in edges(poly) {
        let (ax, ay, bx, by) = (
            a.x as i128 * d,
            a.y as i128 * d,
            b.x as i128 * d,
            b.y as i128 * d,
        );
        let (dx, dy) = ((b.x - a.x) as i128, (b.y - a.y) as i128);
        // `d` times the cross product of the edge with the vector to the point.
        let cross = dx * (py - ay) - dy * (px - ax);
        if cross == 0
            && px >= ax.min(bx)
            && px <= ax.max(bx)
            && py >= ay.min(by)
            && py <= ay.max(by)
        {
            return Loc::Boundary;
        }
        if (ay > py) != (by > py) && (cross > 0) == (dy > 0) {
            inside = !inside;
        }
    }
    if inside { Loc::Inside } else { Loc::Outside }
}

pub fn locate(p: Pt, poly: &[Pt]) -> Loc {
    locate_scaled(p.x as i128, p.y as i128, 1, poly)
}

/// The largest integer whose square does not exceed `n`.
pub fn isqrt(n: i128) -> i128 {
    if n <= 0 {
        return 0;
    }
    let mut r = (n as f64).sqrt() as i128;
    while r * r > n {
        r -= 1;
    }
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

/// `value` rounded down to a whole cell.
pub fn floor_cell(value: C) -> C {
    value.div_euclid(CELL) * CELL
}

/// `value` rounded up to a whole cell.
pub fn ceil_cell(value: C) -> C {
    -floor_cell(-value)
}

/// A coordinate formatted in cells, for messages.
pub fn cells(value: C) -> String {
    Num(value).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: C, y: C, s: C) -> Vec<Pt> {
        vec![
            Pt::new(x, y),
            Pt::new(x + s, y),
            Pt::new(x + s, y + s),
            Pt::new(x, y + s),
        ]
    }

    #[test]
    fn locate_is_exact_on_boundaries() {
        let sq = square(0, 0, 4000);
        assert_eq!(locate(Pt::new(2000, 2000), &sq), Loc::Inside);
        assert_eq!(locate(Pt::new(4000, 2000), &sq), Loc::Boundary);
        assert_eq!(locate(Pt::new(0, 0), &sq), Loc::Boundary);
        assert_eq!(locate(Pt::new(4001, 2000), &sq), Loc::Outside);
        // The rational point (3999.5, 2000).
        assert_eq!(locate_scaled(7999, 4000, 2, &sq), Loc::Inside);
    }

    #[test]
    fn winding_and_rounding() {
        let sq = square(0, 0, 1000);
        assert!(area2(&sq) > 0);
        let mut reversed = sq.clone();
        reversed.reverse();
        assert_eq!(area2(&clockwise(reversed)), area2(&sq));
        assert_eq!(floor_cell(-1500), -2000);
        assert_eq!(ceil_cell(1001), 2000);
        assert_eq!(isqrt(8_999_999), 2999);
    }
}
