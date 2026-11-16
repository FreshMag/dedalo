//! Editor assistance from the core (spec 14.2): completion, hover help and
//! source locations. The editor has no language knowledge of its own.

use std::collections::BTreeMap;

use dedalo_model::document::*;
use dedalo_syntax::ast::{self, Block};
use dedalo_syntax::text::LineIndex;
use dedalo_syntax::{SyntaxKind, SyntaxNode, TextSize};
use serde::Serialize;

use crate::Registry;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Completion {
    pub label: String,
    /// `keyword`, `attribute`, `value`, `reference` or `snippet`.
    pub kind: &'static str,
    pub detail: String,
    /// Text to insert. Lines after the first are indented relative to the
    /// line the completion starts on.
    pub insert: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Completions {
    /// Byte offset where the replaced word starts.
    pub from: usize,
    pub items: Vec<Completion>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hover {
    pub from: usize,
    pub to: usize,
    pub markdown: String,
}

/// Short descriptions of the block keywords.
const BLOCKS: &[(&str, &str, bool)] = &[
    (
        "dedalo",
        "Header: language version and plugin requirements.",
        false,
    ),
    ("dungeon", "The dungeon. A file holds exactly one.", true),
    (
        "grid",
        "Scale label (`unit = \"5ft\"`) and grid type.",
        false,
    ),
    (
        "settings",
        "Defaults: `default_wall`, `default_floor`, `seed`, `max_stub`.",
        false,
    ),
    (
        "render",
        "Theme, title, live text and room-ID labels.",
        false,
    ),
    (
        "legend",
        "Legend mode, position, title and `entry` blocks.",
        false,
    ),
    (
        "properties",
        "Typed game data from the catalog; `x_` names are free.",
        false,
    ),
    (
        "room",
        "A space with a `shape`, placed by its `layout` block.",
        true,
    ),
    (
        "corridor",
        "Joins two spaces: `connects = [\"a\", \"b\"]`.",
        true,
    ),
    (
        "connection",
        "A door or passage between two spaces: `from`, `to`, `type`.",
        true,
    ),
    (
        "feature",
        "A fixed element: furniture, trap, light, terrain, mechanism.",
        true,
    ),
    ("entity", "A creature, NPC or item.", true),
    (
        "note",
        "An annotation, numbered on the map; DM-only by default.",
        true,
    ),
    (
        "export",
        "One output configuration: preset, format, rotation, toggles.",
        true,
    ),
    (
        "layout",
        "Room placement: `position`, or `north_of` and friends with `align`, `offset`, `gap`.",
        false,
    ),
    (
        "placement",
        "Wall or anchor placement: `wall`, `offset`, `anchor`, `width`.",
        false,
    ),
    ("visibility", "Category toggles of an export.", false),
    (
        "entry",
        "A legend entry: `entry \"trap\" { label = \"Trappola\" }`.",
        true,
    ),
    ("plugin", "A plugin requirement or plugin options.", true),
];

/// Short descriptions of attributes, by name.
const ATTRS: &[(&str, &str)] = &[
    ("name", "Display name, printed as the room label."),
    (
        "shape",
        "`square(side)`, `rectangle(w, h)`, `circle(r)` or `polygon([[x, y], ...])`.",
    ),
    ("rotation", "0, 90, 180 or 270 degrees clockwise."),
    ("visibility", "`public`, `dm`, `conditional` or `hidden`."),
    (
        "reveal_when",
        "Condition tag that reveals a `conditional` element when an export lists it in `revealed`.",
    ),
    (
        "tags",
        "Free tags. The first room tagged `entrance` is the entrance.",
    ),
    ("connects", "The two spaces a corridor joins."),
    ("width", "Width in cells."),
    (
        "path",
        "Lattice points along a corridor's centre line; orthogonal segments.",
    ),
    (
        "length",
        "Gap a corridor asks for between relatively placed rooms; default 2.",
    ),
    (
        "offset",
        "Cells from the start of the wall: its west end (north and south walls) or north end (east and west walls).",
    ),
    (
        "from",
        "The space a connection starts in; `placement.wall` is a side of it.",
    ),
    ("to", "The space a connection leads to."),
    (
        "type",
        "The element type from the catalog, or the connection type.",
    ),
    ("subtype", "A subtype of the type."),
    (
        "secret",
        "A secret door: hidden from players, drawn as S for the DM.",
    ),
    (
        "position",
        "Top-left cell, counted from 0 in the owning room; on a room's `layout`, the map point of its top-left corner.",
    ),
    ("size", "Footprint in cells, `[w, h]`."),
    (
        "facing",
        "Direction a creature faces: north, north_east, ...",
    ),
    (
        "reference",
        "Opaque pointer to an external bestiary, such as `srd:skeleton`.",
    ),
    (
        "contained_in",
        "The container holding this item; it is then listed, not drawn.",
    ),
    ("target", "The element a note is attached to."),
    ("text", "The note's text."),
    (
        "adjacent_to",
        "The room this room is placed next to, with `direction`.",
    ),
    (
        "direction",
        "Which side of `adjacent_to` this room lies on.",
    ),
    ("north_of", "Place this room north of another room."),
    ("south_of", "Place this room south of another room."),
    ("east_of", "Place this room east of another room."),
    ("west_of", "Place this room west of another room."),
    ("align", "`start`, `center` or `end` along the shared side."),
    ("gap", "Cells between the two rooms."),
    (
        "mode",
        "Layout: `absolute`, `relative` or `automatic`. Legend: `automatic`, `custom` or `none`.",
    ),
    ("wall", "`north`, `south`, `east` or `west`."),
    ("edge", "Polygon edge by index, from the first vertex."),
    ("anchor", "`center` or a compass point of the room."),
    ("preset", "`dm`, `player` or `clean`."),
    ("format", "`svg`, `png`, `pdf` or `txt`."),
    ("cell_px", "Pixels per cell, 16 to 512."),
    (
        "revealed",
        "Condition tags that reveal `conditional` elements.",
    ),
    ("include", "IDs shown regardless of visibility."),
    ("exclude", "IDs never shown."),
    (
        "bounds",
        "`visible` crops to what is drawn; `full` keeps DM and player maps aligned.",
    ),
    ("unit", "The scale label, such as `5ft`."),
    ("seed", "Seed of the random stream."),
    (
        "max_stub",
        "Longest gap a door bridges with a stub passage; default 2.",
    ),
    ("theme", "The theme; `classic` is built in."),
    ("version", "Language version."),
];

fn block_names(parent: &str, grand: &str) -> &'static [&'static str] {
    match parent {
        "" => &["dedalo", "dungeon"],
        "dedalo" => &["plugin"],
        "dungeon" => &[
            "grid",
            "settings",
            "render",
            "legend",
            "properties",
            "room",
            "corridor",
            "connection",
            "feature",
            "entity",
            "note",
            "export",
        ],
        "room" => &["layout", "properties", "feature", "entity", "note"],
        "corridor" => &["properties", "feature", "entity", "note"],
        "connection" | "feature" | "entity" => &["placement", "properties", "note"],
        "export" => &["visibility", "legend", "render"],
        "legend" => &["entry"],
        "render" if grand == "dungeon" || grand == "export" => &["plugin"],
        _ => &[],
    }
}

fn attr_names(parent: &str, grand: &str) -> &'static [&'static str] {
    match parent {
        "dedalo" => &["version"],
        "dungeon" => &["name"],
        "grid" => &["unit", "type"],
        "settings" => &["default_wall", "default_floor", "seed", "max_stub"],
        "render" => &["theme", "title", "text", "room_ids"],
        "legend" => &["mode", "position", "title"],
        "entry" => &["label"],
        "room" => &[
            "name",
            "shape",
            "rotation",
            "visibility",
            "reveal_when",
            "tags",
        ],
        "corridor" => &[
            "name",
            "connects",
            "width",
            "path",
            "length",
            "offset",
            "visibility",
            "reveal_when",
            "tags",
        ],
        "connection" => &[
            "from",
            "to",
            "type",
            "name",
            "secret",
            "position",
            "visibility",
            "reveal_when",
            "tags",
        ],
        "feature" | "entity" => &[
            "type",
            "subtype",
            "name",
            "position",
            "size",
            "rotation",
            "facing",
            "visibility",
            "reveal_when",
            "reference",
            "contained_in",
            "tags",
        ],
        "note" => &["text", "target", "position", "visibility", "reveal_when"],
        "layout" => &[
            "north_of",
            "south_of",
            "east_of",
            "west_of",
            "align",
            "offset",
            "gap",
            "position",
            "adjacent_to",
            "direction",
            "mode",
        ],
        "placement" if grand == "connection" => &["wall", "offset", "width", "edge"],
        "placement" => &["wall", "offset", "anchor"],
        "export" => &[
            "preset",
            "format",
            "cell_px",
            "rotation",
            "revealed",
            "include",
            "exclude",
            "bounds",
            "resolution",
            "page",
            "orientation",
        ],
        "visibility" => TOGGLES,
        "plugin" => &["version"],
        _ => &[],
    }
}

