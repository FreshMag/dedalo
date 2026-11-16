//! Document model to DSL text (spec 6.3).
//!
//! The text is regenerated in canonical form: each element nested in its
//! space, aliases normalised, defaults omitted. Reading it back gives the
//! same document, comments included.

use std::collections::BTreeMap;

use dedalo_catalog::{Catalog, PropSpec, PropType};
use dedalo_syntax::ast::{is_symbol, quote};

use crate::document::*;
use crate::value::{Num, Value};

/// Writes a document as canonical DSL text.
pub fn to_dsl(doc: &Document, cat: &Catalog) -> String {
    let mut w = Writer {
        cat,
        out: String::new(),
        level: 0,
    };
    w.document(doc);
    // The formatter owns blank lines and one-line blocks, so the writer
    // only has to produce correct text.
    let parsed = dedalo_syntax::parse(&w.out);
    if parsed.errors.is_empty() {
        dedalo_syntax::fmt::format(&parsed.syntax(), &BTreeMap::new())
    } else {
        w.out
    }
}

struct Writer<'a> {
    cat: &'a Catalog,
    out: String,
    level: usize,
}

const NO_COMMENTS: &Comments = &Comments {
    leading: Vec::new(),
    trailing: None,
    end: Vec::new(),
    attrs: BTreeMap::new(),
};

fn point(p: &Point) -> String {
    format!("[{}, {}]", p[0], p[1])
}

fn symbol(text: &str) -> String {
    if is_symbol(text) {
        text.to_string()
    } else {
        quote(text)
    }
}

