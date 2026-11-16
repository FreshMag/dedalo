//! The document model: the canonical form of a dungeon (spec section 6).
//!
//! DSL text and JSON are two spellings of these types. The JSON Schema is
//! generated from them, never written by hand. Defaults are not stored: an
//! absent field means the default. Aliases are stored in normalised form.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::value::{Num, Value};

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// Defines an enumeration whose values are written as bare symbols.
macro_rules! symbols {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl $name {
            pub const NAMES: &'static [&'static str] = &[$($text),+];

            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }
    };
}

symbols!(
    /// Who may see an element (spec 9.2).
    Visibility { Public => "public", Dm => "dm", Conditional => "conditional", Hidden => "hidden" }
);
symbols!(Direction { North => "north", South => "south", East => "east", West => "west" });
symbols!(Align { Start => "start", Center => "center", End => "end" });
symbols!(SpaceKind { Room => "room", Corridor => "corridor" });
symbols!(
    /// The block keyword of an element.
    Class { Feature => "feature", Entity => "entity" }
);
symbols!(LayoutMode { Absolute => "absolute", Relative => "relative", Automatic => "automatic" });
symbols!(Facing {
    North => "north", NorthEast => "north_east", East => "east", SouthEast => "south_east",
    South => "south", SouthWest => "south_west", West => "west", NorthWest => "north_west",
});
symbols!(Anchor {
    Center => "center", North => "north", NorthEast => "north_east", East => "east", SouthEast => "south_east",
    South => "south", SouthWest => "south_west", West => "west", NorthWest => "north_west",
});
symbols!(Preset { Dm => "dm", Player => "player", Clean => "clean" });
symbols!(Resolution { Low => "low", Medium => "medium", High => "high" });
symbols!(Bounds { Visible => "visible", Full => "full" });
symbols!(Page { Fit => "fit", A4 => "a4", A3 => "a3", Letter => "letter" });
symbols!(Orientation { Portrait => "portrait", Landscape => "landscape" });
symbols!(TextMode { Outline => "outline", Live => "live" });
symbols!(LegendMode { Automatic => "automatic", Custom => "custom", None => "none" });
symbols!(LegendPosition { Right => "right", Bottom => "bottom", Left => "left", Top => "top" });

/// The connection types of release 0.1. Doors that are locked or secret are
/// the type `door` with a flag (spec 5.10).
pub const CONNECTION_TYPES: &[&str] = &["door", "archway", "open", "stairs", "ladder", "trapdoor"];
/// Connection types placed by a cell position instead of a wall (spec 7.5).
pub const VERTICAL_TYPES: &[&str] = &["stairs", "ladder", "trapdoor"];
/// The category toggles of an export's `visibility` block (spec 9.1).
pub const TOGGLES: &[&str] = &[
    "grid",
    "room_labels",
    "features",
    "mechanisms",
    "traps",
    "creatures",
    "treasure",
    "secret_doors",
    "lighting",
    "terrain",
    "notes",
];
/// The built-in output formats (spec 9.6).
pub const FORMATS: &[&str] = &["svg", "png", "pdf", "txt"];
/// The newest language version this build reads.
pub const LANGUAGE_VERSION: u32 = 1;

pub type Props = BTreeMap<String, Value>;
pub type Point = [Num; 2];

/// Comments attached to a node, so that conversion to JSON and back loses
/// nothing but whitespace (spec 6.2).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Comments {
    /// Comment lines above the node.
    #[serde(default, skip_serializing_if = "is_default")]
    pub leading: Vec<String>,
    /// Comment on the node's own line.
    #[serde(default, skip_serializing_if = "is_default")]
    pub trailing: Option<String>,
    /// Comments after the last item inside the node.
    #[serde(default, skip_serializing_if = "is_default")]
    pub end: Vec<String>,
    /// Comments on the node's attributes, keyed by dotted path such as
    /// `layout.offset`.
    #[serde(default, skip_serializing_if = "is_default")]
    pub attrs: BTreeMap<String, Comments>,
}

impl Comments {
    pub fn is_empty(&self) -> bool {
        *self == Comments::default()
    }
}

/// A whole `.dedalo` file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Document {
    #[serde(rename = "$schema", default, skip_serializing_if = "is_default")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub dedalo: Option<Header>,
    pub dungeon: Dungeon,
    #[serde(default, skip_serializing_if = "is_default")]
    pub comments: Comments,
}

