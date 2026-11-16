//! The scene: the only rendering contract (spec 9.3).
//!
//! A scene is a restricted vector document, close to a subset of SVG. Units
//! are cells; pixels appear only when the scene is serialised. By design it
//! cannot express scripts, filters, external references, foreign objects or
//! animation, so whatever it holds renders the same everywhere.

mod svg;

use serde::Serialize;

pub use svg::to_svg;

/// The layers of a scene, bottom to top.
pub const LAYERS: [&str; 12] = [
    "background",
    "floor",
    "terrain",
    "lighting",
    "grid",
    "walls",
    "openings",
    "features",
    "entities",
    "markers",
    "labels",
    "frame",
];

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Scene {
    /// `[x, y, width, height]` of the canvas, in cells.
    pub bounds: [f64; 4],
    /// Pixels per cell at emit time.
    pub cell_px: u32,
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Layer {
    pub name: String,
    pub nodes: Vec<Node>,
}

/// The element a node was drawn for. The editor uses it for click-to-source.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Source {
    pub id: String,
    pub role: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum Node {
    Group {
        /// An affine transform `[a, b, c, d, e, f]`.
        #[serde(skip_serializing_if = "Option::is_none")]
        transform: Option<[f64; 6]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        clip: Option<Vec<Cmd>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
        children: Vec<Node>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<Source>,
    },
    Path {
        d: Vec<Cmd>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fill: Option<Paint>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stroke: Option<Stroke>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<Source>,
    },
    Text {
        /// The anchor point; `y` is the vertical centre of the text.
        x: f64,
        y: f64,
        text: String,
        /// Font size in cells.
        size: f64,
        anchor: TextAnchor,
        bold: bool,
        fill: String,
        /// An outline drawn behind the text: colour and width in cells.
        #[serde(skip_serializing_if = "Option::is_none")]
        halo: Option<(String, f64)>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<Source>,
    },
}

/// A path command, in absolute coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmd {
    Move(f64, f64),
    Line(f64, f64),
    /// A circular arc to `(x, y)`.
    Arc {
        r: f64,
        large: bool,
        clockwise: bool,
        x: f64,
        y: f64,
    },
    Cubic(f64, f64, f64, f64, f64, f64),
    Close,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Paint {
    /// An sRGB colour such as `#a8d0f0`, with alpha.
    Solid { color: String, alpha: f64 },
    /// A radial gradient with `(offset, colour, alpha)` stops.
    Radial {
        cx: f64,
        cy: f64,
        r: f64,
        stops: Vec<(f64, String, f64)>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stroke {
    pub color: String,
    /// Width in cells.
    pub width: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dash: Option<Vec<f64>>,
    pub round: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

impl Scene {
    /// An empty scene with every layer present.
    pub fn new(bounds: [f64; 4], cell_px: u32) -> Scene {
        Scene {
            bounds,
            cell_px,
            layers: LAYERS
                .iter()
                .map(|name| Layer {
                    name: name.to_string(),
                    nodes: Vec::new(),
                })
                .collect(),
        }
    }

    /// Adds a node to a layer. Unknown layer names go to `markers`.
    pub fn push(&mut self, layer: &str, node: Node) {
        let index = LAYERS.iter().position(|l| *l == layer).unwrap_or(9);
        self.layers[index].nodes.push(node);
    }
}

impl Paint {
    pub fn solid(color: &str) -> Paint {
        Paint::Solid {
            color: color.to_string(),
            alpha: 1.0,
        }
    }
}

impl Stroke {
    pub fn new(color: &str, width: f64) -> Stroke {
        Stroke {
            color: color.to_string(),
            width,
            dash: None,
            round: false,
        }
    }

    pub fn round(mut self) -> Stroke {
        self.round = true;
        self
    }

    pub fn dashed(mut self, on: f64, off: f64) -> Stroke {
        self.dash = Some(vec![on, off]);
        self
    }
}

impl Node {
    pub fn path(d: Vec<Cmd>, fill: Option<Paint>, stroke: Option<Stroke>) -> Node {
        Node::Path {
            d,
            fill,
            stroke,
            source: None,
        }
    }

    pub fn fill(d: Vec<Cmd>, color: &str) -> Node {
        Node::path(d, Some(Paint::solid(color)), None)
    }

    pub fn stroke(d: Vec<Cmd>, stroke: Stroke) -> Node {
        Node::path(d, None, Some(stroke))
    }

    pub fn text(x: f64, y: f64, text: &str, size: f64, anchor: TextAnchor, fill: &str) -> Node {
        Node::Text {
            x,
            y,
            text: text.to_string(),
            size,
            anchor,
            bold: false,
            fill: fill.to_string(),
            halo: None,
            source: None,
        }
    }

    pub fn group(children: Vec<Node>) -> Node {
        Node::Group {
            transform: None,
            clip: None,
            opacity: None,
            children,
            source: None,
        }
    }

    /// Attaches a source annotation.
    pub fn with_source(mut self, src: Option<Source>) -> Node {
        match &mut self {
            Node::Group { source, .. } | Node::Path { source, .. } | Node::Text { source, .. } => {
                *source = src
            }
        }
        self
    }

    pub fn bold(mut self) -> Node {
        if let Node::Text { bold, .. } = &mut self {
            *bold = true;
        }
        self
    }

    pub fn halo(mut self, color: &str, width: f64) -> Node {
        if let Node::Text { halo, .. } = &mut self {
            *halo = Some((color.to_string(), width));
        }
        self
    }
}

/// Path builders.
pub mod shapes {
    use super::Cmd;

    pub fn polygon(points: &[(f64, f64)]) -> Vec<Cmd> {
        let mut d: Vec<Cmd> = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    Cmd::Move(p.0, p.1)
                } else {
                    Cmd::Line(p.0, p.1)
                }
            })
            .collect();
        d.push(Cmd::Close);
        d
    }

    pub fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<Cmd> {
        polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
    }

    pub fn line(a: (f64, f64), b: (f64, f64)) -> Vec<Cmd> {
        vec![Cmd::Move(a.0, a.1), Cmd::Line(b.0, b.1)]
    }

    pub fn circle(cx: f64, cy: f64, r: f64) -> Vec<Cmd> {
        vec![
            Cmd::Move(cx - r, cy),
            Cmd::Arc {
                r,
                large: false,
                clockwise: true,
                x: cx + r,
                y: cy,
            },
            Cmd::Arc {
                r,
                large: false,
                clockwise: true,
                x: cx - r,
                y: cy,
            },
            Cmd::Close,
        ]
    }
}