fn snippet(keyword: &str) -> String {
    match keyword {
        "room" => "room \"new_room\" {\n    shape = rectangle(6, 4)\n\n    layout {\n        east_of = \"\"\n    }\n}".to_string(),
        "corridor" => "corridor \"new_corridor\" {\n    connects = [\"\", \"\"]\n}".to_string(),
        "connection" => "connection \"new_door\" {\n    from = \"\"\n    to = \"\"\n    type = door\n}".to_string(),
        "feature" => "feature \"new_feature\" {\n    type = furniture\n    position = [0, 0]\n}".to_string(),
        "entity" => "entity \"new_entity\" {\n    type = creature\n    position = [0, 0]\n}".to_string(),
        "note" => "note \"new_note\" {\n    text = \"\"\n}".to_string(),
        "export" => "export \"players\" {\n    preset = player\n    format = png\n}".to_string(),
        "entry" => "entry \"trap\" {\n    label = \"\"\n}".to_string(),
        "plugin" => "plugin \"\" {\n}".to_string(),
        other => format!("{other} {{\n}}"),
    }
}

/// The keywords of the blocks enclosing `offset`, outermost first, with the
/// nodes themselves.
fn enclosing(root: &SyntaxNode, offset: usize) -> Vec<Block> {
    let at = TextSize::new(offset as u32);
    let mut out = Vec::new();
    let mut node = root.clone();
    loop {
        let next = node.children().find(|c| {
            let r = c.text_range();
            match c.kind() {
                SyntaxKind::Block => c
                    .children()
                    .find(|b| b.kind() == SyntaxKind::Body)
                    .is_some_and(|b| {
                        b.text_range().start() < at
                            && (at < b.text_range().end() || !b.text().to_string().ends_with('}'))
                    }),
                SyntaxKind::Body => r.start() < at && at <= r.end(),
                _ => false,
            }
        });
        match next {
            Some(n) if n.kind() == SyntaxKind::Block => {
                out.push(Block(n.clone()));
                node = n;
            }
            Some(n) => node = n,
            None => return out,
        }
    }
}

