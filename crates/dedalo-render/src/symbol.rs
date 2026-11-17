//! SVG symbol files for themes (spec 9.4), and their sanitiser (spec 11.7).
//!
//! A theme may map a type to an SVG file. The file is untrusted: it passes
//! an allow-list that keeps plain geometry and nothing else. Scripts, event
//! attributes, styles, external references, images, text and foreign
//! objects are dropped. What is left is converted to scene nodes, so a
//! symbol can express no more than the scene can.

use dedalo_scene::shapes::{circle, ellipse, line, polygon, rect};
use dedalo_scene::{Cmd, Node, Paint, Stroke};

/// Largest symbol file, in bytes.
pub const MAX_BYTES: usize = 64 * 1024;
const MAX_ELEMENTS: usize = 500;
const MAX_DEPTH: usize = 16;
const MAX_COMMANDS: usize = 5000;

/// The colour that stands for the theme's ink.
pub const CURRENT: &str = "currentColor";

const SHAPES: &[&str] = &[
    "path", "rect", "circle", "ellipse", "line", "polyline", "polygon",
];
/// Presentation attributes kept on every element.
const PAINT_ATTRS: &[&str] = &[
    "fill",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-dasharray",
    "opacity",
    "fill-opacity",
    "transform",
];

/// The geometry attributes kept on each element.
fn geometry_attrs(name: &str) -> &'static [&'static str] {
    match name {
        "svg" => &["viewBox", "width", "height", "xmlns"],
        "path" => &["d"],
        "rect" => &["x", "y", "width", "height"],
        "circle" => &["cx", "cy", "r"],
        "ellipse" => &["cx", "cy", "rx", "ry"],
        "line" => &["x1", "y1", "x2", "y2"],
        "polyline" | "polygon" => &["points"],
        _ => &[],
    }
}

/// An element that passed the allow-list.
#[derive(Clone, Debug, PartialEq)]
struct El {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<El>,
}

impl El {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn num(&self, name: &str) -> f64 {
        self.attr(name)
            .and_then(|v| v.trim().trim_end_matches("px").parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    }
}

// ----- reading ------------------------------------------------------------------

struct Reader<'a> {
    text: &'a str,
    at: usize,
    elements: usize,
}

