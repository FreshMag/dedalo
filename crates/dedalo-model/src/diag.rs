//! Diagnostics: one record per problem, with a stable code (spec section 10).

use std::collections::BTreeMap;

use dedalo_syntax::TextRange;
use dedalo_syntax::text::LineIndex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ops::Op;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

macro_rules! codes {
    ($($variant:ident $text:literal $severity:ident $stage:literal $summary:literal $explain:literal;)+) => {
        /// The diagnostic codes. Codes are permanent: one is never renamed or reused.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
        pub enum Code {
            $(#[serde(rename = $text)] $variant),+
        }

        impl Code {
            pub const ALL: &'static [Code] = &[$(Code::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $(Code::$variant => $text),+ }
            }

            pub fn parse(text: &str) -> Option<Code> {
                match text { $($text => Some(Code::$variant),)+ _ => None }
            }

            pub fn severity(self) -> Severity {
                match self { $(Code::$variant => Severity::$severity),+ }
            }

            /// The pipeline stage that raises the code.
            pub fn stage(self) -> &'static str {
                match self { $(Code::$variant => $stage),+ }
            }

            /// One sentence saying what the code means.
            pub fn summary(self) -> &'static str {
                match self { $(Code::$variant => $summary),+ }
            }

            /// Causes and a worked fix, for `dedalo explain`.
            pub fn explain(self) -> &'static str {
                match self { $(Code::$variant => $explain),+ }
            }
        }
    };
}