fn strings(items: &[String]) -> String {
    format!(
        "[{}]",
        items
            .iter()
            .map(|s| quote(s))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn shape(call: &ShapeCall) -> String {
    format!(
        "{}({})",
        call.func,
        call.args
            .iter()
            .map(|a| value(a, None, None))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// A free-form value. `ty` and `of` say how strings are spelt.
fn value(v: &Value, ty: Option<PropType>, of: Option<PropType>) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => n.to_string(),
        Value::Str(s) if ty == Some(PropType::Symbol) => symbol(s),
        Value::Str(s) => quote(s),
        Value::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|i| value(i, of, None))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Map(m) if v.is_call() => {
            let name = m.get("fn").and_then(Value::as_str).unwrap_or_default();
            let args = match m.get("args") {
                Some(Value::List(args)) => args
                    .iter()
                    .map(|a| value(a, None, None))
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => String::new(),
            };
            format!("{name}({args})")
        }
        Value::Map(m) if m.is_empty() => "{}".to_string(),
        Value::Map(m) => {
            let entries: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{} = {}", symbol(k), value(v, None, None)))
                .collect();
            format!("{{ {} }}", entries.join(", "))
        }
    }
}

impl Writer<'_> {
    fn line(&mut self, text: &str) {
        for _ in 0..self.level {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn comment_lines(&mut self, lines: &[String]) {
        for line in lines {
            self.line(line);
        }
    }

    fn with_trailing(text: String, trailing: &Option<String>) -> String {
        match trailing {
            Some(comment) => format!("{text} {comment}"),
            None => text,
        }
    }

    /// One attribute line, with the comments filed under `path`.
    fn attr(&mut self, cm: &Comments, path: &str, text: String) {
        let own = cm.attrs.get(path).unwrap_or(NO_COMMENTS);
        self.comment_lines(&own.leading);
        let name = path.rsplit('.').next().unwrap_or(path);
        self.line(&Self::with_trailing(
            format!("{name} = {text}"),
            &own.trailing,
        ));
    }

    fn opt(&mut self, cm: &Comments, path: &str, text: Option<String>) {
        if let Some(text) = text {
            self.attr(cm, path, text);
        }
    }

    /// A string attribute; multi-line text becomes a heredoc when that is exact.
    fn text_attr(&mut self, cm: &Comments, path: &str, text: &str) {
        let lines: Vec<&str> = text
            .strip_suffix('\n')
            .map(|t| t.split('\n').collect())
            .unwrap_or_default();
        let exact = !lines.is_empty()
            && !text.contains('\r')
            && !text.contains("${")
            && !text.contains("%{")
            && lines.iter().any(|l| !l.trim().is_empty())
            && lines.iter().all(|l| l.trim() != "EOT")
            && lines
                .iter()
                .filter(|l| !l.trim().is_empty())
                .any(|l| !l.starts_with([' ', '\t']));
        if !exact {
            return self.attr(cm, path, quote(text));
        }
        let own = cm.attrs.get(path).unwrap_or(NO_COMMENTS);
        self.comment_lines(&own.leading);
        let name = path.rsplit('.').next().unwrap_or(path);
        self.line(&format!("{name} = <<-EOT"));
        self.level += 1;
        for l in lines {
            if l.is_empty() {
                self.out.push('\n');
            } else {
                self.line(l);
            }
        }
        self.line(&Self::with_trailing("EOT".to_string(), &own.trailing));
        self.level -= 1;
    }

    /// Opens a block that is its own node.
    fn open(&mut self, cm: &Comments, header: &str) {
        self.comment_lines(&cm.leading);
        self.line(&format!("{header} {{"));
        self.level += 1;
    }

    fn close(&mut self, cm: &Comments) {
        self.comment_lines(&cm.end);
        self.level -= 1;
        self.line(&Self::with_trailing("}".to_string(), &cm.trailing));
    }

    /// Opens a sub-block whose comments are filed under `path` of its node.
    fn open_sub(&mut self, cm: &Comments, path: &str) {
        let own = cm.attrs.get(path).unwrap_or(NO_COMMENTS);
        self.comment_lines(&own.leading);
        let name = path.rsplit('.').next().unwrap_or(path);
        self.line(&format!("{name} {{"));
        self.level += 1;
    }

    fn close_sub(&mut self, cm: &Comments, path: &str) {
        let own = cm.attrs.get(path).unwrap_or(NO_COMMENTS);
        self.comment_lines(&own.end);
        self.level -= 1;
        self.line(&Self::with_trailing("}".to_string(), &own.trailing));
    }

    fn document(&mut self, doc: &Document) {
        if let Some(header) = &doc.dedalo {
            self.open(&header.comments, "dedalo");
            self.opt(
                &header.comments,
                "version",
                header.version.map(|v| v.to_string()),
            );
            for plugin in &header.plugins {
                self.line(&format!("plugin {} {{", quote(&plugin.id)));
                self.level += 1;
                if let Some(version) = &plugin.version {
                    self.line(&format!("version = {}", quote(version)));
                }
                self.level -= 1;
                self.line("}");
            }
            self.close(&header.comments);
        }
        self.dungeon(&doc.dungeon);
        self.comment_lines(&doc.comments.end);
    }

    fn dungeon(&mut self, d: &Dungeon) {
        let cm = &d.comments;
        self.open(cm, &format!("dungeon {}", quote(&d.id)));
        if let Some(name) = &d.name {
            self.text_attr(cm, "name", name);
        }
        if let Some(grid) = &d.grid {
            self.open_sub(cm, "grid");
            self.opt(cm, "grid.unit", grid.unit.as_deref().map(quote));
            self.opt(cm, "grid.type", grid.ty.as_deref().map(symbol));
            self.close_sub(cm, "grid");
        }
        if let Some(s) = &d.settings {
            self.open_sub(cm, "settings");
            self.opt(
                cm,
                "settings.default_wall",
                s.default_wall.as_deref().map(symbol),
            );
            self.opt(
                cm,
                "settings.default_floor",
                s.default_floor.as_deref().map(symbol),
            );
            self.opt(cm, "settings.seed", s.seed.map(|v| v.to_string()));
            self.opt(cm, "settings.max_stub", s.max_stub.map(|v| v.to_string()));
            self.close_sub(cm, "settings");
        }
        if let Some(render) = &d.render {
            self.render(cm, "render", render);
        }
        if let Some(legend) = &d.legend {
            self.legend(cm, "legend", legend);
        }
        let cat = self.cat;
        self.properties(cm, &d.properties, &|n| cat.common.properties.get(n));
        for space in &d.spaces {
            self.space(space);
        }
        for el in &d.elements {
            self.element(el);
        }
        for conn in &d.connections {
            self.connection(conn);
        }
        for note in &d.notes {
            self.note(note);
        }
        for export in &d.exports {
            self.export(export);
        }
        self.close(cm);
    }

    fn render(&mut self, cm: &Comments, path: &str, r: &RenderOptions) {
        self.open_sub(cm, path);
        self.opt(cm, &format!("{path}.theme"), r.theme.as_deref().map(symbol));
        self.opt(cm, &format!("{path}.title"), r.title.map(|v| v.to_string()));
        self.opt(
            cm,
            &format!("{path}.text"),
            r.text.map(|v| v.as_str().to_string()),
        );
        self.opt(
            cm,
            &format!("{path}.room_ids"),
            r.room_ids.map(|v| v.to_string()),
        );
        for (id, options) in &r.plugins {
            self.line(&format!("plugin {} {{", quote(id)));
            self.level += 1;
            for (name, v) in options {
                self.line(&format!("{name} = {}", value(v, None, None)));
            }
            self.level -= 1;
            self.line("}");
        }
        self.close_sub(cm, path);
    }

    fn legend(&mut self, cm: &Comments, path: &str, l: &Legend) {
        self.open_sub(cm, path);
        self.opt(
            cm,
            &format!("{path}.mode"),
            l.mode.map(|v| v.as_str().to_string()),
        );
        self.opt(
            cm,
            &format!("{path}.position"),
            l.position.map(|v| v.as_str().to_string()),
        );
        self.opt(cm, &format!("{path}.title"), l.title.as_deref().map(quote));
        for entry in &l.entries {
            self.line(&format!("entry {} {{", quote(&entry.key)));
            self.level += 1;
            if let Some(label) = &entry.label {
                self.line(&format!("label = {}", quote(label)));
            }
            self.level -= 1;
            self.line("}");
        }
        self.close_sub(cm, path);
    }

    fn properties<'s>(
        &mut self,
        cm: &Comments,
        props: &Props,
        schema: &dyn Fn(&str) -> Option<&'s PropSpec>,
    ) {
        if props.is_empty() && !cm.attrs.contains_key("properties") {
            return;
        }
        self.open_sub(cm, "properties");
        for (name, v) in props {
            let spec = schema(name);
            let path = format!("properties.{name}");
            match v {
                Value::Str(s) if spec.is_none_or(|s| s.ty == PropType::String) => {
                    self.text_attr(cm, &path, s)
                }
                _ => self.attr(
                    cm,
                    &path,
                    value(v, spec.map(|s| s.ty), spec.and_then(|s| s.of)),
                ),
            }
        }
        self.close_sub(cm, "properties");
    }

    fn flags(
        &mut self,
        cm: &Comments,
        visibility: Option<Visibility>,
        reveal_when: &Option<String>,
        tags: &[String],
    ) {
        self.opt(cm, "visibility", visibility.map(|v| v.as_str().to_string()));
        self.opt(cm, "reveal_when", reveal_when.as_deref().map(quote));
        if !tags.is_empty() {
            self.attr(cm, "tags", strings(tags));
        }
    }

    fn space(&mut self, s: &Space) {
        let cm = &s.comments;
        self.open(cm, &format!("{} {}", s.kind.as_str(), quote(&s.id)));
        if let Some(name) = &s.name {
            self.text_attr(cm, "name", name);
        }
        self.opt(cm, "shape", s.shape.as_ref().map(shape));
        self.opt(cm, "rotation", s.rotation.map(|v| v.to_string()));
        if s.kind == SpaceKind::Corridor {
            if !s.connects.is_empty() {
                self.attr(cm, "connects", strings(&s.connects));
            }
            self.opt(cm, "width", s.width.map(|v| v.to_string()));
            if !s.path.is_empty() {
                self.attr(
                    cm,
                    "path",
                    format!(
                        "[{}]",
                        s.path.iter().map(point).collect::<Vec<_>>().join(", ")
                    ),
                );
            }
            self.opt(cm, "length", s.length.map(|v| v.to_string()));
            self.opt(cm, "offset", s.offset.map(|v| v.to_string()));
        }
        self.flags(cm, s.visibility, &s.reveal_when, &s.tags);
        if let Some(l) = &s.layout {
            self.open_sub(cm, "layout");
            self.opt(cm, "layout.mode", l.mode.map(|v| v.as_str().to_string()));
            self.opt(cm, "layout.position", l.position.as_ref().map(point));
            self.opt(
                cm,
                "layout.adjacent_to",
                l.adjacent_to.as_deref().map(quote),
            );
            self.opt(
                cm,
                "layout.direction",
                l.direction.map(|v| v.as_str().to_string()),
            );
            self.opt(cm, "layout.align", l.align.map(|v| v.as_str().to_string()));
            self.opt(cm, "layout.offset", l.offset.map(|v| v.to_string()));
            self.opt(cm, "layout.gap", l.gap.map(|v| v.to_string()));
            self.close_sub(cm, "layout");
        }
        let (cat, kind) = (self.cat, s.kind.as_str());
        self.properties(cm, &s.properties, &|n| cat.structure_property(kind, n));
        for el in &s.elements {
            self.element(el);
        }
        self.close(cm);
    }

    fn element(&mut self, e: &Element) {
        let cm = &e.comments;
        self.open(cm, &format!("{} {}", e.class.as_str(), quote(&e.id)));
        self.attr(cm, "type", symbol(&e.ty));
        self.opt(cm, "subtype", e.subtype.as_deref().map(symbol));
        if let Some(name) = &e.name {
            self.text_attr(cm, "name", name);
        }
        self.opt(cm, "position", e.position.as_ref().map(point));
        self.opt(cm, "size", e.size.as_ref().map(point));
        self.opt(cm, "shape", e.shape.as_ref().map(shape));
        self.opt(cm, "rotation", e.rotation.map(|v| v.to_string()));
        self.opt(cm, "facing", e.facing.map(|v| v.as_str().to_string()));
        self.opt(cm, "reference", e.reference.as_deref().map(quote));
        self.opt(cm, "contained_in", e.contained_in.as_deref().map(quote));
        self.flags(cm, e.visibility, &e.reveal_when, &e.tags);
        if let Some(p) = &e.placement {
            self.open_sub(cm, "placement");
            self.opt(cm, "placement.wall", p.wall.map(|v| v.as_str().to_string()));
            self.opt(cm, "placement.offset", p.offset.map(|v| v.to_string()));
            self.opt(
                cm,
                "placement.anchor",
                p.anchor.map(|v| v.as_str().to_string()),
            );
            self.close_sub(cm, "placement");
        }
        let cat = self.cat;
        self.properties(cm, &e.properties, &|n| cat.property(&e.ty, n));
        self.close(cm);
    }

    fn connection(&mut self, c: &Connection) {
        let cm = &c.comments;
        self.open(cm, &format!("connection {}", quote(&c.id)));
        if let Some(name) = &c.name {
            self.text_attr(cm, "name", name);
        }
        self.attr(cm, "from", quote(&c.from));
        self.opt(cm, "to", c.to.as_deref().map(quote));
        self.attr(cm, "type", symbol(&c.ty));
        self.opt(cm, "secret", c.secret.map(|v| v.to_string()));
        self.opt(cm, "position", c.position.as_ref().map(point));
        self.flags(cm, c.visibility, &c.reveal_when, &c.tags);
        if let Some(p) = &c.placement {
            self.open_sub(cm, "placement");
            self.opt(cm, "placement.wall", p.wall.map(|v| v.as_str().to_string()));
            self.opt(cm, "placement.edge", p.edge.map(|v| v.to_string()));
            self.opt(
                cm,
                "placement.bearing",
                p.bearing.map(|v: Num| v.to_string()),
            );
            self.opt(cm, "placement.offset", p.offset.map(|v| v.to_string()));
            self.opt(cm, "placement.width", p.width.map(|v| v.to_string()));
            self.close_sub(cm, "placement");
        }
        let cat = self.cat;
        self.properties(cm, &c.properties, &|n| {
            cat.structure_property("connection", n)
        });
        self.close(cm);
    }

    fn note(&mut self, n: &Note) {
        let cm = &n.comments;
        self.open(cm, &format!("note {}", quote(&n.id)));
        self.opt(cm, "target", n.target.as_deref().map(quote));
        self.text_attr(cm, "text", &n.text);
        self.opt(cm, "position", n.position.as_ref().map(point));
        self.flags(cm, n.visibility, &n.reveal_when, &[]);
        self.close(cm);
    }

    fn export(&mut self, e: &Export) {
        let cm = &e.comments;
        let header = if e.name == "default" {
            "export".to_string()
        } else {
            format!("export {}", quote(&e.name))
        };
        self.open(cm, &header);
        self.opt(cm, "preset", e.preset.map(|v| v.as_str().to_string()));
        self.opt(cm, "format", e.format.as_deref().map(symbol));
        self.opt(
            cm,
            "resolution",
            e.resolution.map(|v| v.as_str().to_string()),
        );
        self.opt(cm, "cell_px", e.cell_px.map(|v| v.to_string()));
        self.opt(cm, "rotation", e.rotation.map(|v| v.to_string()));
        for (name, list) in [
            ("revealed", &e.revealed),
            ("include", &e.include),
            ("exclude", &e.exclude),
        ] {
            if !list.is_empty() {
                self.attr(cm, name, strings(list));
            }
        }
        self.opt(cm, "bounds", e.bounds.map(|v| v.as_str().to_string()));
        self.opt(cm, "page", e.page.map(|v| v.as_str().to_string()));
        self.opt(
            cm,
            "orientation",
            e.orientation.map(|v| v.as_str().to_string()),
        );
        if !e.visibility.is_empty() || cm.attrs.contains_key("visibility") {
            self.open_sub(cm, "visibility");
            for (toggle, on) in &e.visibility {
                self.attr(cm, &format!("visibility.{toggle}"), on.to_string());
            }
            self.close_sub(cm, "visibility");
        }
        if let Some(legend) = &e.legend {
            self.legend(cm, "legend", legend);
        }
        if let Some(render) = &e.render {
            self.render(cm, "render", render);
        }
        self.close(cm);
    }
}