fn decode(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')
}

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn skip_past(&mut self, end: &str) -> Result<(), String> {
        match self.rest().find(end) {
            Some(i) => {
                self.at += i + end.len();
                Ok(())
            }
            None => Err(format!("`{end}` is missing: the file is cut short")),
        }
    }

    fn skip_space(&mut self) {
        let trimmed = self.rest().trim_start();
        self.at = self.text.len() - trimmed.len();
    }

    fn name(&mut self) -> &'a str {
        let rest = self.rest();
        let end = rest.find(|c| !name_char(c)).unwrap_or(rest.len());
        self.at += end;
        &rest[..end]
    }

    /// Skips text, comments, declarations and processing instructions up
    /// to the next tag.
    fn skip_misc(&mut self) -> Result<(), String> {
        loop {
            match self.rest().find('<') {
                None => {
                    self.at = self.text.len();
                    return Ok(());
                }
                Some(i) => self.at += i,
            }
            let rest = self.rest();
            if rest.starts_with("<!--") {
                self.skip_past("-->")?;
            } else if rest.starts_with("<![CDATA[") {
                self.skip_past("]]>")?;
            } else if rest.starts_with("<?") {
                self.skip_past("?>")?;
            } else if rest.starts_with("<!") {
                // A document type, possibly with an internal subset.
                let close = rest.find('>').ok_or("the document type is cut short")?;
                match rest.find('[').filter(|open| *open < close) {
                    Some(_) => self.skip_past("]>")?,
                    None => self.at += close + 1,
                }
            } else {
                return Ok(());
            }
        }
    }

    /// Reads the element at the cursor, which is at its `<`. Returns
    /// nothing for an element the allow-list drops.
    fn element(&mut self, depth: usize) -> Result<Option<El>, String> {
        if depth > MAX_DEPTH {
            return Err(format!("elements nest more than {MAX_DEPTH} deep"));
        }
        self.elements += 1;
        if self.elements > MAX_ELEMENTS {
            return Err(format!("more than {MAX_ELEMENTS} elements"));
        }
        self.at += 1;
        let name = self.name();
        if name.is_empty() {
            return Err("a tag has no name".to_string());
        }
        let mut attrs: Vec<(String, String)> = Vec::new();
        let self_closing = loop {
            self.skip_space();
            let rest = self.rest();
            if rest.starts_with("/>") {
                self.at += 2;
                break true;
            }
            if rest.starts_with('>') {
                self.at += 1;
                break false;
            }
            let key = self.name();
            if key.is_empty() {
                return Err(format!("the tag `{name}` is malformed"));
            }
            self.skip_space();
            if !self.rest().starts_with('=') {
                return Err(format!("the attribute `{key}` has no value"));
            }
            self.at += 1;
            self.skip_space();
            let quote = self
                .rest()
                .chars()
                .next()
                .filter(|c| matches!(c, '"' | '\''));
            let Some(quote) = quote else {
                return Err(format!("the value of `{key}` is not quoted"));
            };
            self.at += 1;
            let rest = self.rest();
            let end = rest
                .find(quote)
                .ok_or_else(|| format!("the value of `{key}` is not closed"))?;
            attrs.push((key.to_string(), decode(&rest[..end])));
            self.at += end + 1;
        };
        let mut children = Vec::new();
        if !self_closing {
            loop {
                self.skip_misc()?;
                let rest = self.rest();
                if rest.is_empty() {
                    return Err(format!("`{name}` is never closed"));
                }
                if rest.starts_with("</") {
                    self.at += 2;
                    let closing = self.name();
                    if closing != name {
                        return Err(format!("`{name}` is closed by `</{closing}>`"));
                    }
                    self.skip_space();
                    if !self.rest().starts_with('>') {
                        return Err(format!("`</{name}` is malformed"));
                    }
                    self.at += 1;
                    break;
                }
                children.extend(self.element(depth + 1)?);
            }
        }
        // The allow-list. A dropped element takes its subtree with it.
        let container = matches!(name, "svg" | "g");
        if !container && !SHAPES.contains(&name) {
            return Ok(None);
        }
        let geometry = geometry_attrs(name);
        attrs.retain(|(k, v)| {
            (geometry.contains(&k.as_str()) || PAINT_ATTRS.contains(&k.as_str()))
                && safe_value(k, v)
        });
        attrs.dedup_by(|b, a| a.0 == b.0);
        if !container {
            children.clear();
        }
        Ok(Some(El {
            name: name.to_string(),
            attrs,
            children,
        }))
    }
}

/// Whether an attribute value is plain data: a colour, a number list or
/// path data. Anything that could point outside the file is refused.
fn safe_value(key: &str, value: &str) -> bool {
    let v = value.trim();
    match key {
        "xmlns" => v == "http://www.w3.org/2000/svg",
        "fill" | "stroke" => colour(v).is_some(),
        "stroke-linecap" => matches!(v, "butt" | "round" | "square"),
        _ => {
            v.len() <= 32 * 1024
                && v.chars().all(|c| {
                    c.is_ascii_alphanumeric()
                        || matches!(
                            c,
                            ' ' | ',' | '.' | '-' | '+' | '(' | ')' | '\n' | '\t' | '\r'
                        )
                })
                && !v.to_ascii_lowercase().contains("url")
        }
    }
}

/// A colour the scene can paint: `none`, the theme's ink, or a hex colour
/// in six digits.
fn colour(value: &str) -> Option<Option<String>> {
    let v = value.trim();
    match v.to_ascii_lowercase().as_str() {
        "none" | "transparent" => return Some(None),
        "currentcolor" => return Some(Some(CURRENT.to_string())),
        "black" => return Some(Some("#000000".to_string())),
        "white" => return Some(Some("#ffffff".to_string())),
        _ => {}
    }
    let hex = v.strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        3 => Some(Some(format!(
            "#{}",
            hex.chars()
                .flat_map(|c| [c, c])
                .collect::<String>()
                .to_ascii_lowercase()
        ))),
        6 => Some(Some(format!("#{}", hex.to_ascii_lowercase()))),
        _ => None,
    }
}

