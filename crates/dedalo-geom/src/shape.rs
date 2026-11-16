//! Shape functions to outlines (spec 7.2).

use dedalo_model::suggest::did_you_mean;
use dedalo_model::{ShapeCall, Value};

use crate::prim::{C, CELL, Pt, Rect, area2, cells, edges, on_segment, segments_touch};

/// A shape in its own frame: the origin is the top-left corner of its
/// bounding box, before any rotation.
#[derive(Clone, Debug, PartialEq)]
pub struct Local {
    /// Clockwise outline. For a circle, a polygonal approximation within
    /// 0.01 cell, used for containment and overlap tests.
    pub poly: Vec<Pt>,
    pub w: C,
    pub h: C,
    /// The exact radius when the shape is a circle.
    pub radius: Option<C>,
    /// For each edge of `poly`, its index as the author counted it.
    pub edge_index: Vec<usize>,
}

const SHAPES: &[&str] = &["square", "rectangle", "circle", "polygon"];

fn number(v: &Value) -> Option<C> {
    v.as_num().map(|n| n.0)
}

fn rectangle(w: C, h: C) -> Local {
    Local {
        poly: Rect {
            x0: 0,
            y0: 0,
            x1: w,
            y1: h,
        }
        .corners()
        .to_vec(),
        w,
        h,
        radius: None,
        edge_index: (0..4).collect(),
    }
}

/// Largest side of the bounding box of a shape, in cells.
pub const MAX_SIDE: C = 1000;

/// Builds the outline of a shape call, or says why it is invalid.
pub fn build(call: &ShapeCall) -> Result<Local, String> {
    let local = outline(call)?;
    if local.w > MAX_SIDE * CELL || local.h > MAX_SIDE * CELL {
        return Err(format!(
            "the shape is {} by {} cells; the largest supported is {MAX_SIDE} by {MAX_SIDE}.",
            cells(local.w),
            cells(local.h)
        ));
    }
    Ok(local)
}

fn outline(call: &ShapeCall) -> Result<Local, String> {
    let args = &call.args;
    let numbers: Option<Vec<C>> = args.iter().map(number).collect();
    match (call.func.as_str(), numbers.as_deref()) {
        ("square", Some([side])) => {
            if *side < CELL {
                return Err(format!(
                    "square({}) is smaller than one cell; the side must be at least 1.",
                    cells(*side)
                ));
            }
            Ok(rectangle(*side, *side))
        }
        ("rectangle", Some([w, h])) => {
            if *w < CELL || *h < CELL {
                return Err(format!(
                    "rectangle({}, {}) has a side below one cell; both sides must be at least 1.",
                    cells(*w),
                    cells(*h)
                ));
            }
            Ok(rectangle(*w, *h))
        }
        ("circle", Some([r])) => {
            if 2 * *r < CELL {
                return Err(format!(
                    "circle({}) is smaller than one cell; the radius must be at least 0.5.",
                    cells(*r)
                ));
            }
            Ok(circle(*r))
        }
        ("square", _) => Err("square takes one number: square(side).".to_string()),
        ("rectangle", _) => {
            Err("rectangle takes two numbers: rectangle(width, height).".to_string())
        }
        ("circle", _) => Err("circle takes one number: circle(radius).".to_string()),
        ("polygon", _) => polygon(args),
        ("ellipse" | "l_shape", _) => Err(format!(
            "the shape `{}` arrives in release 1.0; use polygon(points) until then.",
            call.func
        )),
        ("composite", _) => Err(
            "the shape `composite` arrives after release 1.0; use polygon(points) until then."
                .to_string(),
        ),
        (other, _) => Err(format!(
            "`{other}` is not a shape.{} The shapes are square(side), rectangle(width, height), circle(radius) and polygon(points).",
            did_you_mean(other, SHAPES.iter().copied())
        )),
    }
}

/// A circle inscribed in a `2r` by `2r` bounding box. The vertices start at
/// north and run clockwise, so the four compass points are exact vertices.
fn circle(r: C) -> Local {
    let cells = r as f64 / CELL as f64;
    // The sagitta of a chord is about r * pi^2 / (2 n^2); keep it under 0.01.
    let n = (((22.3 * libm::sqrt(cells)) / 8.0).ceil() as usize * 8).max(16);
    let poly = (0..n)
        .map(|k| {
            let angle = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
            Pt::new(
                r + libm::round(r as f64 * libm::sin(angle)) as C,
                r - libm::round(r as f64 * libm::cos(angle)) as C,
            )
        })
        .collect();
    Local {
        poly,
        w: 2 * r,
        h: 2 * r,
        radius: Some(r),
        edge_index: (0..n).collect(),
    }
}

