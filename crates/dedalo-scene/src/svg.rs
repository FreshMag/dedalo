//! SVG serialisation of a scene.
//!
//! The output is byte-stable: fixed attribute order, fixed number
//! formatting, no metadata, no timestamps. The same scene always gives the
//! same bytes on every platform (spec 15.2).

use std::fmt::Write;

use crate::{Cmd, Node, Paint, Scene, Stroke, TextAnchor};

/// Live text names the bundled font first, so a viewer that has it, and
/// resvg, which is given it, draw it exactly.
const FONT_FAMILY: &str = "Noto Sans, sans-serif";

/// A number with at most three decimals and no trailing zeros.
fn n(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".to_string()
    } else {
        text.to_string()
    }
}

/// A scale factor with six decimals, enough for glyph transforms.
fn n6(value: f64) -> String {
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".to_string()
    } else {
        text.to_string()
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn path_data(d: &[Cmd]) -> String {
    let mut out = String::new();
    for cmd in d {
        match *cmd {
            Cmd::Move(x, y) => write!(out, "M{} {}", n(x), n(y)),
            Cmd::Line(x, y) => write!(out, "L{} {}", n(x), n(y)),
            Cmd::Arc {
                r,
                ry,
                large,
                clockwise,
                x,
                y,
            } => {
                write!(
                    out,
                    "A{} {} 0 {} {} {} {}",
                    n(r),
                    n(ry.unwrap_or(r)),
                    large as u8,
                    clockwise as u8,
                    n(x),
                    n(y)
                )
            }
            Cmd::Cubic(x1, y1, x2, y2, x, y) => write!(
                out,
                "C{} {} {} {} {} {}",
                n(x1),
                n(y1),
                n(x2),
                n(y2),
                n(x),
                n(y)
            ),
            Cmd::Close => write!(out, "Z"),
        }
        .expect("writing to a string");
    }
    out
}

struct Writer {
    live_text: bool,
    /// Glyph definitions already written, by font and glyph.
    glyphs: std::collections::BTreeMap<(bool, u16), String>,
    defs: String,
    body: String,
    next_id: u32,
}

impl Writer {
    /// The ID of a glyph's definition, writing it on first use.
    fn glyph(&mut self, bold: bool, glyph: u16) -> String {
        if let Some(id) = self.glyphs.get(&(bold, glyph)) {
            return id.clone();
        }
        let id = format!("{}{glyph}", if bold { "b" } else { "r" });
        write!(
            self.defs,
            "<path id=\"{id}\" d=\"{}\"/>",
            crate::font::glyph_path(bold, glyph)
        )
        .expect("writing to a string");
        self.glyphs.insert((bold, glyph), id.clone());
        id
    }

    fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    fn fill(&mut self, paint: &Option<Paint>) -> String {
        match paint {
            None => " fill=\"none\"".to_string(),
            Some(Paint::Solid { color, alpha }) if *alpha >= 1.0 => {
                format!(" fill=\"{}\"", escape(color))
            }
            Some(Paint::Solid { color, alpha }) => {
                format!(" fill=\"{}\" fill-opacity=\"{}\"", escape(color), n(*alpha))
            }
            Some(Paint::Radial { cx, cy, r, stops }) => {
                let id = self.id("g");
                write!(self.defs, "<radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\">", n(*cx), n(*cy), n(*r))
                    .expect("writing to a string");
                for (offset, color, alpha) in stops {
                    write!(
                        self.defs,
                        "<stop offset=\"{}\" stop-color=\"{}\" stop-opacity=\"{}\"/>",
                        n(*offset),
                        escape(color),
                        n(*alpha)
                    )
                    .expect("writing to a string");
                }
                self.defs.push_str("</radialGradient>");
                format!(" fill=\"url(#{id})\"")
            }
        }
    }

    fn stroke(stroke: &Option<Stroke>) -> String {
        let Some(s) = stroke else {
            return String::new();
        };
        let mut out = format!(
            " stroke=\"{}\" stroke-width=\"{}\"",
            escape(&s.color),
            n(s.width)
        );
        if s.round {
            out.push_str(" stroke-linecap=\"round\" stroke-linejoin=\"round\"");
        }
        if let Some(dash) = &s.dash {
            let parts: Vec<String> = dash.iter().map(|v| n(*v)).collect();
            write!(out, " stroke-dasharray=\"{}\"", parts.join(" ")).expect("writing to a string");
        }
        out
    }

    fn source(source: &Option<crate::Source>) -> String {
        match source {
            Some(s) => format!(
                " data-id=\"{}\" data-role=\"{}\"",
                escape(&s.id),
                escape(&s.role)
            ),
            None => String::new(),
        }
    }

    fn node(&mut self, node: &Node) {
        match node {
            Node::Group {
                transform,
                clip,
                opacity,
                children,
                source,
            } => {
                let mut attrs = Self::source(source);
                if let Some(t) = transform {
                    let parts: Vec<String> = t.iter().map(|v| n(*v)).collect();
                    write!(attrs, " transform=\"matrix({})\"", parts.join(" "))
                        .expect("writing to a string");
                }
                if let Some(clip) = clip {
                    let id = self.id("c");
                    write!(
                        self.defs,
                        "<clipPath id=\"{id}\"><path d=\"{}\"/></clipPath>",
                        path_data(clip)
                    )
                    .expect("writing to a string");
                    write!(attrs, " clip-path=\"url(#{id})\"").expect("writing to a string");
                }
                if let Some(opacity) = opacity {
                    write!(attrs, " opacity=\"{}\"", n(*opacity)).expect("writing to a string");
                }
                write!(self.body, "<g{attrs}>").expect("writing to a string");
                for child in children {
                    self.node(child);
                }
                self.body.push_str("</g>\n");
            }
            Node::Path {
                d,
                fill,
                stroke,
                source,
            } => {
                let fill = self.fill(fill);
                writeln!(
                    self.body,
                    "<path{} d=\"{}\"{fill}{}/>",
                    Self::source(source),
                    path_data(d),
                    Self::stroke(stroke)
                )
                .expect("writing to a string");
            }
            Node::Text {
                x,
                y,
                text,
                size,
                anchor,
                bold,
                fill,
                halo,
                source,
            } => {
                if !self.live_text {
                    let run = crate::font::layout(text, *x, *y, *size, *anchor, *bold);
                    if run.glyphs.is_empty() {
                        return;
                    }
                    let mut uses = String::new();
                    for (glyph, offset) in &run.glyphs {
                        let id = self.glyph(*bold, *glyph);
                        write!(uses, "<use href=\"#{id}\" x=\"{offset}\"/>")
                            .expect("writing to a string");
                    }
                    // Glyphs are in font units with y up: scale and flip.
                    let transform = format!(
                        "matrix({} 0 0 {} {} {})",
                        n6(run.scale),
                        n6(-run.scale),
                        n(run.x),
                        n(run.baseline)
                    );
                    if let Some((color, width)) = halo {
                        writeln!(
                            self.body,
                            "<g transform=\"{transform}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\">{uses}</g>",
                            escape(color),
                            n(*width / run.scale)
                        )
                        .expect("writing to a string");
                    }
                    writeln!(
                        self.body,
                        "<g{} transform=\"{transform}\" fill=\"{}\">{uses}</g>",
                        Self::source(source),
                        escape(fill)
                    )
                    .expect("writing to a string");
                    return;
                }
                let anchor = match anchor {
                    TextAnchor::Start => "start",
                    TextAnchor::Middle => "middle",
                    TextAnchor::End => "end",
                };
                // `y` is the vertical centre; SVG wants the baseline.
                let common = format!(
                    "x=\"{}\" y=\"{}\" font-family=\"{FONT_FAMILY}\" font-size=\"{}\"{} text-anchor=\"{anchor}\"",
                    n(*x),
                    n(crate::font::baseline(*y, *size, *bold)),
                    n(*size),
                    if *bold { " font-weight=\"bold\"" } else { "" }
                );
                if let Some((color, width)) = halo {
                    writeln!(
                        self.body,
                        "<text {common} fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\">{}</text>",
                        escape(color),
                        n(*width),
                        escape(text)
                    )
                    .expect("writing to a string");
                }
                writeln!(
                    self.body,
                    "<text{} {common} fill=\"{}\">{}</text>",
                    Self::source(source),
                    escape(fill),
                    escape(text)
                )
                .expect("writing to a string");
            }
        }
    }
}

/// Serialises a scene as an SVG document.
pub fn to_svg(scene: &Scene) -> String {
    let mut w = Writer {
        live_text: scene.live_text,
        glyphs: std::collections::BTreeMap::new(),
        defs: String::new(),
        body: String::new(),
        next_id: 0,
    };
    for layer in scene.layers.iter().filter(|l| !l.nodes.is_empty()) {
        writeln!(w.body, "<g id=\"layer-{}\">", escape(&layer.name)).expect("writing to a string");
        for node in &layer.nodes {
            w.node(node);
        }
        w.body.push_str("</g>\n");
    }
    let [x, y, width, height] = scene.bounds;
    let px = scene.cell_px as f64;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">\n",
        n(x),
        n(y),
        n(width),
        n(height),
        n((width * px).round()),
        n((height * px).round())
    );
    if !w.defs.is_empty() {
        writeln!(out, "<defs>{}</defs>", w.defs).expect("writing to a string");
    }
    out.push_str(&w.body);
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Source, shapes};

    #[test]
    fn numbers_are_stable() {
        assert_eq!(n(8.0), "8");
        assert_eq!(n(0.14), "0.14");
        assert_eq!(n(-0.0001), "0");
        assert_eq!(n(1.0 / 3.0), "0.333");
        assert_eq!(n(-2.5), "-2.5");
    }

    #[test]
    fn a_scene_serialises_without_scripts_or_metadata() {
        let mut scene = Scene::new([0.0, 0.0, 8.0, 6.0], 64);
        scene.push(
            "floor",
            Node::fill(shapes::rect(0.0, 0.0, 8.0, 6.0), "#ffffff").with_source(Some(Source {
                id: "cell".into(),
                role: "floor".into(),
            })),
        );
        scene.push(
            "labels",
            Node::text(
                4.0,
                3.0,
                "A <b> & \"c\"",
                0.4,
                TextAnchor::Middle,
                "#111111",
            ),
        );
        // By default text is drawn as outlines.
        assert!(!to_svg(&scene).contains("<text"));
        scene.live_text = true;
        let svg = to_svg(&scene);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 6\" width=\"512\" height=\"384\">\n"));
        assert!(svg.contains(
            "<path data-id=\"cell\" data-role=\"floor\" d=\"M0 0L8 0L8 6L0 6Z\" fill=\"#ffffff\"/>"
        ));
        assert!(svg.contains("A &lt;b&gt; &amp; &quot;c&quot;"));
        assert_eq!(svg, to_svg(&scene));
    }
}
