//! Exact test for overlapping polygon interiors (spec 7.3).
//!
//! Touching edges are fine and are the normal case: adjacent rooms share
//! walls. The test is exact because it works on integer coordinates and on
//! rational probe points scaled to integers.

use crate::prim::{Loc, Pt, Rect, edges, locate_scaled, on_segment, properly_cross};

/// Whether the interiors of two simple polygons intersect.
pub fn interiors_intersect(a: &[Pt], b: &[Pt]) -> bool {
    if a.len() < 3 || b.len() < 3 || !Rect::of(a).overlaps(&Rect::of(b)) {
        return false;
    }
    for (a0, a1) in edges(a) {
        for (b0, b1) in edges(b) {
            if properly_cross(a0, a1, b0, b1) {
                return true;
            }
        }
    }
    // No edges cross. Either one boundary runs through the other interior,
    // or the two polygons cover the same region, or the interiors are disjoint.
    let (a_enters_b, a_on_b) = probe(a, b);
    if a_enters_b {
        return true;
    }
    let (b_enters_a, b_on_a) = probe(b, a);
    b_enters_a || (a_on_b && b_on_a)
}

/// Splits each edge of `a` where the boundary of `b` meets it and tests the
/// midpoint of every piece. Returns whether a midpoint lies strictly inside
/// `b`, and whether every midpoint lies on the boundary of `b`.
fn probe(a: &[Pt], b: &[Pt]) -> (bool, bool) {
    let mut all_on_boundary = true;
    for (a0, a1) in edges(a) {
        let dir = a1.minus(a0);
        let len2 = dir.dot(dir) as i128;
        if len2 == 0 {
            continue;
        }
        // Parameters along the edge, as numerators over `len2`.
        let mut cuts: Vec<i128> = vec![0, len2];
        cuts.extend(
            b.iter()
                .filter(|v| on_segment(a0, a1, **v))
                .map(|v| v.minus(a0).dot(dir) as i128),
        );
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            // The midpoint, as a point with denominator `2 * len2`.
            let (mid, d) = (pair[0] + pair[1], 2 * len2);
            let px = a0.x as i128 * d + dir.x as i128 * mid;
            let py = a0.y as i128 * d + dir.y as i128 * mid;
            match locate_scaled(px, py, d, b) {
                Loc::Inside => return (true, false),
                Loc::Boundary => {}
                Loc::Outside => all_on_boundary = false,
            }
        }
    }
    (false, all_on_boundary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<Pt> {
        vec![
            Pt::new(x0, y0),
            Pt::new(x1, y0),
            Pt::new(x1, y1),
            Pt::new(x0, y1),
        ]
    }

    #[test]
    fn touching_is_not_overlapping() {
        let a = rect(0, 0, 8000, 6000);
        assert!(
            !interiors_intersect(&a, &rect(8000, 0, 12000, 6000)),
            "shared edge"
        );
        assert!(
            !interiors_intersect(&a, &rect(1000, -6000, 7000, 0)),
            "partly shared edge"
        );
        assert!(
            !interiors_intersect(&a, &rect(8000, 6000, 9000, 7000)),
            "shared corner"
        );
        assert!(
            !interiors_intersect(&a, &rect(9000, 0, 12000, 6000)),
            "apart"
        );
    }

    #[test]
    fn overlaps_are_found_without_crossing_edges() {
        let a = rect(0, 0, 8000, 6000);
        assert!(
            interiors_intersect(&a, &rect(7000, -7000, 15000, 1000)),
            "the draft's reference example"
        );
        assert!(interiors_intersect(&a, &a), "identical");
        assert!(
            interiors_intersect(&a, &rect(0, 0, 4000, 6000)),
            "half, sharing three edges"
        );
        assert!(
            interiors_intersect(&a, &rect(2000, 2000, 3000, 3000)),
            "contained"
        );
        assert!(
            interiors_intersect(&rect(2000, 2000, 3000, 3000), &a),
            "containing"
        );
    }

    #[test]
    fn concave_and_diagonal_shapes() {
        // An L shape and a square sitting in its notch.
        let l = vec![
            Pt::new(0, 0),
            Pt::new(4000, 0),
            Pt::new(4000, 2000),
            Pt::new(2000, 2000),
            Pt::new(2000, 4000),
            Pt::new(0, 4000),
        ];
        assert!(!interiors_intersect(&l, &rect(2000, 2000, 4000, 4000)));
        assert!(interiors_intersect(&l, &rect(1000, 1000, 4000, 4000)));
        // A diamond whose corner touches a square's edge.
        let diamond = vec![
            Pt::new(6000, 0),
            Pt::new(8000, 2000),
            Pt::new(6000, 4000),
            Pt::new(4000, 2000),
        ];
        assert!(!interiors_intersect(&rect(0, 0, 4000, 4000), &diamond));
        assert!(interiors_intersect(&rect(0, 0, 5000, 4000), &diamond));
    }
}
