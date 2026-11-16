//! Semantic checks on a document: IDs, references, catalog types and
//! property typing. They run on the model, so DSL and JSON input get the
//! same diagnostics. Each diagnostic names its element and attribute path;
//! the source map turns that into a span when the input was DSL text.

use std::collections::BTreeMap;

use dedalo_catalog::{Catalog, Class as CatalogClass, PropSpec, PropType};

use crate::diag::{Code, Diagnostic};
use crate::document::*;
use crate::suggest::{closest, did_you_mean, one_of};
use crate::value::{Num, Value};

/// Upper bound on elements per dungeon (spec 10.3).
pub const MAX_ELEMENTS: usize = 50_000;
/// Largest coordinate, in cells. Like the other input limits it bounds
/// resource use, and it keeps integer geometry far from overflow.
pub const MAX_COORDINATE: i64 = 100_000;
/// Largest side of an element footprint and largest corridor width, in cells.
pub const MAX_EXTENT: i64 = 100;

/// Geometric numbers outside the supported range. The resolve stage does not
/// run on a document that has any.
pub fn geometry_limits(d: &Dungeon) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut range = |owner: &str, at: &str, value: Num, limit: i64| {
        if value.0.abs() > limit * 1000 {
            let message = format!(
                "`{owner}` sets `{at}` to {value}, outside the supported range of -{limit} to {limit} cells."
            );
            out.push(
                Diagnostic::new(Code::InvalidValue, message)
                    .element(owner)
                    .at(at),
            );
        }
    };
    fn numbers(value: &Value, found: &mut Vec<Num>) {
        match value {
            Value::Num(n) => found.push(*n),
            Value::List(items) => items.iter().for_each(|i| numbers(i, found)),
            Value::Map(map) => map.values().for_each(|i| numbers(i, found)),
            _ => {}
        }
    }
    if let Some(stub) = d.settings.as_ref().and_then(|s| s.max_stub) {
        range(
            &format!("dungeon:{}", d.id),
            "settings.max_stub",
            stub,
            MAX_EXTENT,
        );
    }
    for s in &d.spaces {
        let mut args = Vec::new();
        s.shape
            .iter()
            .flat_map(|c| &c.args)
            .for_each(|a| numbers(a, &mut args));
        args.into_iter()
            .for_each(|n| range(&s.id, "shape", n, MAX_COORDINATE));
        if let Some(l) = &s.layout {
            l.position
                .iter()
                .flatten()
                .for_each(|n| range(&s.id, "layout.position", *n, MAX_COORDINATE));
            l.offset
                .iter()
                .for_each(|n| range(&s.id, "layout.offset", *n, MAX_COORDINATE));
            l.gap
                .iter()
                .for_each(|n| range(&s.id, "layout.gap", *n, MAX_COORDINATE));
        }
        s.path
            .iter()
            .flatten()
            .for_each(|n| range(&s.id, "path", *n, MAX_COORDINATE));
        s.width
            .iter()
            .for_each(|n| range(&s.id, "width", *n, MAX_EXTENT));
        s.length
            .iter()
            .for_each(|n| range(&s.id, "length", *n, MAX_COORDINATE));
        s.offset
            .iter()
            .for_each(|n| range(&s.id, "offset", *n, MAX_COORDINATE));
    }
    for (_, e) in d.elements() {
        e.position
            .iter()
            .flatten()
            .for_each(|n| range(&e.id, "position", *n, MAX_COORDINATE));
        e.size
            .iter()
            .flatten()
            .for_each(|n| range(&e.id, "size", *n, MAX_EXTENT));
        let offset = e.placement.as_ref().and_then(|p| p.offset);
        offset
            .iter()
            .for_each(|n| range(&e.id, "placement.offset", *n, MAX_COORDINATE));
    }
    for c in &d.connections {
        c.position
            .iter()
            .flatten()
            .for_each(|n| range(&c.id, "position", *n, MAX_COORDINATE));
        if let Some(p) = &c.placement {
            p.offset
                .iter()
                .for_each(|n| range(&c.id, "placement.offset", *n, MAX_COORDINATE));
            p.width
                .iter()
                .for_each(|n| range(&c.id, "placement.width", *n, MAX_EXTENT));
        }
    }
    for n in &d.notes {
        n.position
            .iter()
            .flatten()
            .for_each(|v| range(&n.id, "position", *v, MAX_COORDINATE));
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Room,
    Corridor,
    Feature,
    Entity,
    Connection,
    Note,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Room => "room",
            Kind::Corridor => "corridor",
            Kind::Feature => "feature",
            Kind::Entity => "entity",
            Kind::Connection => "connection",
            Kind::Note => "note",
        }
    }

    fn is_space(self) -> bool {
        matches!(self, Kind::Room | Kind::Corridor)
    }
}