fn attr_value(block: &Block, name: &str) -> Option<String> {
    let body = block.body()?;
    ast::items(&body).find_map(|i| match i {
        ast::Item::Attr(a) if a.name_text() == name => {
            let v = a.value()?;
            v.symbol_text()
                .or_else(|| v.literal_token().map(|t| ast::label_text(&t)))
        }
        _ => None,
    })
}

struct Ids {
    spaces: Vec<(String, String)>,
    features: Vec<(String, String)>,
    all: Vec<(String, String)>,
}

fn ids(doc: Option<&Document>) -> Ids {
    let mut ids = Ids {
        spaces: Vec::new(),
        features: Vec::new(),
        all: Vec::new(),
    };
    let Some(doc) = doc else { return ids };
    let d = &doc.dungeon;
    for s in &d.spaces {
        let entry = (
            s.id.clone(),
            format!("{} {}", s.kind.as_str(), s.name.clone().unwrap_or_default())
                .trim()
                .to_string(),
        );
        ids.spaces.push(entry.clone());
        ids.all.push(entry);
    }
    for (_, e) in d.elements() {
        let entry = (e.id.clone(), format!("{} {}", e.class.as_str(), e.ty));
        if e.class == Class::Feature {
            ids.features.push(entry.clone());
        }
        ids.all.push(entry);
    }
    ids.all.extend(
        d.connections
            .iter()
            .map(|c| (c.id.clone(), format!("connection {}", c.ty))),
    );
    ids.all
        .extend(d.notes.iter().map(|n| (n.id.clone(), "note".to_string())));
    ids
}

