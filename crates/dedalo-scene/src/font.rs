//! Bundled fonts: text metrics and text as outlines (spec 4 and 9.6).
//!
//! Text is converted to paths by default, so an SVG renders the same on
//! every system and resvg needs no system fonts. The fonts are Noto Sans
//! Regular and Bold under the SIL Open Font License (see `fonts/OFL.txt`),
//! subset to the Latin ranges. Kerning and shaping are not applied, which
//! keeps the layout simple and identical everywhere.

use std::sync::OnceLock;

use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::{Cmd, TextAnchor};

pub const REGULAR: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");
pub const BOLD: &[u8] = include_bytes!("../fonts/NotoSans-Bold.ttf");
/// The family name the fonts declare, for live SVG text.
pub const FAMILY: &str = "Noto Sans";

fn face(bold: bool) -> &'static Face<'static> {
    static REGULAR_FACE: OnceLock<Face<'static>> = OnceLock::new();
    static BOLD_FACE: OnceLock<Face<'static>> = OnceLock::new();
    let (cell, data) = if bold {
        (&BOLD_FACE, BOLD)
    } else {
        (&REGULAR_FACE, REGULAR)
    };
    cell.get_or_init(|| Face::parse(data, 0).expect("the bundled font parses"))
}

fn glyph(face: &Face, c: char) -> GlyphId {
    face.glyph_index(c)
        .or_else(|| face.glyph_index('?'))
        .unwrap_or(GlyphId(0))
}

fn scale(face: &Face, size: f64) -> f64 {
    size / face.units_per_em() as f64
}

/// The advance width of a text, in the units of `size`.
pub fn measure(text: &str, size: f64, bold: bool) -> f64 {
    let f = face(bold);
    let units: u32 = text
        .chars()
        .map(|c| f.glyph_hor_advance(glyph(f, c)).unwrap_or(0) as u32)
        .sum();
    units as f64 * scale(f, size)
}

struct Pen {
    x: f64,
    y: f64,
    k: f64,
    last: (f64, f64),
    out: Vec<Cmd>,
}

impl Pen {
    fn at(&self, x: f32, y: f32) -> (f64, f64) {
        (self.x + x as f64 * self.k, self.y - y as f64 * self.k)
    }
}

impl OutlineBuilder for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.out.push(Cmd::Move(p.0, p.1));
        self.last = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.out.push(Cmd::Line(p.0, p.1));
        self.last = p;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // A quadratic segment as the equivalent cubic.
        let (q, p, s) = (self.at(x1, y1), self.at(x, y), self.last);
        let c1 = (s.0 + 2.0 / 3.0 * (q.0 - s.0), s.1 + 2.0 / 3.0 * (q.1 - s.1));
        let c2 = (p.0 + 2.0 / 3.0 * (q.0 - p.0), p.1 + 2.0 / 3.0 * (q.1 - p.1));
        self.out.push(Cmd::Cubic(c1.0, c1.1, c2.0, c2.1, p.0, p.1));
        self.last = p;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (a, b, p) = (self.at(x1, y1), self.at(x2, y2), self.at(x, y));
        self.out.push(Cmd::Cubic(a.0, a.1, b.0, b.1, p.0, p.1));
        self.last = p;
    }

    fn close(&mut self) {
        self.out.push(Cmd::Close);
    }
}

/// The baseline of a text whose vertical centre is `y`: capital letters are
/// centred on it.
pub fn baseline(y: f64, size: f64, bold: bool) -> f64 {
    let f = face(bold);
    let cap = f
        .capital_height()
        .map_or(0.7 * f.units_per_em() as f64, |h| h as f64);
    y + cap * scale(f, size) / 2.0
}

/// A text as a path, anchored at `x` and vertically centred on `y`.
pub fn outline(text: &str, x: f64, y: f64, size: f64, anchor: TextAnchor, bold: bool) -> Vec<Cmd> {
    let f = face(bold);
    let k = scale(f, size);
    let width = measure(text, size, bold);
    let start = match anchor {
        TextAnchor::Start => x,
        TextAnchor::Middle => x - width / 2.0,
        TextAnchor::End => x - width,
    };
    let mut pen = Pen {
        x: start,
        y: baseline(y, size, bold),
        k,
        last: (0.0, 0.0),
        out: Vec::new(),
    };
    for c in text.chars() {
        let g = glyph(f, c);
        f.outline_glyph(g, &mut pen);
        pen.x += f.glyph_hor_advance(g).unwrap_or(0) as f64 * k;
    }
    pen.out
}

/// A glyph outline as SVG path data in font units, y pointing up.
pub fn glyph_path(bold: bool, glyph: u16) -> String {
    struct Raw(String);
    impl OutlineBuilder for Raw {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push_str(&format!("M{x} {y}"));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push_str(&format!("L{x} {y}"));
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.0.push_str(&format!("Q{x1} {y1} {x} {y}"));
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.0.push_str(&format!("C{x1} {y1} {x2} {y2} {x} {y}"));
        }
        fn close(&mut self) {
            self.0.push('Z');
        }
    }
    let mut raw = Raw(String::new());
    face(bold).outline_glyph(GlyphId(glyph), &mut raw);
    raw.0
}

/// A text laid out as glyphs: the scale from font units to the units of
/// `size`, the origin of the first glyph, and each glyph with its offset
/// along the baseline in font units. Glyphs without an outline are left out.
pub struct Run {
    pub scale: f64,
    pub x: f64,
    pub baseline: f64,
    pub glyphs: Vec<(u16, u32)>,
}

pub fn layout(text: &str, x: f64, y: f64, size: f64, anchor: TextAnchor, bold: bool) -> Run {
    let f = face(bold);
    let width = measure(text, size, bold);
    let start = match anchor {
        TextAnchor::Start => x,
        TextAnchor::Middle => x - width / 2.0,
        TextAnchor::End => x - width,
    };
    let mut glyphs = Vec::new();
    let mut pen = 0u32;
    for c in text.chars() {
        let g = glyph(f, c);
        if f.glyph_bounding_box(g).is_some() {
            glyphs.push((g.0, pen));
        }
        pen += f.glyph_hor_advance(g).unwrap_or(0) as u32;
    }
    Run {
        scale: scale(f, size),
        x: start,
        baseline: baseline(y, size, bold),
        glyphs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_and_outlines() {
        let w = measure("Legend", 1.0, false);
        assert!(w > 2.5 && w < 4.0, "{w}");
        assert!(measure("Legend", 1.0, true) > w);
        let d = outline("A", 0.0, 0.0, 1.0, TextAnchor::Start, false);
        assert!(matches!(d[0], Cmd::Move(..)) && d.contains(&Cmd::Close));
        assert!(outline(" ", 0.0, 0.0, 1.0, TextAnchor::Start, false).is_empty());
        // A character the fonts lack draws as a question mark, not as nothing.
        assert!(!outline("\u{4e2d}", 0.0, 0.0, 1.0, TextAnchor::Start, false).is_empty());
    }
}