struct Checker<'a> {
    cat: &'a Catalog,
    ids: BTreeMap<&'a str, Kind>,
    out: Vec<Diagnostic>,
}

pub fn check(doc: &Document, cat: &Catalog) -> Vec<Diagnostic> {
    let d = &doc.dungeon;
    let mut c = Checker {
        cat,
        ids: BTreeMap::new(),
        out: Vec::new(),
    };

    // One ID namespace per dungeon (spec 5.6).
    let mut declared: Vec<(&str, Kind)> = Vec::new();
    for space in &d.spaces {
        declared.push((
            &space.id,
            if space.kind == SpaceKind::Room {
                Kind::Room
            } else {
                Kind::Corridor
            },
        ));
    }
    for (_, el) in d.elements() {
        declared.push((
            &el.id,
            if el.class == Class::Feature {
                Kind::Feature
            } else {
                Kind::Entity
            },
        ));
    }
    declared.extend(
        d.connections
            .iter()
            .map(|x| (x.id.as_str(), Kind::Connection)),
    );
    declared.extend(d.notes.iter().map(|x| (x.id.as_str(), Kind::Note)));
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for (id, kind) in &declared {
        let n = counts.entry(id).or_insert(0);
        *n += 1;
        match c.ids.get(id) {
            None => {
                c.ids.insert(id, *kind);
            }
            Some(first) => {
                let message = format!(
                    "The ID `{id}` is used by a {} and again by a {}. IDs share one namespace per dungeon; rename one of them.",
                    first.name(),
                    kind.name()
                );
                c.out.push(
                    Diagnostic::new(Code::DuplicateId, message)
                        .element(*id)
                        .at(format!("#{n}")),
                );
            }
        }
    }
    if declared.len() > MAX_ELEMENTS {
        c.out.push(Diagnostic::new(
            Code::InvalidValue,
            format!(
                "The dungeon declares {} elements; the limit is {MAX_ELEMENTS}.",
                declared.len()
            ),
        ));
    }

    for space in &d.spaces {
        c.space(space);
        for el in &space.elements {
            c.element(el);
        }
    }
    for el in &d.elements {
        c.element(el);
    }
    for conn in &d.connections {
        c.connection(conn);
    }
    for note in &d.notes {
        if let Some(target) = &note.target {
            c.reference(&note.id, "target", target, "any element", |_| true);
        }
    }
    c.exports(d);
    c.out.extend(geometry_limits(d));
    c.out
}

impl<'a> Checker<'a> {
    fn candidates(&self, accept: impl Fn(Kind) -> bool) -> Vec<&'a str> {
        self.ids
            .iter()
            .filter(|(_, k)| accept(**k))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Checks that `target` names an existing element of an accepted kind.
    fn reference(
        &mut self,
        owner: &str,
        at: &str,
        target: &str,
        expected: &str,
        accept: impl Fn(Kind) -> bool,
    ) -> bool {
        let message = match self.ids.get(target) {
            Some(kind) if accept(*kind) => return true,
            Some(kind) => format!(
                "`{owner}` refers to `{target}` in `{at}`, but `{target}` is a {}; {expected} is expected.",
                kind.name()
            ),
            None => {
                format!("`{owner}` refers to `{target}` in `{at}`, but no element has that ID.")
            }
        };
        let candidates = self.candidates(&accept);
        let mut d = Diagnostic::new(Code::UnknownReference, message)
            .element(owner)
            .at(at);
        if let Some(near) = closest(target, candidates.iter().copied()) {
            d = d.suggest(format!("Did you mean `{near}`?"));
        } else if !candidates.is_empty() && candidates.len() <= 12 {
            d = d.suggest(format!("Existing IDs: {}.", candidates.join(", ")));
        }
        self.out.push(d);
        false
    }