fn values(names: &[&str], detail: &str) -> Vec<Completion> {
    names
        .iter()
        .map(|n| Completion {
            label: n.to_string(),
            kind: "value",
            detail: detail.to_string(),
            insert: n.to_string(),
        })
        .collect()
}

/// Completions at a byte offset of DSL text.
pub fn complete(source: &str, offset: usize, reg: &Registry) -> Completions {
    let offset = offset.min(source.len());
    let offset = (0..=offset)
        .rev()
        .find(|o| source.is_char_boundary(*o))
        .unwrap_or(0);
    let before = &source[..offset];
    let from = before
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '"'))
        .map_or(0, |p| p + 1);
    let prefix = source[from..offset].trim_start_matches('"').to_lowercase();
    let line = &before[before.rfind('\n').map_or(0, |p| p + 1)..from];
    let parsed = dedalo_syntax::parse(source);
    let root = parsed.syntax();
    let blocks = enclosing(&root, offset);
    let keywords: Vec<String> = blocks.iter().map(|b| b.keyword_text()).collect();
    let parent = keywords.last().map_or("", String::as_str);
    let grand = if keywords.len() >= 2 {
        keywords[keywords.len() - 2].as_str()
    } else {
        ""
    };
    let cat = &reg.catalog;
    let lowered = dedalo_model::lower::lower(&root, source, cat);
    let ids = ids(lowered.document.as_ref());
    // The element the cursor is in, for `type`, `subtype` and properties.
    let element = blocks.iter().rev().find(|b| {
        matches!(
            b.keyword_text().as_str(),
            "room" | "corridor" | "connection" | "feature" | "entity"
        )
    });
    let element_type = element
        .and_then(|b| attr_value(b, "type"))
        .unwrap_or_default();

    // `name = |` or `name = [..., |`: a value. Otherwise an item name.
    let value_of = line
        .trim_start()
        .split_once('=')
        .map(|(name, _)| name.trim().to_string())
        .filter(|n| ast::is_symbol(n));
    let mut items: Vec<Completion> = Vec::new();
    match value_of {
        Some(attr) => {
            let refs = |list: &[(String, String)]| -> Vec<Completion> {
                list.iter()
                    .map(|(id, d)| Completion {
                        label: id.clone(),
                        kind: "reference",
                        detail: d.clone(),
                        insert: format!("\"{id}\""),
                    })
                    .collect()
            };
            items = match (parent, attr.as_str()) {
                ("properties", name) => {
                    let spec = match grand {
                        "room" | "corridor" | "connection" => cat.structure_property(grand, name),
                        _ => cat.property(&element_type, name),
                    };
                    match spec {
                        Some(s) if s.values.is_some() => values(
                            &s.values
                                .iter()
                                .flatten()
                                .map(String::as_str)
                                .collect::<Vec<_>>(),
                            s.ty.describe(),
                        ),
                        Some(s) if s.ty == dedalo_catalog::PropType::Boolean => {
                            values(&["true", "false"], "boolean")
                        }
                        Some(s)
                            if s.ty == dedalo_catalog::PropType::Reference
                                || s.of == Some(dedalo_catalog::PropType::Reference) =>
                        {
                            refs(&ids.all)
                        }
                        _ => Vec::new(),
                    }
                }
                ("connection", "type") => {
                    let mut v = values(CONNECTION_TYPES, "connection type");
                    v.extend(values(&["locked_door", "secret_door"], "alias of door"));
                    v
                }
                ("feature" | "entity", "type") => cat
                    .types
                    .iter()
                    .filter(|(_, t)| match t.class {
                        dedalo_catalog::Class::Either => true,
                        dedalo_catalog::Class::Feature => parent == "feature",
                        dedalo_catalog::Class::Entity => parent == "entity",
                    })
                    .map(|(n, t)| Completion {
                        label: n.clone(),
                        kind: "value",
                        detail: format!("{}, {}", t.label, t.category),
                        insert: n.clone(),
                    })
                    .collect(),
                (_, "subtype") => cat
                    .get(&element_type)
                    .map(|t| {
                        values(
                            &t.subtypes.iter().map(String::as_str).collect::<Vec<_>>(),
                            &t.label,
                        )
                    })
                    .unwrap_or_default(),
                (_, "shape") => vec![
                    Completion {
                        label: "rectangle".into(),
                        kind: "snippet",
                        detail: "rectangle(width, height)".into(),
                        insert: "rectangle(6, 4)".into(),
                    },
                    Completion {
                        label: "square".into(),
                        kind: "snippet",
                        detail: "square(side)".into(),
                        insert: "square(5)".into(),
                    },
                    Completion {
                        label: "circle".into(),
                        kind: "snippet",
                        detail: "circle(radius)".into(),
                        insert: "circle(3)".into(),
                    },
                    Completion {
                        label: "polygon".into(),
                        kind: "snippet",
                        detail: "polygon(points)".into(),
                        insert: "polygon([[0, 0], [6, 0], [6, 3], [3, 3], [3, 6], [0, 6]])".into(),
                    },
                ],
                (_, "wall" | "direction") => values(Direction::NAMES, "side"),
                (_, "align") => values(Align::NAMES, "alignment"),
                (_, "anchor") => values(Anchor::NAMES, "anchor"),
                (_, "visibility") => values(Visibility::NAMES, "visibility"),
                (_, "facing") => values(Facing::NAMES, "facing"),
                (_, "preset") => values(Preset::NAMES, "preset"),
                (_, "format") => values(FORMATS, "format"),
                (_, "resolution") => values(Resolution::NAMES, "resolution"),
                (_, "bounds") => values(Bounds::NAMES, "bounds"),
                (_, "page") => values(Page::NAMES, "page"),
                (_, "orientation") => values(Orientation::NAMES, "orientation"),
                (_, "scale") => values(PrintScale::NAMES, "print scale"),
                ("layout", "mode") => values(LayoutMode::NAMES, "layout mode"),
                ("legend", "mode") => values(LegendMode::NAMES, "legend mode"),
                ("legend", "position") => values(LegendPosition::NAMES, "legend position"),
                ("render", "text") => values(TextMode::NAMES, "text mode"),
                ("render", "theme") => values(
                    &reg.themes.keys().map(String::as_str).collect::<Vec<_>>(),
                    "theme",
                ),
                (_, "rotation") => values(&["0", "90", "180", "270"], "degrees"),
                (_, "secret" | "title" | "room_ids") => values(&["true", "false"], "boolean"),
                ("visibility", _) => values(&["true", "false"], "toggle"),
                (
                    _,
                    "from" | "to" | "connects" | "adjacent_to" | "north_of" | "south_of"
                    | "east_of" | "west_of" | "room" | "corridor",
                ) => refs(&ids.spaces),
                (_, "contained_in") => refs(&ids.features),
                (_, "target" | "include" | "exclude") => refs(&ids.all),
                _ => Vec::new(),
            };
        }
        None => {
            if !line.trim().is_empty() {
                return Completions { from, items };
            }
            for kw in block_names(parent, grand) {
                let doc = BLOCKS.iter().find(|b| b.0 == *kw).map_or("", |b| b.1);
                items.push(Completion {
                    label: kw.to_string(),
                    kind: "keyword",
                    detail: doc.to_string(),
                    insert: snippet(kw),
                });
            }
            if parent == "properties" {
                let names = match grand {
                    "room" | "corridor" | "connection" => cat.structure_property_names(grand),
                    _ => cat.property_names(&element_type),
                };
                for n in names {
                    items.push(Completion {
                        label: n.to_string(),
                        kind: "attribute",
                        detail: "property".to_string(),
                        insert: format!("{n} = "),
                    });
                }
            }
            for a in attr_names(parent, grand) {
                let doc = ATTRS.iter().find(|x| x.0 == *a).map_or("", |x| x.1);
                items.push(Completion {
                    label: a.to_string(),
                    kind: "attribute",
                    detail: doc.to_string(),
                    insert: format!("{a} = "),
                });
            }
        }
    }
    items.retain(|c| c.label.to_lowercase().starts_with(&prefix));
    let mut seen = std::collections::BTreeSet::new();
    items.retain(|c| seen.insert(c.label.clone()));
    Completions { from, items }
}