/// The `dedalo` header block: language version and plugin requirements.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Header {
    #[serde(default, skip_serializing_if = "is_default")]
    pub version: Option<u32>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub plugins: Vec<PluginRequirement>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub comments: Comments,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginRequirement {
    pub id: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dungeon {
    pub id: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub grid: Option<Grid>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub settings: Option<Settings>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub render: Option<RenderOptions>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub legend: Option<Legend>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub properties: Props,
    /// Rooms and corridors, in declaration order.
    #[serde(default, skip_serializing_if = "is_default")]
    pub spaces: Vec<Space>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub connections: Vec<Connection>,
    /// Elements that name no space; they are assigned by position.
    #[serde(default, skip_serializing_if = "is_default")]
    pub elements: Vec<Element>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub notes: Vec<Note>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub exports: Vec<Export>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub comments: Comments,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    /// Label of the scale, for example `5ft`. It does not affect geometry.
    #[serde(default, skip_serializing_if = "is_default")]
    pub unit: Option<String>,
    /// Only `square` before 1.0.
    #[serde(rename = "type", default, skip_serializing_if = "is_default")]
    pub ty: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "is_default")]
    pub default_wall: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub default_floor: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub seed: Option<i64>,
    /// Longest gap a connection bridges with a stub passage, in cells.
    #[serde(default, skip_serializing_if = "is_default")]
    pub max_stub: Option<Num>,
}

/// A room or a corridor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Space {
    pub kind: SpaceKind,
    pub id: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: Option<String>,
    /// Rooms only.
    #[serde(default, skip_serializing_if = "is_default")]
    pub shape: Option<ShapeCall>,
    /// Rooms only: 0, 90, 180 or 270 degrees clockwise.
    #[serde(default, skip_serializing_if = "is_default")]
    pub rotation: Option<i64>,
    /// Rooms only.
    #[serde(default, skip_serializing_if = "is_default")]
    pub layout: Option<Layout>,
    /// Corridors only: the two spaces it joins.
    #[serde(default, skip_serializing_if = "is_default")]
    pub connects: Vec<String>,
    /// Corridors only: width in cells, default 1.
    #[serde(default, skip_serializing_if = "is_default")]
    pub width: Option<Num>,
    /// Corridors only: global lattice points along the centre line.
    #[serde(default, skip_serializing_if = "is_default")]
    pub path: Vec<Point>,
    /// Corridors only: the gap it asks for between the rooms it joins.
    #[serde(default, skip_serializing_if = "is_default")]
    pub length: Option<Num>,
    /// Corridors only: distance from the start of the first space's wall.
    #[serde(default, skip_serializing_if = "is_default")]
    pub offset: Option<Num>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub visibility: Option<Visibility>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub reveal_when: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub properties: Props,
    #[serde(default, skip_serializing_if = "is_default")]
    pub elements: Vec<Element>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub comments: Comments,
}

impl Space {
    pub fn new(kind: SpaceKind, id: String) -> Space {
        Space {
            kind,
            id,
            name: None,
            shape: None,
            rotation: None,
            layout: None,
            connects: Vec::new(),
            width: None,
            path: Vec::new(),
            length: None,
            offset: None,
            visibility: None,
            reveal_when: None,
            tags: Vec::new(),
            properties: Props::new(),
            elements: Vec::new(),
            comments: Comments::default(),
        }
    }
}

/// A shape written as a function call, for example `rectangle(8, 6)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShapeCall {
    #[serde(rename = "fn")]
    pub func: String,
    #[serde(default)]
    pub args: Vec<Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default, skip_serializing_if = "is_default")]
    pub mode: Option<LayoutMode>,
    /// Absolute placement: the lattice point of the room's top-left corner.
    #[serde(default, skip_serializing_if = "is_default")]
    pub position: Option<Point>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub adjacent_to: Option<String>,
    /// Which side of `adjacent_to` this room lies on.
    #[serde(default, skip_serializing_if = "is_default")]
    pub direction: Option<Direction>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub align: Option<Align>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub offset: Option<Num>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub gap: Option<Num>,
}

/// A passage between two spaces.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub id: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: Option<String>,
    pub from: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub to: Option<String>,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub secret: Option<bool>,
    /// Stairs, ladders and trapdoors: the cell inside `from`.
    #[serde(default, skip_serializing_if = "is_default")]
    pub position: Option<Point>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub placement: Option<ConnectionPlacement>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub visibility: Option<Visibility>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub reveal_when: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub properties: Props,
    #[serde(default, skip_serializing_if = "is_default")]
    pub comments: Comments,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectionPlacement {
    /// Side of the `from` space.
    #[serde(default, skip_serializing_if = "is_default")]
    pub wall: Option<Direction>,
    /// Polygon edge by index, as an alternative to `wall`.
    #[serde(default, skip_serializing_if = "is_default")]
    pub edge: Option<i64>,
    /// Circle boundary point in degrees clockwise from north.
    #[serde(default, skip_serializing_if = "is_default")]
    pub bearing: Option<Num>,
    /// Distance in cells from the start of the wall to the start of the opening.
    #[serde(default, skip_serializing_if = "is_default")]
    pub offset: Option<Num>,
    /// Opening width in cells, default 1.
    #[serde(default, skip_serializing_if = "is_default")]
    pub width: Option<Num>,
}

