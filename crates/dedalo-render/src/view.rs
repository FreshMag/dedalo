//! Stage 4 of the pipeline: the audience-filtered view (spec 9.1 and 9.2).
//!
//! This stage only removes and rotates. It is the secrecy boundary: what it
//! drops never reaches a painter, the scene, the output or a plugin.

use std::collections::{BTreeMap, BTreeSet};

use dedalo_geom::{Opening, Pt, RElement, RNote, RSpace, Rect, Resolved, Wall, derive_walls};
use dedalo_model::{
    Bounds, Direction, Document, Export, Facing, Legend, Preset, Resolution, SpaceKind, Visibility,
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Dm,
    Player,
}

/// One output configuration, after presets, the document's `export` block
/// and command-line overrides have been merged.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportSpec {
    pub name: String,
    pub preset: Preset,
    /// Category toggles that override the preset.
    pub toggles: BTreeMap<String, bool>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub revealed: Vec<String>,
    pub rotation: i64,
    pub format: String,
    pub cell_px: u32,
    pub bounds: Bounds,
    /// PDF only: page size and orientation.
    pub page: dedalo_model::Page,
    pub orientation: Option<dedalo_model::Orientation>,
    /// PDF only: print one cell per inch, tiled across pages.
    pub tabletop: bool,
    pub legend: Legend,
    pub theme: Option<String>,
    pub title: Option<bool>,
    pub room_ids: bool,
    /// Keep text selectable instead of drawing it as outlines.
    pub live_text: bool,
    /// The plugins the document requires, in header order.
    pub plugins: Vec<String>,
    /// Options for plugins, by plugin ID, from `render { plugin "id" { } }`.
    pub plugin_options: BTreeMap<String, dedalo_model::Props>,
    /// A PNG rendering made by the host, for exporters that need one when
    /// this build cannot rasterise (the browser's core module).
    pub host_png: Option<Vec<u8>>,
}

impl ExportSpec {
    /// The export a preset alone describes.
    pub fn preset(preset: Preset) -> ExportSpec {
        ExportSpec {
            name: preset.as_str().to_string(),
            preset,
            toggles: BTreeMap::new(),
            include: Vec::new(),
            exclude: Vec::new(),
            revealed: Vec::new(),
            rotation: 0,
            format: "svg".to_string(),
            cell_px: 64,
            bounds: Bounds::Visible,
            page: dedalo_model::Page::Fit,
            orientation: None,
            tabletop: false,
            legend: Legend::default(),
            theme: None,
            title: None,
            room_ids: false,
            live_text: false,
            plugins: Vec::new(),
            plugin_options: BTreeMap::new(),
            host_png: None,
        }
    }

    /// The export named `name` in the document. With no name: the export
    /// called `default` when there is one, else the `dm` preset with the
    /// dungeon-level `render` and `legend` blocks.
    pub fn from_document(doc: &Document, name: Option<&str>) -> Option<ExportSpec> {
        let d = &doc.dungeon;
        let export = match name {
            Some(n) => Some(d.exports.iter().find(|e| e.name == n)?),
            None => d.exports.iter().find(|e| e.name == "default"),
        };
        let mut spec = ExportSpec::preset(export.and_then(|e| e.preset).unwrap_or(Preset::Dm));
        spec.name = export.map_or_else(|| "default".to_string(), |e| e.name.clone());
        spec.legend = d.legend.clone().unwrap_or_default();
        let mut render = d.render.clone().unwrap_or_default();
        if let Some(Export {
            legend,
            render: over,
            ..
        }) = export
        {
            if let Some(legend) = legend {
                spec.legend = legend.clone();
            }
            if let Some(over) = over {
                render.theme = over.theme.clone().or(render.theme);
                render.title = over.title.or(render.title);
                render.room_ids = over.room_ids.or(render.room_ids);
                render.text = over.text.or(render.text);
            }
        }
        spec.theme = render.theme;
        spec.title = render.title;
        spec.room_ids = render.room_ids.unwrap_or(false);
        spec.live_text = render.text == Some(dedalo_model::TextMode::Live);
        spec.plugins = doc
            .dedalo
            .iter()
            .flat_map(|h| h.plugins.iter().map(|p| p.id.clone()))
            .collect();
        spec.plugin_options = render.plugins.clone();
        if let Some(over) = export.and_then(|e| e.render.as_ref()) {
            for (id, options) in &over.plugins {
                spec.plugin_options
                    .entry(id.clone())
                    .or_default()
                    .extend(options.clone());
            }
        }
        if let Some(e) = export {
            spec.toggles = e.visibility.clone();
            spec.include = e.include.clone();
            spec.exclude = e.exclude.clone();
            spec.revealed = e.revealed.clone();
            spec.rotation = e.rotation.unwrap_or(0);
            spec.format = e.format.clone().unwrap_or_else(|| "svg".to_string());
            spec.bounds = e.bounds.unwrap_or(Bounds::Visible);
            spec.page = e.page.unwrap_or(dedalo_model::Page::Fit);
            spec.orientation = e.orientation;
            spec.tabletop = e.scale == Some(dedalo_model::PrintScale::Tabletop);
            spec.cell_px = match (e.cell_px, e.resolution) {
                (Some(px), _) => px as u32,
                (None, Some(Resolution::Low)) => 32,
                (None, Some(Resolution::High)) => 128,
                (None, _) => 64,
            };
        }
        Some(spec)
    }