/// Hover help for the token at a byte offset.
pub fn hover(source: &str, offset: usize, reg: &Registry) -> Option<Hover> {
    let parsed = dedalo_syntax::parse(source);
    let root = parsed.syntax();
    let offset = offset.min(source.len().saturating_sub(1));
    let token = root
        .token_at_offset(TextSize::new(offset as u32))
        .right_biased()?;
    let range = token.text_range();
    let (from, to) = (
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    );
    let parent = token.parent()?;
    let text = token.text().to_string();
    let cat = &reg.catalog;
    let markdown = match (token.kind(), parent.kind()) {
        (SyntaxKind::Ident, SyntaxKind::Block) => {
            let (_, doc, _) = BLOCKS.iter().find(|b| b.0 == text)?;
            format!("**{text}**: {doc}")
        }
        (SyntaxKind::Ident, SyntaxKind::Attribute) => {
            let in_properties = parent
                .parent()
                .and_then(|b| b.parent())
                .map(Block)
                .is_some_and(|b| b.keyword_text() == "properties");
            if in_properties {
                let element = parent
                    .ancestors()
                    .filter(|n| n.kind() == SyntaxKind::Block)
                    .map(Block)
                    .find(|b| matches!(b.keyword_text().as_str(), "feature" | "entity"));
                let ty = element
                    .and_then(|b| attr_value(&b, "type"))
                    .unwrap_or_default();
                let spec = cat.property(&ty, &text)?;
                format!(
                    "**{text}**: property, {}{}",
                    spec.ty.describe(),
                    spec.doc
                        .as_ref()
                        .map(|d| format!(". {d}"))
                        .unwrap_or_default()
                )
            } else {
                let (_, doc) = ATTRS.iter().find(|a| a.0 == text)?;
                format!("**{text}**: {doc}")
            }
        }
        (SyntaxKind::Ident, SyntaxKind::Symbol) => {
            let attr = parent
                .parent()
                .filter(|p| p.kind() == SyntaxKind::Attribute)
                .map(ast::Attribute);
            let attr_name = attr.map(|a| a.name_text()).unwrap_or_default();
            match (attr_name.as_str(), cat.get(&text)) {
                ("type", Some(t)) => format!(
                    "**{text}**: {} ({}, category `{}`, visible to `{}` by default){}",
                    t.label,
                    format!("{:?}", t.class).to_lowercase(),
                    t.category,
                    t.default_visibility,
                    if t.subtypes.is_empty() {
                        String::new()
                    } else {
                        format!("\n\nSubtypes: {}", t.subtypes.join(", "))
                    }
                ),
                _ => return None,
            }
        }
        (SyntaxKind::Ident, SyntaxKind::Call) => match text.as_str() {
            "square" => "**square(side)**: an axis-aligned square.".to_string(),
            "rectangle" => "**rectangle(width, height)**: an axis-aligned rectangle.".to_string(),
            "circle" => "**circle(radius)**: a circle inscribed in a 2r by 2r box.".to_string(),
            "polygon" => {
                "**polygon(points)**: a simple polygon of three or more `[x, y]` vertices."
                    .to_string()
            }
            _ => return None,
        },
        (SyntaxKind::Str, _) => {
            let id = ast::label_text(&token);
            let compiled = crate::compile(crate::Input::Dsl(source), reg);
            let resolved = compiled.resolved.as_ref()?;
            if let Some(s) = resolved.spaces.iter().find(|s| s.id == id) {
                let c = |v: i64| dedalo_model::Num(v).to_string();
                format!(
                    "**{}** `{id}`{}: {} by {} cells at x {} to {}, y {} to {}",
                    s.kind.as_str(),
                    s.name
                        .as_ref()
                        .map(|n| format!(" \"{n}\""))
                        .unwrap_or_default(),
                    c(s.bbox.w()),
                    c(s.bbox.h()),
                    c(s.bbox.x0),
                    c(s.bbox.x1),
                    c(s.bbox.y0),
                    c(s.bbox.y1)
                )
            } else if let Some(e) = resolved.elements.iter().find(|e| e.id == id) {
                format!(
                    "**{}** `{id}`: {}, in `{}`, visible to `{}`",
                    e.class.as_str(),
                    e.label,
                    e.space.clone().unwrap_or_default(),
                    e.visibility.as_str()
                )
            } else {
                let o = resolved.openings.iter().find(|o| o.id == id)?;
                format!(
                    "**connection** `{id}`: {} from `{}` to `{}`",
                    o.ty,
                    o.from,
                    o.to.clone().unwrap_or_default()
                )
            }
        }
        _ => return None,
    };
    Some(Hover { from, to, markdown })
}