/// A feature or an entity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub class: Class,
    pub id: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub subtype: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: Option<String>,
    /// Top-left cell of the footprint, in the owning space.
    #[serde(default, skip_serializing_if = "is_default")]
    pub position: Option<Point>,
    /// Footprint in cells; defaults to the catalog entry.
    #[serde(default, skip_serializing_if = "is_default")]
    pub size: Option<Point>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub shape: Option<ShapeCall>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub rotation: Option<i64>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub facing: Option<Facing>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub placement: Option<ElementPlacement>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub visibility: Option<Visibility>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub reveal_when: Option<String>,
    /// Opaque pointer to an external bestiary. Never interpreted.
    #[serde(default, skip_serializing_if = "is_default")]
    pub reference: Option<String>,
    /// Items only: the container that holds it.
    #[serde(default, skip_serializing_if = "is_default")]
    pub contained_in: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub properties: Props,
    #[serde(default, skip_serializing_if = "is_default")]
    pub comments: Comments,
}

impl Element {
    pub fn new(class: Class, id: String) -> Element {
        Element {
            class,
            id,
            ty: String::new(),
            subtype: None,
            name: None,
            position: None,
            size: None,
            shape: None,
            rotation: None,
            facing: None,
            placement: None,
            visibility: None,
            reveal_when: None,
            reference: None,
            contained_in: None,
            tags: Vec::new(),
            properties: Props::new(),
            comments: Comments::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementPlacement {
    /// Against a wall of the owning space.
    #[serde(default, skip_serializing_if = "is_default")]
    pub wall: Option<Direction>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub offset: Option<Num>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub anchor: Option<Anchor>,
}

/// An annotation, optionally attached to an element or positioned.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub text: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub position: Option<Point>,
    /// Notes default to `dm`.
    #[serde(default, skip_serializing_if = "is_default")]
    pub visibility: Option<Visibility>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub reveal_when: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub comments: Comments,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderOptions {
    /// Theme name, dotted when it comes from a plugin.
    #[serde(default, skip_serializing_if = "is_default")]
    pub theme: Option<String>,
    /// Whether the dungeon name is drawn as a title.
    #[serde(default, skip_serializing_if = "is_default")]
    pub title: Option<bool>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub text: Option<TextMode>,
    /// Print room IDs instead of names.
    #[serde(default, skip_serializing_if = "is_default")]
    pub room_ids: Option<bool>,
    /// Options passed to plugins, by plugin ID.
    #[serde(default, skip_serializing_if = "is_default")]
    pub plugins: BTreeMap<String, Props>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Legend {
    #[serde(default, skip_serializing_if = "is_default")]
    pub mode: Option<LegendMode>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub position: Option<LegendPosition>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub title: Option<String>,
    /// Custom entries, or label overrides in automatic mode.
    #[serde(default, skip_serializing_if = "is_default")]
    pub entries: Vec<LegendEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LegendEntry {
    /// What the entry describes: a type (`trap`), a type and subtype
    /// (`lighting.torch`) or a connection type (`door`).
    pub key: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub label: Option<String>,
}

/// One output configuration (spec 9.1).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Export {
    /// `default` when the block has no label.
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub preset: Option<Preset>,
    /// `svg`, `png`, `pdf`, `txt`, or a dotted plugin format.
    #[serde(default, skip_serializing_if = "is_default")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub resolution: Option<Resolution>,
    /// Pixels per cell, 16 to 512; overrides `resolution`.
    #[serde(default, skip_serializing_if = "is_default")]
    pub cell_px: Option<i64>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub rotation: Option<i64>,
    /// Condition tags that reveal `conditional` elements.
    #[serde(default, skip_serializing_if = "is_default")]
    pub revealed: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub include: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub exclude: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub bounds: Option<Bounds>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub page: Option<Page>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub orientation: Option<Orientation>,
    /// Category toggles that override the preset.
    #[serde(default, skip_serializing_if = "is_default")]
    pub visibility: BTreeMap<String, bool>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub legend: Option<Legend>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub render: Option<RenderOptions>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub comments: Comments,
}

impl Dungeon {
    /// Every element with the space that owns it, in declaration order.
    pub fn elements(&self) -> impl Iterator<Item = (Option<&Space>, &Element)> {
        let nested = self
            .spaces
            .iter()
            .flat_map(|s| s.elements.iter().map(move |e| (Some(s), e)));
        nested.chain(self.elements.iter().map(|e| (None, e)))
    }

    pub fn space(&self, id: &str) -> Option<&Space> {
        self.spaces.iter().find(|s| s.id == id)
    }
}