fn polygon(args: &[Value]) -> Result<Local, String> {
    const USAGE: &str = "polygon takes a list of points: polygon([[0, 0], [4, 0], [4, 3]]).";
    // Accept both polygon([p, p, p]) and polygon(p, p, p).
    let items: &[Value] = match args {
        [Value::List(items)] if items.iter().all(|i| matches!(i, Value::List(_))) => items,
        _ => args,
    };
    let mut points = Vec::new();
    for item in items {
        match item {
            Value::List(xy) if xy.len() == 2 => match (number(&xy[0]), number(&xy[1])) {
                (Some(x), Some(y)) => points.push(Pt::new(x, y)),
                _ => return Err(USAGE.to_string()),
            },
            _ => return Err(USAGE.to_string()),
        }
    }
    if points.len() < 3 {
        return Err(format!(
            "a polygon needs at least three vertices, but {} were given.",
            points.len()
        ));
    }
    let n = points.len();
    for i in 0..n {
        if points[i] == points[(i + 1) % n] {
            return Err(format!(
                "polygon vertex {} repeats the one before it at [{}, {}].",
                (i + 1) % n,
                cells(points[i].x),
                cells(points[i].y)
            ));
        }
    }
    let all: Vec<(Pt, Pt)> = edges(&points).collect();
    for i in 0..n {
        // Neighbouring edges may only meet at their shared vertex.
        let (a, b) = all[i];
        let (c, d) = all[(i + 1) % n];
        if on_segment(a, b, d) || on_segment(c, d, a) {
            return Err(format!(
                "the polygon folds back on itself at vertex {}.",
                (i + 1) % n
            ));
        }
        for (j, (c, d)) in all.iter().copied().enumerate().skip(i + 2) {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segments_touch(a, b, c, d) {
                return Err(format!(
                    "the polygon intersects itself: edge {i} meets edge {j}. A polygon must be simple."
                ));
            }
        }
    }
    let area = area2(&points);
    if area == 0 {
        return Err("the polygon has zero area.".to_string());
    }
    let bbox = Rect::of(&points);
    if bbox.w() < CELL || bbox.h() < CELL {
        return Err(format!(
            "the polygon is {} by {} cells; both sides must be at least 1.",
            cells(bbox.w()),
            cells(bbox.h())
        ));
    }
    let mut poly: Vec<Pt> = points
        .iter()
        .map(|p| Pt::new(p.x - bbox.x0, p.y - bbox.y0))
        .collect();
    let mut edge_index: Vec<usize> = (0..n).collect();
    if area < 0 {
        // Reversing the winding renumbers the edges.
        poly.reverse();
        edge_index = (0..n).map(|j| (2 * n - 2 - j) % n).collect();
    }
    Ok(Local {
        poly,
        w: bbox.w(),
        h: bbox.h(),
        radius: None,
        edge_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dedalo_model::Num;

    fn call(func: &str, args: Vec<Value>) -> ShapeCall {
        ShapeCall {
            func: func.to_string(),
            args,
        }
    }

    fn n(v: i64) -> Value {
        Value::Num(Num::int(v))
    }

    fn pts(points: &[[i64; 2]]) -> Value {
        Value::List(
            points
                .iter()
                .map(|p| Value::List(vec![n(p[0]), n(p[1])]))
                .collect(),
        )
    }

    #[test]
    fn rectangles_and_circles() {
        let r = build(&call("rectangle", vec![n(8), n(6)])).unwrap();
        assert_eq!((r.w, r.h, r.poly.len()), (8000, 6000, 4));
        let c = build(&call("circle", vec![n(3)])).unwrap();
        assert_eq!((c.w, c.h, c.radius), (6000, 6000, Some(3000)));
        assert!(area2(&c.poly) > 0);
        let quarter = c.poly.len() / 4;
        assert_eq!(
            (c.poly[0], c.poly[quarter], c.poly[2 * quarter]),
            (Pt::new(3000, 0), Pt::new(6000, 3000), Pt::new(3000, 6000))
        );
        assert!(build(&call("rectangle", vec![n(8), n(0)])).is_err());
        assert!(
            build(&call("hexagon", vec![n(3)]))
                .unwrap_err()
                .contains("not a shape")
        );
    }

    #[test]
    fn polygons_are_normalised_and_validated() {
        // Counter-clockwise input, offset from the origin.
        let p = build(&call(
            "polygon",
            vec![pts(&[[2, 1], [2, 5], [6, 5], [6, 1]])],
        ))
        .unwrap();
        assert!(area2(&p.poly) > 0);
        assert_eq!(
            Rect::of(&p.poly),
            Rect {
                x0: 0,
                y0: 0,
                x1: 4000,
                y1: 4000
            }
        );
        // Edge 0 as written runs from [2,1] to [2,5]: the west side.
        let j = p.edge_index.iter().position(|e| *e == 0).unwrap();
        let (a, b) = (p.poly[j], p.poly[(j + 1) % 4]);
        assert_eq!((a.x, b.x), (0, 0));
        let bow_tie = build(&call(
            "polygon",
            vec![pts(&[[0, 0], [4, 4], [4, 0], [0, 4]])],
        ));
        assert!(bow_tie.unwrap_err().contains("intersects itself"));
        assert!(build(&call("polygon", vec![pts(&[[0, 0], [4, 0]])])).is_err());
    }
}