fn read(svg: &str) -> Result<El, String> {
    if svg.len() > MAX_BYTES {
        return Err(format!(
            "the file is {} bytes; a symbol is at most {MAX_BYTES}",
            svg.len()
        ));
    }
    let mut reader = Reader {
        text: svg,
        at: 0,
        elements: 0,
    };
    reader.skip_misc()?;
    if reader.rest().is_empty() {
        return Err("the file holds no `svg` element".to_string());
    }
    match reader.element(0)? {
        Some(root) if root.name == "svg" => Ok(root),
        _ => Err("the root element is not `svg`".to_string()),
    }
}

// ----- writing ------------------------------------------------------------------

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

fn write(el: &El, out: &mut String) {
    out.push('<');
    out.push_str(&el.name);
    for (k, v) in &el.attrs {
        out.push_str(&format!(" {k}=\"{}\"", escape(v)));
    }
    if el.children.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for child in &el.children {
        write(child, out);
    }
    out.push_str(&format!("</{}>", el.name));
}

/// The file reduced to what the allow-list keeps: `svg`, `g` and the basic
/// shapes, with geometry and plain colours.
pub fn sanitise(svg: &str) -> Result<String, String> {
    let mut root = read(svg)?;
    if root.attr("xmlns").is_none() {
        root.attrs.insert(
            0,
            (
                "xmlns".to_string(),
                "http://www.w3.org/2000/svg".to_string(),
            ),
        );
    }
    let mut out = String::new();
    write(&root, &mut out);
    out.push('\n');
    Ok(out)
}

// ----- conversion to scene nodes ----------------------------------------------------

/// A symbol ready to draw: nodes in the units of its own view box.
#[derive(Clone, Debug, PartialEq)]
pub struct Art {
    /// `[x, y, width, height]`.
    pub view_box: [f64; 4],
    pub nodes: Vec<Node>,
}

fn numbers(text: &str) -> Vec<f64> {
    text.split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .collect()
}

struct PathReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl PathReader<'_> {
    fn skip(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b',')
        {
            self.at += 1;
        }
    }

    fn command(&mut self) -> Option<u8> {
        self.skip();
        let b = *self.bytes.get(self.at)?;
        b.is_ascii_alphabetic().then(|| {
            self.at += 1;
            b
        })
    }

    fn number(&mut self) -> Option<f64> {
        self.skip();
        let start = self.at;
        let peek = |at: usize| self.bytes.get(at).copied();
        let mut at = start;
        if matches!(peek(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let mut dot = false;
        while let Some(b) = peek(at) {
            if b.is_ascii_digit() || (b == b'.' && !dot) {
                dot |= b == b'.';
                at += 1;
            } else {
                break;
            }
        }
        if matches!(peek(at), Some(b'e' | b'E')) {
            let mut exp = at + 1;
            if matches!(peek(exp), Some(b'+' | b'-')) {
                exp += 1;
            }
            if peek(exp).is_some_and(|b| b.is_ascii_digit()) {
                while peek(exp).is_some_and(|b| b.is_ascii_digit()) {
                    exp += 1;
                }
                at = exp;
            }
        }
        let value = std::str::from_utf8(&self.bytes[start..at])
            .ok()?
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())?;
        self.at = at;
        Some(value)
    }

    /// An arc flag: a single `0` or `1`, which may touch the next number.
    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let b = *self.bytes.get(self.at)?;
        matches!(b, b'0' | b'1').then(|| {
            self.at += 1;
            b == b'1'
        })
    }
}