    pub fn audience(&self) -> Audience {
        if self.preset == Preset::Dm {
            Audience::Dm
        } else {
            Audience::Player
        }
    }

    /// Whether a category toggle is on. The `clean` preset turns all off.
    pub fn toggle(&self, name: &str) -> bool {
        self.toggles
            .get(name)
            .copied()
            .unwrap_or(self.preset != Preset::Clean)
    }
}

/// What one audience may see, ready to paint.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct View {
    pub audience: Audience,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_unit: Option<String>,
    /// Quarter turns clockwise already applied to every coordinate.
    pub rotation: i64,
    pub cell_px: u32,
    /// Bounding box of the map content.
    pub bounds: Rect,
    pub show_grid: bool,
    pub show_labels: bool,
    pub show_legend: bool,
    pub show_frame: bool,
    pub live_text: bool,
    pub room_ids: bool,
    pub legend: Legend,
    pub seed: u64,
    pub spaces: Vec<RSpace>,
    pub walls: Vec<Wall>,
    pub openings: Vec<Opening>,
    pub elements: Vec<RElement>,
    pub notes: Vec<RNote>,
    /// For the notes panel: each container with the items it holds.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contents: Vec<(String, Vec<String>)>,
}

fn turn(p: Pt, quarters: i64) -> Pt {
    (0..quarters.rem_euclid(4)).fold(p, |q, _| Pt::new(-q.y, q.x))
}

fn turn_all(points: &mut [Pt], quarters: i64) {
    for p in points {
        *p = turn(*p, quarters);
    }
}

fn turn_rect(r: Rect, quarters: i64) -> Rect {
    Rect::of(&r.corners().map(|p| turn(p, quarters)))
}

fn turn_direction(d: Direction, quarters: i64) -> Direction {
    const ORDER: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];
    let i = ORDER.iter().position(|x| *x == d).unwrap_or(0) as i64;
    ORDER[(i + quarters).rem_euclid(4) as usize]
}

fn turn_facing(f: Facing, quarters: i64) -> Facing {
    const ORDER: [Facing; 8] = [
        Facing::North,
        Facing::NorthEast,
        Facing::East,
        Facing::SouthEast,
        Facing::South,
        Facing::SouthWest,
        Facing::West,
        Facing::NorthWest,
    ];
    let i = ORDER.iter().position(|x| *x == f).unwrap_or(0) as i64;
    ORDER[(i + 2 * quarters).rem_euclid(8) as usize]
}