/// Where the block declaring `id` starts: `[line, column]`, from 1.
pub fn locate(source: &str, id: &str) -> Option<[u32; 2]> {
    let root = dedalo_syntax::parse(source).syntax();
    let node = root
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::Block)
        .map(Block)
        .find(|b| b.labels().first().map(ast::label_text).as_deref() == Some(id))?;
    Some(LineIndex::new(source).line_col(node.0.text_range().start().into()))
}

/// The spaces and their elements, for the editor's outline panel.
pub fn outline(source: &str, reg: &Registry) -> Vec<BTreeMap<&'static str, serde_json::Value>> {
    let root = dedalo_syntax::parse(source).syntax();
    let lowered = dedalo_model::lower::lower(&root, source, &reg.catalog);
    let Some(doc) = lowered.document else {
        return Vec::new();
    };
    let entry = |kind: &str, id: &str, label: String, depth: u8| {
        BTreeMap::from([
            ("kind", serde_json::json!(kind)),
            ("id", serde_json::json!(id)),
            ("label", serde_json::json!(label)),
            ("depth", serde_json::json!(depth)),
            ("line", serde_json::json!(locate(source, id).map(|p| p[0]))),
        ])
    };
    let d = &doc.dungeon;
    let mut out = Vec::new();
    for s in &d.spaces {
        out.push(entry(
            s.kind.as_str(),
            &s.id,
            s.name.clone().unwrap_or_default(),
            0,
        ));
        for e in &s.elements {
            out.push(entry(
                e.class.as_str(),
                &e.id,
                e.subtype.clone().unwrap_or(e.ty.clone()),
                1,
            ));
        }
    }
    for c in &d.connections {
        out.push(entry(
            "connection",
            &c.id,
            format!("{} → {}", c.from, c.to.clone().unwrap_or_default()),
            0,
        ));
    }
    for n in &d.notes {
        out.push(entry(
            "note",
            &n.id,
            n.target.clone().unwrap_or_default(),
            0,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "dungeon \"d\" {\n    room \"hall\" {\n        shape = square(4)\n\n        feature \"t\" {\n            type = trap\n            subtype = \n        }\n    }\n\n    connection \"c\" {\n        from = \n    }\n}\n";

    fn labels(c: &Completions) -> Vec<String> {
        c.items.iter().map(|i| i.label.clone()).collect()
    }

    #[test]
    fn completion_follows_the_context() {
        let reg = Registry::builtin();
        let at = |needle: &str| SRC.find(needle).unwrap() + needle.len();
        assert_eq!(
            labels(&complete(SRC, at("subtype = "), &reg)),
            ["pressure_plate", "dart_launcher", "pit", "generic"]
        );
        assert_eq!(labels(&complete(SRC, at("from = "), &reg)), ["hall"]);
        assert_eq!(
            complete(SRC, at("from = "), &reg).items[0].insert,
            "\"hall\""
        );
        let in_room = SRC.find("\n\n        feature").unwrap() + 1;
        let names = labels(&complete(SRC, in_room, &reg));
        assert!(
            names.contains(&"layout".to_string()) && names.contains(&"rotation".to_string()),
            "{names:?}"
        );
        let typed = "dungeon \"d\" {\n    room \"a\" {\n        sh";
        assert_eq!(labels(&complete(typed, typed.len(), &reg)), ["shape"]);
    }

    #[test]
    fn hover_explains_tokens() {
        let reg = Registry::builtin();
        let on = |needle: &str| {
            hover(SRC, SRC.find(needle).unwrap(), &reg)
                .map(|h| h.markdown)
                .unwrap_or_default()
        };
        assert!(on("trap").contains("Trap"), "{}", on("trap"));
        assert!(on("shape").contains("rectangle"), "{}", on("shape"));
        assert!(on("connection").starts_with("**connection**"));
        assert_eq!(locate(SRC, "c"), Some([11, 5]));
    }
}