/// SVG path data as scene commands, in absolute coordinates.
fn path_data(d: &str) -> Result<Vec<Cmd>, String> {
    let mut r = PathReader {
        bytes: d.as_bytes(),
        at: 0,
    };
    let mut out: Vec<Cmd> = Vec::new();
    let (mut x, mut y, mut sx, mut sy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    // The last control point, for the smooth commands.
    let mut control: Option<(f64, f64, bool)> = None;
    let mut cmd = r.command().ok_or("path data starts with no command")?;
    loop {
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        let mut next_control = None;
        let bad = || format!("the path command `{}` lacks numbers", cmd as char);
        match cmd.to_ascii_uppercase() {
            b'M' => {
                (x, y) = (
                    ox + r.number().ok_or_else(bad)?,
                    oy + r.number().ok_or_else(bad)?,
                );
                (sx, sy) = (x, y);
                out.push(Cmd::Move(x, y));
            }
            b'L' => {
                (x, y) = (
                    ox + r.number().ok_or_else(bad)?,
                    oy + r.number().ok_or_else(bad)?,
                );
                out.push(Cmd::Line(x, y));
            }
            b'H' => {
                x = ox + r.number().ok_or_else(bad)?;
                out.push(Cmd::Line(x, y));
            }
            b'V' => {
                y = oy + r.number().ok_or_else(bad)?;
                out.push(Cmd::Line(x, y));
            }
            b'C' | b'S' => {
                let (x1, y1) = if cmd.eq_ignore_ascii_case(&b'C') {
                    (
                        ox + r.number().ok_or_else(bad)?,
                        oy + r.number().ok_or_else(bad)?,
                    )
                } else {
                    match control {
                        Some((cx, cy, true)) => (2.0 * x - cx, 2.0 * y - cy),
                        _ => (x, y),
                    }
                };
                let (x2, y2) = (
                    ox + r.number().ok_or_else(bad)?,
                    oy + r.number().ok_or_else(bad)?,
                );
                (x, y) = (
                    ox + r.number().ok_or_else(bad)?,
                    oy + r.number().ok_or_else(bad)?,
                );
                out.push(Cmd::Cubic(x1, y1, x2, y2, x, y));
                next_control = Some((x2, y2, true));
            }
            b'Q' | b'T' => {
                let (qx, qy) = if cmd.eq_ignore_ascii_case(&b'Q') {
                    (
                        ox + r.number().ok_or_else(bad)?,
                        oy + r.number().ok_or_else(bad)?,
                    )
                } else {
                    match control {
                        Some((cx, cy, false)) => (2.0 * x - cx, 2.0 * y - cy),
                        _ => (x, y),
                    }
                };
                let (x0, y0) = (x, y);
                (x, y) = (
                    ox + r.number().ok_or_else(bad)?,
                    oy + r.number().ok_or_else(bad)?,
                );
                // A quadratic curve as the cubic that draws the same line.
                out.push(Cmd::Cubic(
                    x0 + 2.0 / 3.0 * (qx - x0),
                    y0 + 2.0 / 3.0 * (qy - y0),
                    x + 2.0 / 3.0 * (qx - x),
                    y + 2.0 / 3.0 * (qy - y),
                    x,
                    y,
                ));
                next_control = Some((qx, qy, false));
            }
            b'A' => {
                let (rx, ry) = (
                    r.number().ok_or_else(bad)?.abs(),
                    r.number().ok_or_else(bad)?.abs(),
                );
                // The scene's arcs are not tilted; the angle is read and
                // dropped.
                r.number().ok_or_else(bad)?;
                let (large, clockwise) = (r.flag().ok_or_else(bad)?, r.flag().ok_or_else(bad)?);
                (x, y) = (
                    ox + r.number().ok_or_else(bad)?,
                    oy + r.number().ok_or_else(bad)?,
                );
                if rx == 0.0 || ry == 0.0 {
                    out.push(Cmd::Line(x, y));
                } else {
                    out.push(Cmd::Arc {
                        r: rx,
                        ry: (rx != ry).then_some(ry),
                        large,
                        clockwise,
                        x,
                        y,
                    });
                }
            }
            b'Z' => {
                (x, y) = (sx, sy);
                out.push(Cmd::Close);
            }
            other => return Err(format!("`{}` is not a path command", other as char)),
        }
        control = next_control;
        if out.len() > MAX_COMMANDS {
            return Err(format!("a path has more than {MAX_COMMANDS} commands"));
        }
        cmd = match r.command() {
            Some(next) => next,
            None => {
                r.skip();
                if r.at >= r.bytes.len() {
                    break;
                }
                // More numbers repeat the command; after a move they draw lines.
                match cmd {
                    b'M' => b'L',
                    b'm' => b'l',
                    b'Z' | b'z' => return Err("numbers follow a closed path".to_string()),
                    same => same,
                }
            }
        };
    }
    Ok(out)
}

type Matrix = [f64; 6];

fn multiply(m: Matrix, n: Matrix) -> Matrix {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

/// An SVG `transform` list as one matrix.
fn transform(text: &str) -> Result<Matrix, String> {
    let mut m: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut rest = text.trim();
    while !rest.is_empty() {
        let open = rest.find('(').ok_or("a transform has no `(`")?;
        let close = rest.find(')').ok_or("a transform has no `)`")?;
        if close < open {
            return Err("a transform is malformed".to_string());
        }
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let a = numbers(&rest[open + 1..close]);
        let step: Matrix = match (name, a.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => [*a, *b, *c, *d, *e, *f],
            ("translate", [tx]) => [1.0, 0.0, 0.0, 1.0, *tx, 0.0],
            ("translate", [tx, ty]) => [1.0, 0.0, 0.0, 1.0, *tx, *ty],
            ("scale", [s]) => [*s, 0.0, 0.0, *s, 0.0, 0.0],
            ("scale", [sx, sy]) => [*sx, 0.0, 0.0, *sy, 0.0, 0.0],
            ("rotate", [deg]) => rotation(*deg, 0.0, 0.0),
            ("rotate", [deg, cx, cy]) => rotation(*deg, *cx, *cy),
            _ => return Err(format!("the transform `{name}` is not supported")),
        };
        m = multiply(m, step);
        rest = rest[close + 1..].trim();
    }
    Ok(m)
}

/// A rotation of `degrees` clockwise about a point.
pub(crate) fn rotation(degrees: f64, cx: f64, cy: f64) -> Matrix {
    let a = degrees.to_radians();
    let (sin, cos) = (libm::sin(a), libm::cos(a));
    [
        cos,
        sin,
        -sin,
        cos,
        cx - cos * cx + sin * cy,
        cy - sin * cx - cos * cy,
    ]
}

/// The paint an element inherits and may override.
#[derive(Clone)]
struct Style {
    fill: Option<String>,
    stroke: Option<String>,
    width: f64,
    round: bool,
    dash: Option<Vec<f64>>,
    fill_alpha: f64,
}

fn convert(el: &El, inherited: &Style) -> Result<Option<Node>, String> {
    let mut style = inherited.clone();
    if let Some(fill) = el.attr("fill").and_then(colour) {
        style.fill = fill;
    }
    if let Some(stroke) = el.attr("stroke").and_then(colour) {
        style.stroke = stroke;
    }
    if el.attr("stroke-width").is_some() {
        style.width = el.num("stroke-width").max(0.0);
    }
    if let Some(cap) = el.attr("stroke-linecap") {
        style.round = cap == "round";
    }
    if let Some(dash) = el.attr("stroke-dasharray") {
        let dash = numbers(dash);
        style.dash = (dash.len() >= 2 && dash.iter().all(|d| *d > 0.0)).then_some(dash);
    }
    if el.attr("fill-opacity").is_some() {
        style.fill_alpha = el.num("fill-opacity").clamp(0.0, 1.0);
    }
    let opacity = el
        .attr("opacity")
        .map(|_| el.num("opacity").clamp(0.0, 1.0));
    let matrix = el.attr("transform").map(transform).transpose()?;

    let d = match el.name.as_str() {
        "svg" | "g" => {
            let children = el
                .children
                .iter()
                .map(|c| convert(c, &style))
                .collect::<Result<Vec<_>, _>>()?;
            let children: Vec<Node> = children.into_iter().flatten().collect();
            return Ok((!children.is_empty()).then_some(Node::Group {
                transform: matrix,
                clip: None,
                opacity,
                children,
                source: None,
            }));
        }
        "path" => path_data(el.attr("d").unwrap_or_default())?,
        "rect" => rect(el.num("x"), el.num("y"), el.num("width"), el.num("height")),
        "circle" => circle(el.num("cx"), el.num("cy"), el.num("r")),
        "ellipse" => ellipse(el.num("cx"), el.num("cy"), el.num("rx"), el.num("ry")),
        "line" => line((el.num("x1"), el.num("y1")), (el.num("x2"), el.num("y2"))),
        _ => {
            let n = numbers(el.attr("points").unwrap_or_default());
            let points: Vec<(f64, f64)> =
                n.as_chunks::<2>().0.iter().map(|p| (p[0], p[1])).collect();
            if points.len() < 2 {
                return Ok(None);
            }
            let mut d = polygon(&points);
            if el.name == "polyline" {
                d.retain(|c| *c != Cmd::Close);
            }
            d
        }
    };
    if d.is_empty() {
        return Ok(None);
    }
    // A line has no inside to fill.
    let fill = style
        .fill
        .as_ref()
        .filter(|_| el.name != "line")
        .map(|color| Paint::Solid {
            color: color.clone(),
            alpha: style.fill_alpha,
        });
    let stroke = style
        .stroke
        .as_ref()
        .filter(|_| style.width > 0.0)
        .map(|color| Stroke {
            color: color.clone(),
            width: style.width,
            dash: style.dash.clone(),
            round: style.round,
        });
    if fill.is_none() && stroke.is_none() {
        return Ok(None);
    }
    let path = Node::Path {
        d,
        fill,
        stroke,
        source: None,
    };
    Ok(Some(if matrix.is_some() || opacity.is_some() {
        Node::Group {
            transform: matrix,
            clip: None,
            opacity,
            children: vec![path],
            source: None,
        }
    } else {
        path
    }))
}

/// Reads a symbol file: sanitises it and converts what is left.
pub fn parse(svg: &str) -> Result<Art, String> {
    let root = read(svg)?;
    let view_box = match root.attr("viewBox").map(numbers).as_deref() {
        Some([x, y, w, h]) => [*x, *y, *w, *h],
        Some(_) => return Err("`viewBox` needs four numbers".to_string()),
        None => [0.0, 0.0, root.num("width"), root.num("height")],
    };
    if view_box[2] <= 0.0 || view_box[3] <= 0.0 {
        return Err("the `svg` element needs a `viewBox` with a positive size".to_string());
    }
    // As in SVG, shapes are filled unless they say otherwise; the default
    // colour is the theme's ink.
    let style = Style {
        fill: Some(CURRENT.to_string()),
        stroke: None,
        width: 1.0,
        round: false,
        dash: None,
        fill_alpha: 1.0,
    };
    let nodes: Vec<Node> = convert(&root, &style)?.into_iter().collect();
    if nodes.is_empty() {
        return Err("nothing drawable is left after sanitising".to_string());
    }
    Ok(Art { view_box, nodes })
}

fn tint(node: &mut Node, ink: &str) {
    match node {
        Node::Group { children, .. } => children.iter_mut().for_each(|c| tint(c, ink)),
        Node::Path { fill, stroke, .. } => {
            if let Some(Paint::Solid { color, .. }) = fill
                && color == CURRENT
            {
                *color = ink.to_string();
            }
            if let Some(stroke) = stroke
                && stroke.color == CURRENT
            {
                stroke.color = ink.to_string();
            }
        }
        Node::Text { .. } => {}
    }
}

impl Art {
    /// The symbol fitted into a frame given in cells, centred, turned by
    /// `degrees` clockwise, with the theme's ink where it asks for it.
    pub fn place(&self, (x, y, w, h): (f64, f64, f64, f64), degrees: f64, ink: &str) -> Node {
        let [vx, vy, vw, vh] = self.view_box;
        let scale = (w / vw).min(h / vh);
        let (tx, ty) = (
            x + (w - vw * scale) / 2.0 - vx * scale,
            y + (h - vh * scale) / 2.0 - vy * scale,
        );
        let mut matrix: Matrix = [scale, 0.0, 0.0, scale, tx, ty];
        if degrees != 0.0 {
            matrix = multiply(rotation(degrees, x + w / 2.0, y + h / 2.0), matrix);
        }
        let mut children = self.nodes.clone();
        children.iter_mut().for_each(|c| tint(c, ink));
        Node::Group {
            transform: Some(matrix),
            clip: None,
            opacity: None,
            children,
            source: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTILE: &str = r##"<?xml version="1.0"?>
<!DOCTYPE svg [ <!ENTITY xxe SYSTEM "file:///etc/passwd"> ]>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
     viewBox="0 0 10 10" onload="alert(1)">
  <script>alert(1)</script>
  <style>path { fill: url(http://example.com/x) }</style>
  <foreignObject><body xmlns="http://www.w3.org/1999/xhtml"><iframe src="http://x"/></body></foreignObject>
  <image xlink:href="http://example.com/track.png" width="1" height="1"/>
  <use href="http://example.com/other.svg#a"/>
  <a href="javascript:alert(1)"><circle cx="1" cy="1" r="1"/></a>
  <text>&xxe;</text>
  <g onclick="steal()" fill="url(#leak)">
    <path d="M1 1 L9 1 L9 9 Z" onmouseover="x()" fill="#ABC" style="fill:red"/>
    <rect x="2" y="2" width="3" height="3" fill="javascript:alert(1)"/>
  </g>
</svg>"##;

    #[test]
    fn the_sanitiser_keeps_geometry_and_nothing_else() {
        let clean = sanitise(HOSTILE).unwrap();
        assert_eq!(
            clean,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><g><path d=\"M1 1 L9 1 L9 9 Z\" fill=\"#ABC\"/><rect x=\"2\" y=\"2\" width=\"3\" height=\"3\"/></g></svg>\n"
        );
        for word in [
            "script",
            "onload",
            "onclick",
            "href",
            "foreign",
            "style",
            "url",
            "image",
            "xxe",
            "javascript",
            "iframe",
        ] {
            assert!(!clean.contains(word), "`{word}` survived");
        }
        // Sanitising again changes nothing.
        assert_eq!(sanitise(&clean).unwrap(), clean);
    }

    #[test]
    fn symbols_become_scene_nodes() {
        let art = parse(HOSTILE).unwrap();
        assert_eq!(art.view_box, [0.0, 0.0, 10.0, 10.0]);
        let placed = art.place((2.0, 3.0, 1.0, 1.0), 0.0, "#111111");
        let text = serde_json::to_string(&placed).unwrap();
        assert!(text.contains("#aabbcc"), "{text}");
        // The rectangle lost its hostile fill and takes the ink.
        assert!(text.contains("#111111"), "{text}");
        assert!(!text.contains(CURRENT));
    }

    #[test]
    fn path_data_is_read_in_every_spelling() {
        let d = path_data("M10-5l5,5h-5v5zm1 1 2 2q1 0 1 1t1 1a2 2 0 011 1").unwrap();
        assert_eq!(d[0], Cmd::Move(10.0, -5.0));
        assert_eq!(d[1], Cmd::Line(15.0, 0.0));
        assert_eq!(d[2], Cmd::Line(10.0, 0.0));
        assert_eq!(d[3], Cmd::Line(10.0, 5.0));
        assert_eq!(d[4], Cmd::Close);
        assert_eq!(d[5], Cmd::Move(11.0, -4.0));
        assert_eq!(d[6], Cmd::Line(13.0, -2.0));
        assert!(matches!(
            d[9],
            Cmd::Arc {
                large: false,
                clockwise: true,
                ..
            }
        ));
        assert!(path_data("M1").is_err());
        assert!(path_data("X1 1").is_err());
    }

    #[test]
    fn broken_and_oversized_files_are_refused() {
        for bad in [
            "",
            "<svg",
            "<svg viewBox='0 0 1 1'><g></svg>",
            "<html></html>",
            "<svg viewBox='0 0 1 1'></svg>",
            "<svg><rect width='1' height='1'/></svg>",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        let deep = format!(
            "<svg viewBox='0 0 1 1'>{}<rect width='1' height='1'/>{}</svg>",
            "<g>".repeat(40),
            "</g>".repeat(40)
        );
        assert!(parse(&deep).unwrap_err().contains("deep"));
        let big = format!(
            "<svg viewBox='0 0 1 1'><!--{}--></svg>",
            "x".repeat(MAX_BYTES)
        );
        assert!(parse(&big).unwrap_err().contains("bytes"));
    }
}