codes! {
    SyntaxError "SYNTAX_ERROR" Error "parse"
        "The text is not valid Dedalo syntax."
        "Dedalo is a strict subset of HCL: blocks (`keyword \"id\" { }`), attributes (`name = value`), literals, lists, objects and shape calls. Operators, conditionals, `for` expressions, splats and `${...}` templates are not part of version 1.\n\nFix: read the message for the construct expected at that position. Typical causes are a missing `}` or `\"`, a missing `=`, or an expression such as `width = 2 + 1`; write `width = 3`.";
    UnknownBlock "UNKNOWN_BLOCK" Error "lower"
        "A block keyword is not valid at this place."
        "Each block is allowed only inside certain parents (spec 5.5): for example `layout` inside `room`, `placement` inside `connection`, `feature` or `entity`.\n\nFix: use the keyword proposed in the message, or move the block to a parent that accepts it. Example: `layout { }` written directly in `dungeon` belongs inside the `room` it places.";
    UnknownAttribute "UNKNOWN_ATTRIBUTE" Error "lower"
        "An attribute name is not defined for this block."
        "Structural attributes are fixed per block type. Game and descriptive data go in a `properties { }` block instead.\n\nFix: use the closest name proposed in the message, or move the value into `properties`. Example: `detection_dc = 15` on a trap becomes `properties { detection_dc = 15 }`.";
    TypeMismatch "TYPE_MISMATCH" Error "lower"
        "A value has the wrong type."
        "The schema of each block fixes the type of every attribute and property: number, string, boolean, symbol, reference, point, list or shape.\n\nFix: write the value in the expected form. Example: `position = 3` should be a point, `position = [3, 2]`.";
    InvalidValue "INVALID_VALUE" Error "lower"
        "A value has the right type but is not allowed."
        "Raised for values outside an enumeration or range, malformed IDs, attributes set twice, conflicting attributes, and input over the limits (5 MB of source, 50,000 elements, nesting depth 16).\n\nFix: the message lists the allowed values. Example: `rotation = 45` on a room must be one of 0, 90, 180, 270.";
    UnknownProperty "UNKNOWN_PROPERTY" Warning "lower"
        "A property is not defined by the catalog for this element type."
        "Properties are typed by the catalog entry of the element's `type`. Unknown names are kept but flagged, because they are usually typos.\n\nFix: use the closest name proposed in the message, or prefix the name with `x_` to declare custom data: `x_origin = \"homebrew\"`. Run `dedalo catalog <type>` to list the properties of a type.";
    DuplicateId "DUPLICATE_ID" Error "lower"
        "Two elements share one ID."
        "IDs share one namespace per dungeon: a room and a feature cannot have the same ID, so that any tool can address any element by ID alone.\n\nFix: rename one of the two. Example: a room `crypt` and a note `crypt` become `crypt` and `crypt_note`.";
    UnknownReference "UNKNOWN_REFERENCE" Error "lower"
        "A reference names an element that does not exist."
        "References are quoted element IDs (`from = \"entrance\"`). They may point forward, and are resolved after the whole file is read.\n\nFix: use the closest existing ID proposed in the message, or declare the missing element.";
    UnknownType "UNKNOWN_TYPE" Error "lower"
        "An element type or subtype is not in the catalog."
        "The catalog fixes the valid types and subtypes. Run `dedalo catalog` to list them.\n\nFix: use one of the nearest matches in the message. For something the catalog does not cover, use `type = custom` with a one-to-three-character `symbol` and a `label` in `properties`.";
    InvalidShape "INVALID_SHAPE" Error "resolve"
        "A shape cannot be built."
        "A shape is invalid when a dimension is below one cell, a polygon has fewer than three vertices, self-intersects or has zero area, or the shape function is unknown. The shapes of 0.1 are square(side), rectangle(width, height), circle(radius) and polygon(points).\n\nFix: correct the arguments. Example: `shape = rectangle(8, 0)` becomes `shape = rectangle(8, 6)`.";
    LayoutCycle "LAYOUT_CYCLE" Error "resolve"
        "Rooms are placed relative to each other in a loop."
        "Relative placement needs a starting point: `a` north of `b` and `b` south of `a` leaves both undefined.\n\nFix: give one room of the loop an absolute position, `layout { position = [0, 0] }`, or remove its `adjacent_to`.";
    LayoutUnderconstrained "LAYOUT_UNDERCONSTRAINED" Error "resolve"
        "A room has no layout and nothing links it to a placed room."
        "A room without a `layout` block is placed automatically next to a space it shares a connection or corridor with. A room with no such link cannot be placed.\n\nFix: add `layout { position = [x, y] }` or `layout { adjacent_to = \"other\", direction = east }`, or connect the room to a placed room.";
    Overlap "OVERLAP" Error "resolve"
        "The interiors of two spaces intersect."
        "After placement, two spaces may touch along an edge but may not overlap. Automatic overlap resolution is left to a later release.\n\nFix: move one of the rooms. The message proposes the smallest `layout.offset` that clears the overlap; `align = start` or `align = end` often does the same.";
    OutOfBounds "OUT_OF_BOUNDS" Error "resolve"
        "An element lies outside its room."
        "Every corner of an element's footprint must lie inside or on the boundary of the owning space. Positions count cells from 0 at the top-left corner of the space.\n\nFix: use the nearest valid cell proposed in the message. Example: in an 8 by 6 room, valid positions for a one-cell element run from [0, 0] to [7, 5].";
    DoorNotPlaceable "DOOR_NOT_PLACEABLE" Error "resolve"
        "A connection's opening does not fit on the wall the two spaces share."
        "The opening must lie entirely on the segment shared by `from` and `to`. `placement.offset` is measured in cells from the west end of a north or south wall, or the north end of an east or west wall.\n\nFix: set `placement.offset` inside the range given in the message, or remove it to centre the opening.";
    UnresolvableConnection "UNRESOLVABLE_CONNECTION" Error "resolve"
        "The two spaces of a connection neither share a wall nor face each other closely."
        "A connection needs a shared wall, or facing walls at most `settings.max_stub` cells apart (default 2), where a short stub passage is inserted.\n\nFix: make the rooms adjacent (`layout { adjacent_to = ... }`), or join them with a `corridor` instead of a connection.";
    CorridorDisconnected "CORRIDOR_DISCONNECTED" Error "resolve"
        "A corridor does not reach the spaces it connects."
        "An automatic corridor is straight: the facing walls of the two spaces must overlap in projection by at least the corridor's width. A corridor with an explicit `path` must start and end on the walls of the spaces named in `connects`.\n\nFix: move the rooms so their walls face each other, or give the corridor an explicit orthogonal `path` of lattice points.";
    ElementOverlap "ELEMENT_OVERLAP" Warning "resolve"
        "Two blocking features share a cell."
        "Blocking features such as columns and furniture cannot occupy the same cell. Entities may stand on non-blocking features such as traps.\n\nFix: move one of the two features to a free cell.";
    UnreachableRoom "UNREACHABLE_ROOM" Warning "resolve"
        "A room cannot be reached from the entrance."
        "Spaces and connections form a graph. The entrance is the first room tagged `entrance`, else the first declared room.\n\nFix: add a `connection` or `corridor` that joins the room to the rest of the dungeon, or ignore the warning if the room is meant to be sealed.";
    ReferencedElement "REFERENCED_ELEMENT" Error "edit"
        "An element cannot be removed because others refer to it."
        "Removing an element that connections, notes or other elements reference would leave dangling references.\n\nFix: remove or update the referrers listed in the message first, or pass `cascade: true` to remove them together.";
    StaleSource "STALE_SOURCE" Error "edit"
        "The file changed since the edit was prepared."
        "An edit batch may carry `base_hash`, the content hash of the source it was computed against. A mismatch means someone else changed the file.\n\nFix: read the file again, recompute the edit, and send it with the new hash.";
    PluginNotFound "PLUGIN_NOT_FOUND" Error "any"
        "The document requires a plugin that is not installed."
        "Rendering never downloads. Plugins are looked up in `--plugin` paths, `./.dedalo/plugins/`, the per-user configuration directory and the built-ins.\n\nFix: install it with `dedalo plugin add <path-or-url>`, or remove the requirement from the `dedalo` header block.";
    PluginConflict "PLUGIN_CONFLICT" Error "any"
        "Two active plugins claim the same exclusive role."
        "Only one renderer may be active for an export.\n\nFix: remove one of the two `plugin` blocks, or select the renderer explicitly in the export.";
    PluginFailed "PLUGIN_FAILED" Error "any"
        "A plugin returned an error or invalid output."
        "Every plugin response is validated against a published schema.\n\nFix: check the plugin's message. `dedalo plugin check` runs the conformance suite against a plugin.";
    PluginTimeout "PLUGIN_TIMEOUT" Error "any"
        "A plugin exceeded its time or memory budget."
        "Plugins run with a fuel budget and 64 MiB of memory per call.\n\nFix: simplify the input, or report the problem to the plugin author.";
    ExportTooLarge "EXPORT_TOO_LARGE" Error "emit"
        "The output image would exceed 16,384 pixels on one side."
        "The pixel size is the map size in cells times `cell_px` (or `resolution`).\n\nFix: lower `cell_px` to the largest valid value given in the message, or export SVG or PDF, which have no pixel limit.";
}