    fn invalid(&mut self, owner: &str, at: &str, message: String) {
        self.out.push(
            Diagnostic::new(Code::InvalidValue, message)
                .element(owner)
                .at(at),
        );
    }

    fn space(&mut self, s: &Space) {
        let kind = s.kind.as_str();
        if let Some(layout) = &s.layout {
            match (&layout.adjacent_to, layout.direction) {
                (Some(target), Some(_)) => {
                    if *target == s.id {
                        self.invalid(&s.id, "layout.adjacent_to", format!("Room `{}` is placed relative to itself. Name another room in `adjacent_to`.", s.id));
                    } else {
                        self.reference(&s.id, "layout.adjacent_to", target, "a room", |k| k == Kind::Room);
                    }
                }
                (Some(_), None) => self.invalid(
                    &s.id,
                    "layout.adjacent_to",
                    format!("Room `{}` sets `adjacent_to` without `direction`. Add `direction` as north, south, east or west.", s.id),
                ),
                (None, Some(_)) => self.invalid(
                    &s.id,
                    "layout.direction",
                    format!("Room `{}` sets `direction` without `adjacent_to`. Name the room it is placed next to.", s.id),
                ),
                (None, None) => {}
            }
        }
        if s.kind == SpaceKind::Corridor {
            if s.connects.len() != 2 {
                self.invalid(
                    &s.id,
                    "connects",
                    format!(
                        "Corridor `{}` connects {} spaces, but a corridor connects exactly two, for example `connects = [\"entrance\", \"guard_room\"]`. A branch is a separate corridor.",
                        s.id,
                        s.connects.len()
                    ),
                );
            } else if s.connects[0] == s.connects[1] {
                self.invalid(
                    &s.id,
                    "connects",
                    format!(
                        "Corridor `{}` connects `{}` to itself. Name two different spaces.",
                        s.id, s.connects[0]
                    ),
                );
            }
            for target in &s.connects {
                if *target == s.id {
                    self.invalid(
                        &s.id,
                        "connects",
                        format!("Corridor `{}` lists itself in `connects`.", s.id),
                    );
                } else {
                    self.reference(
                        &s.id,
                        "connects",
                        target,
                        "a room or corridor",
                        Kind::is_space,
                    );
                }
            }
            if s.width.is_some_and(|w| w.0 < 1000) {
                self.invalid(
                    &s.id,
                    "width",
                    format!(
                        "Corridor `{}` has width {}, but the minimum is 1 cell.",
                        s.id,
                        s.width.unwrap_or_default()
                    ),
                );
            }
        }
        let cat = self.cat;
        self.props(
            &s.id,
            &s.properties,
            &|n| cat.structure_property(kind, n),
            &cat.structure_property_names(kind),
            kind,
        );
    }

    fn connection(&mut self, c: &Connection) {
        self.reference(&c.id, "from", &c.from, "a room or corridor", Kind::is_space);
        if let Some(to) = &c.to {
            if *to == c.from {
                self.invalid(
                    &c.id,
                    "to",
                    format!(
                        "Connection `{}` joins `{to}` to itself. Name two different spaces.",
                        c.id
                    ),
                );
            } else {
                self.reference(&c.id, "to", to, "a room or corridor", Kind::is_space);
            }
        }
        if !CONNECTION_TYPES.contains(&c.ty.as_str()) {
            let message = format!(
                "Connection `{}` has type `{}`, which is not a connection type. The connection types are {}; `locked_door` and `secret_door` are accepted as aliases of `door`.{}",
                c.id,
                c.ty,
                one_of(CONNECTION_TYPES),
                did_you_mean(&c.ty, CONNECTION_TYPES.iter().copied())
            );
            self.out.push(
                Diagnostic::new(Code::UnknownType, message)
                    .element(c.id.clone())
                    .at("type"),
            );
        } else if c.to.is_none() && !VERTICAL_TYPES.contains(&c.ty.as_str()) {
            self.invalid(
                &c.id,
                "",
                format!("Connection `{}` of type `{}` has no `to`. Name the space it leads to; only stairs, ladders and trapdoors may omit it.", c.id, c.ty),
            );
        }
        let cat = self.cat;
        self.props(
            &c.id,
            &c.properties,
            &|n| cat.structure_property("connection", n),
            &cat.structure_property_names("connection"),
            "connection",
        );
    }