/// Filters the resolved model down to what one export may show.
pub fn build_view(model: &Resolved, spec: &ExportSpec) -> View {
    let audience = spec.audience();
    let admits = |visibility: Visibility, reveal_when: &Option<String>| match visibility {
        Visibility::Public => true,
        Visibility::Dm => audience == Audience::Dm,
        Visibility::Conditional => {
            audience == Audience::Dm
                || reveal_when
                    .as_ref()
                    .is_some_and(|tag| spec.revealed.contains(tag))
        }
        Visibility::Hidden => false,
    };
    // Spec 9.2: drawn when not excluded and either included, or both the
    // visibility level admits the audience and the category toggle is on.
    let shown = |id: &str, visibility: Visibility, reveal_when: &Option<String>, toggle: bool| {
        !spec.exclude.iter().any(|x| x == id)
            && (spec.include.iter().any(|x| x == id) || (admits(visibility, reveal_when) && toggle))
    };

    // A hidden room goes together with the corridors that lead only to it.
    let mut dropped: BTreeSet<&str> = model
        .spaces
        .iter()
        .filter(|s| !shown(&s.id, s.visibility, &s.reveal_when, true))
        .map(|s| s.id.as_str())
        .collect();
    let known: BTreeSet<&str> = model.spaces.iter().map(|s| s.id.as_str()).collect();
    loop {
        let orphaned: Vec<&str> = model
            .spaces
            .iter()
            .filter(|s| s.kind == SpaceKind::Corridor && !dropped.contains(s.id.as_str()))
            .filter(|s| {
                s.connects
                    .iter()
                    .any(|c| dropped.contains(c.as_str()) || !known.contains(c.as_str()))
            })
            .map(|s| s.id.as_str())
            .collect();
        if orphaned.is_empty() {
            break;
        }
        dropped.extend(orphaned);
    }

    let quarters = spec.rotation / 90;
    let mut spaces: Vec<RSpace> = model
        .spaces
        .iter()
        .filter(|s| !dropped.contains(s.id.as_str()))
        .cloned()
        .collect();
    let mut openings: Vec<Opening> = model
        .openings
        .iter()
        .filter(|o| {
            !dropped.contains(o.from.as_str())
                && o.to.as_deref().is_none_or(|to| !dropped.contains(to))
        })
        // A hidden secret door leaves its wall unbroken.
        .filter(|o| {
            shown(
                &o.id,
                o.visibility,
                &o.reveal_when,
                !o.secret || spec.toggle("secret_doors"),
            )
        })
        .cloned()
        .collect();
    let visible_space = |id: &Option<String>| {
        id.as_deref()
            .is_some_and(|s| !dropped.contains(s) && known.contains(s))
    };
    let listed = |e: &RElement| {
        visible_space(&e.space)
            && shown(
                &e.id,
                e.visibility,
                &e.reveal_when,
                spec.toggle(&e.category),
            )
    };
    let mut elements: Vec<RElement> = model
        .elements
        .iter()
        .filter(|e| !e.footprint.is_empty() && listed(e))
        .cloned()
        .collect();

    // Items held in a container are listed under it, for those who may see both.
    let mut contents: Vec<(String, Vec<String>)> = Vec::new();
    for item in model.elements.iter().filter(|e| e.footprint.is_empty()) {
        let Some(container) = item
            .contained_in
            .as_ref()
            .and_then(|c| elements.iter().find(|e| e.id == *c))
        else {
            continue;
        };
        if !shown(
            &item.id,
            item.visibility,
            &item.reveal_when,
            spec.toggle(&item.category),
        ) {
            continue;
        }
        let holder = container
            .name
            .clone()
            .unwrap_or_else(|| container.label.clone());
        let name = item.name.clone().unwrap_or_else(|| item.label.clone());
        match contents.iter_mut().find(|(h, _)| *h == holder) {
            Some((_, items)) => items.push(name),
            None => contents.push((holder, vec![name])),
        }
    }

    let target_visible = |target: &Option<String>| match target {
        None => true,
        Some(t) => {
            spaces.iter().any(|s| s.id == *t)
                || elements.iter().any(|e| e.id == *t)
                || openings.iter().any(|o| o.id == *t)
        }
    };
    let mut notes: Vec<RNote> = model
        .notes
        .iter()
        .filter(|n| {
            target_visible(&n.target)
                && shown(&n.id, n.visibility, &n.reveal_when, spec.toggle("notes"))
        })
        .cloned()
        .collect();

    if quarters.rem_euclid(4) != 0 {
        for s in &mut spaces {
            turn_all(&mut s.polygon, quarters);
            s.bbox = Rect::of(&s.polygon);
            if let Some(c) = &mut s.circle {
                c.center = turn(c.center, quarters);
                *c = c.turned(quarters);
            }
        }
        for o in &mut openings {
            for seg in [&mut o.segment, &mut o.far].into_iter().flatten() {
                turn_all(seg, quarters);
            }
            if let Some(stub) = &mut o.stub {
                turn_all(stub, quarters);
            }
            o.cell = o.cell.map(|c| turn_rect(c, quarters));
        }
        for e in &mut elements {
            turn_all(&mut e.footprint, quarters);
            e.bbox = Rect::of(&e.footprint);
            e.rotation = (e.rotation + 90 * quarters).rem_euclid(360);
            e.wall = e.wall.map(|w| turn_direction(w, quarters));
            e.facing = e.facing.map(|f| turn_facing(f, quarters));
        }
        for n in &mut notes {
            n.anchor = n.anchor.map(|a| turn(a, quarters));
        }
    }

    // With `visible` bounds the canvas is cropped to what is drawn, so blank
    // margin does not betray hidden rooms.
    let stubs = openings
        .iter()
        .filter_map(|o| o.stub.as_deref())
        .map(Rect::of);
    let drawn = spaces
        .iter()
        .map(|s| s.bbox)
        .chain(stubs)
        .reduce(|a, b| a.union(&b));
    let bounds = match spec.bounds {
        Bounds::Full => turn_rect(model.bounds, quarters),
        Bounds::Visible => drawn.unwrap_or_default(),
    };
    let clean = spec.preset == Preset::Clean;
    View {
        audience,
        title: model.name.clone().filter(|_| spec.title.unwrap_or(!clean)),
        grid_unit: model.grid_unit.clone(),
        rotation: spec.rotation.rem_euclid(360),
        cell_px: spec.cell_px,
        bounds,
        show_grid: spec.toggle("grid"),
        show_labels: spec.toggle("room_labels"),
        show_legend: !clean && spec.legend.mode != Some(dedalo_model::LegendMode::None),
        show_frame: !clean,
        room_ids: spec.room_ids,
        live_text: spec.live_text,
        legend: spec.legend.clone(),
        seed: model.seed,
        walls: derive_walls(&spaces, &openings),
        spaces,
        openings,
        elements,
        notes,
        contents,
    }
}