/// A source location. Lines and columns start at 1; columns count characters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Span {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub start: [u32; 2],
    pub end: [u32; 2],
}

impl Span {
    pub fn from_range(range: TextRange, lines: &LineIndex) -> Span {
        Span {
            file: None,
            start: lines.line_col(range.start().into()),
            end: lines.line_col(range.end().into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Related {
    pub element: String,
    pub message: String,
}

/// One problem (spec 10.1). The message names the element, the value found
/// and the values allowed, so a reader without the file can act on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<Related>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    /// An edit that is certain to remove this diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Op>,
    /// Dotted attribute path inside the element, used to find the span.
    #[serde(skip)]
    pub at: Option<String>,
}

impl Diagnostic {
    pub fn new(code: Code, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: code.severity(),
            code,
            element: None,
            message: message.into(),
            span: None,
            related: Vec::new(),
            suggestion: None,
            fix: None,
            at: None,
        }
    }

    pub fn element(mut self, id: impl Into<String>) -> Self {
        self.element = Some(id.into());
        self
    }

    /// Points the diagnostic at an attribute of its element, such as
    /// `placement.offset`.
    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.at = Some(path.into());
        self
    }

    pub fn span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn related(mut self, element: impl Into<String>, message: impl Into<String>) -> Self {
        self.related.push(Related {
            element: element.into(),
            message: message.into(),
        });
        self
    }

    pub fn suggest(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn fix(mut self, op: Op) -> Self {
        self.fix = Some(op);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
}

/// The machine form of a validation result (spec 10.1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub status: Status,
    pub summary: Summary,
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn new(diagnostics: Vec<Diagnostic>) -> Report {
        let errors = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warnings = diagnostics.len() - errors;
        let status = if errors > 0 {
            Status::Error
        } else if warnings > 0 {
            Status::Warning
        } else {
            Status::Ok
        };
        Report {
            status,
            summary: Summary { errors, warnings },
            diagnostics,
        }
    }

    pub fn has_errors(&self) -> bool {
        self.summary.errors > 0
    }
}

/// Where each element and attribute was written, so that later stages can
/// report by element ID and still point at the source.
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    entries: BTreeMap<(String, String), TextRange>,
}

impl SourceMap {
    pub fn insert(&mut self, element: &str, path: &str, range: TextRange) {
        self.entries
            .entry((element.to_string(), path.to_string()))
            .or_insert(range);
    }

    /// The range of `path` inside `element`, falling back to the nearest
    /// enclosing path and finally to the element itself.
    pub fn lookup(&self, element: &str, path: &str) -> Option<TextRange> {
        let mut path = path.to_string();
        loop {
            if let Some(range) = self.entries.get(&(element.to_string(), path.clone())) {
                return Some(*range);
            }
            if path.is_empty() {
                return None;
            }
            path = match path.rsplit_once('.') {
                Some((head, _)) => head.to_string(),
                None => String::new(),
            };
        }
    }

    /// Fills in the span of every diagnostic that names an element but has
    /// no span yet.
    pub fn attach(&self, diagnostics: &mut [Diagnostic], lines: &LineIndex) {
        for d in diagnostics.iter_mut().filter(|d| d.span.is_none()) {
            let Some(element) = &d.element else { continue };
            if let Some(range) = self.lookup(element, d.at.as_deref().unwrap_or("")) {
                d.span = Some(Span::from_range(range, lines));
            }
        }
    }
}