    fn element(&mut self, el: &Element) {
        let cat = self.cat;
        let Some(entry) = cat.get(&el.ty) else {
            let names: Vec<&str> = cat.types.keys().map(String::as_str).collect();
            let message = format!(
                "`{}` has type `{}`, which is not in the catalog.{} The types are {}. Use `type = custom` for anything else.",
                el.id,
                el.ty,
                did_you_mean(&el.ty, names.iter().copied()),
                names.join(", ")
            );
            self.out.push(
                Diagnostic::new(Code::UnknownType, message)
                    .element(el.id.clone())
                    .at("type"),
            );
            return;
        };
        let wanted = match entry.class {
            CatalogClass::Feature => Some(Class::Feature),
            CatalogClass::Entity => Some(Class::Entity),
            CatalogClass::Either => None,
        };
        if let Some(wanted) = wanted.filter(|w| *w != el.class) {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "`{}` is written as `{}`, but type `{}` is always a {}. The class of a type is fixed by the catalog.",
                    el.id,
                    el.class.as_str(),
                    el.ty,
                    wanted.as_str()
                ),
            )
            .element(el.id.clone())
            .suggest(format!("Change the block keyword: `{} \"{}\" {{ ... }}`.", wanted.as_str(), el.id));
            self.out.push(d);
        }
        if let Some(subtype) = &el.subtype
            && !entry.subtypes.is_empty()
            && !entry.subtypes.contains(subtype)
        {
            let names: Vec<&str> = entry.subtypes.iter().map(String::as_str).collect();
            let message = format!(
                "`{}` has subtype `{subtype}`, which type `{}` does not define.{} The subtypes of `{}` are {}.",
                el.id,
                el.ty,
                did_you_mean(subtype, names.iter().copied()),
                el.ty,
                names.join(", ")
            );
            self.out.push(
                Diagnostic::new(Code::UnknownType, message)
                    .element(el.id.clone())
                    .at("subtype"),
            );
        }
        if let Some(container) = &el.contained_in {
            self.reference(&el.id, "contained_in", container, "a feature", |k| {
                k == Kind::Feature
            });
        }
        if let Some(size) = el.size
            && size.iter().any(|n| n.0 <= 0)
        {
            self.invalid(
                &el.id,
                "size",
                format!(
                    "`{}` has size [{}, {}], but both sides must be positive.",
                    el.id, size[0], size[1]
                ),
            );
        }
        self.props(
            &el.id,
            &el.properties,
            &|n| cat.property(&el.ty, n),
            &cat.property_names(&el.ty),
            &el.ty,
        );
    }

    fn props(
        &mut self,
        owner: &str,
        props: &Props,
        schema: &dyn Fn(&str) -> Option<&'a PropSpec>,
        known: &[&str],
        of: &str,
    ) {
        for (name, value) in props {
            // Names starting with `x_` are custom data, always accepted.
            if name.starts_with("x_") || *value == Value::Null {
                continue;
            }
            let at = format!("properties.{name}");
            let Some(spec) = schema(name) else {
                let message = format!(
                    "`{owner}` sets the property `{name}`, which `{of}` does not define.{} The properties of `{of}` are {}. Prefix a name with `x_` to keep custom data.",
                    did_you_mean(name, known.iter().copied()),
                    known.join(", ")
                );
                self.out.push(
                    Diagnostic::new(Code::UnknownProperty, message)
                        .element(owner)
                        .at(at),
                );
                continue;
            };
            self.prop_value(owner, &at, name, spec, spec.ty, value);
        }
    }

    fn prop_value(
        &mut self,
        owner: &str,
        at: &str,
        name: &str,
        spec: &PropSpec,
        ty: PropType,
        value: &Value,
    ) {
        let ok = match (ty, value) {
            (PropType::String, Value::Str(_))
            | (PropType::Boolean, Value::Bool(_))
            | (PropType::Number, Value::Num(_)) => true,
            (PropType::Integer, Value::Num(n)) => n.is_int(),
            (PropType::Symbol, Value::Str(s)) => {
                if let Some(values) = &spec.values
                    && !values.contains(s)
                {
                    let names: Vec<&str> = values.iter().map(String::as_str).collect();
                    self.invalid(
                        owner,
                        at,
                        format!(
                            "Property `{name}` of `{owner}` is `{s}`, but must be {}.{}",
                            one_of(&names),
                            did_you_mean(s, names.iter().copied())
                        ),
                    );
                }
                true
            }
            (PropType::Reference, Value::Str(target)) => {
                self.reference(owner, at, target, "any element", |_| true);
                true
            }
            (PropType::Shape, v) => v.is_call(),
            (PropType::Point, Value::List(items)) => {
                items.len() == 2 && items.iter().all(|i| matches!(i, Value::Num(_)))
            }
            (PropType::List, Value::List(items)) => {
                if let Some(of) = spec.of {
                    for item in items {
                        self.prop_value(owner, at, name, spec, of, item);
                    }
                }
                true
            }
            _ => false,
        };
        if !ok {
            let message = format!(
                "Property `{name}` of `{owner}` expects {}, but found {}.",
                ty.describe(),
                value.describe()
            );
            self.out.push(
                Diagnostic::new(Code::TypeMismatch, message)
                    .element(owner)
                    .at(at),
            );
            return;
        }
        if let Value::Num(n) = value {
            let v = n.to_f64();
            if spec.min.is_some_and(|min| v < min) || spec.max.is_some_and(|max| v > max) {
                let range = match (spec.min, spec.max) {
                    (Some(min), Some(max)) => format!("between {min} and {max}"),
                    (Some(min), None) => format!("at least {min}"),
                    (None, Some(max)) => format!("at most {max}"),
                    (None, None) => String::new(),
                };
                self.invalid(
                    owner,
                    at,
                    format!("Property `{name}` of `{owner}` is {n}, but must be {range}."),
                );
            }
        }
    }

    fn exports(&mut self, d: &Dungeon) {
        let mut names: Vec<&str> = Vec::new();
        for ex in &d.exports {
            let owner = format!("export:{}", ex.name);
            if names.contains(&ex.name.as_str()) {
                let message = format!(
                    "Two `export` blocks are named `{}`. Give each export its own name; an unnamed export is called `default`.",
                    ex.name
                );
                self.out
                    .push(Diagnostic::new(Code::DuplicateId, message).element(owner.clone()));
            }
            names.push(&ex.name);
            if let Some(format) = &ex.format
                && !FORMATS.contains(&format.as_str())
                && !format.contains('.')
            {
                self.invalid(
                    &owner,
                    "format",
                    format!(
                        "Export `{}` has format `{format}`, but the formats are {}; plugin formats are dotted, such as `uvtt.dd2vtt`.{}",
                        ex.name,
                        one_of(FORMATS),
                        did_you_mean(format, FORMATS.iter().copied())
                    ),
                );
            }
            for (field, list) in [("include", &ex.include), ("exclude", &ex.exclude)] {
                for id in list {
                    if !self.ids.contains_key(id.as_str()) {
                        let candidates = self.candidates(|_| true);
                        let message = format!(
                            "Export `{}` lists `{id}` in `{field}`, but no element has that ID.{}",
                            ex.name,
                            did_you_mean(id, candidates)
                        );
                        self.out.push(
                            Diagnostic::new(Code::UnknownReference, message)
                                .element(owner.clone())
                                .at(field),
                        );
                    }
                }
            }
        }
    }
}
