//! Stage 2 of the pipeline: syntax tree to document model.
//!
//! Lowering is directed by the schema of each block (spec 5.4 to 5.10). It
//! reports every problem it finds and keeps going, so one bad attribute does
//! not hide the others. It also records, for the formatter, the canonical
//! spelling of values written leniently.

use std::collections::BTreeMap;

use dedalo_catalog::{Catalog, PropSpec, PropType};
use dedalo_syntax::ast::{self, Attribute, Block, Expr, Item};
use dedalo_syntax::text::LineIndex;
use dedalo_syntax::{NodeOrToken, SyntaxKind, SyntaxNode, TextRange};

use crate::diag::{Code, Diagnostic, SourceMap, Span};
use crate::document::*;
use crate::suggest::{did_you_mean, one_of};
use crate::value::{Num, Value};

/// The result of lowering one file.
pub struct Lowered {
    /// Absent when the file has no `dungeon` block.
    pub document: Option<Document>,
    pub diagnostics: Vec<Diagnostic>,
    /// Canonical spellings for the formatter, by start offset.
    pub rewrites: BTreeMap<u32, String>,
    pub source_map: SourceMap,
}

pub fn lower(root: &SyntaxNode, source: &str, catalog: &Catalog) -> Lowered {
    lower_with(root, source, catalog, &[])
}

/// Lowers with templates supplied by packs, as `(name, template block)`.
/// Their names are namespaced by the plugin, such as `doors.iron`.
pub fn lower_with(
    root: &SyntaxNode,
    source: &str,
    catalog: &Catalog,
    external: &[(String, SyntaxNode)],
) -> Lowered {
    let mut templates = BTreeMap::new();
    for (name, node) in external {
        if node.kind() == SyntaxKind::Block {
            templates.insert(name.clone(), (Block(node.clone()), true));
        }
    }
    let mut l = Lower {
        templates,
        template_target: None,
        cat: catalog,
        lines: LineIndex::new(source),
        diags: Vec::new(),
        rewrites: BTreeMap::new(),
        map: SourceMap::default(),
        seen: BTreeMap::new(),
        block_comments: BTreeMap::new(),
        notes: Vec::new(),
        loose: Vec::new(),
        offsets: BTreeMap::new(),
    };
    let document = l.file(root);
    Lowered {
        document,
        diagnostics: l.diags,
        rewrites: l.rewrites,
        source_map: l.map,
    }
}

const DUNGEON_BLOCKS: &[&str] = &[
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
];
const SPACE_FLAGS: &[&str] = &["name", "visibility", "reveal_when", "tags"];
const ROOM_ATTRS: &[&str] = &[
    "name",
    "shape",
    "rotation",
    "visibility",
    "reveal_when",
    "tags",
];
const CORRIDOR_ATTRS: &[&str] = &[
    "name",
    "connects",
    "width",
    "path",
    "length",
    "offset",
    "visibility",
    "reveal_when",
    "tags",
];
const LAYOUT_ATTRS: &[&str] = &[
    "mode",
    "position",
    "adjacent_to",
    "direction",
    "north_of",
    "south_of",
    "east_of",
    "west_of",
    "align",
    "offset",
    "gap",
];
const RESERVED_RELATIONS: &[&str] = &["near", "far_from", "inside", "outside", "above", "below"];
const CONNECTION_ATTRS: &[&str] = &[
    "name",
    "from",
    "to",
    "type",
    "secret",
    "position",
    "visibility",
    "reveal_when",
    "tags",
];
const CONNECTION_PLACEMENT: &[&str] = &["wall", "edge", "bearing", "offset", "width"];
const ELEMENT_ATTRS: &[&str] = &[
    "type",
    "subtype",
    "name",
    "position",
    "size",
    "shape",
    "rotation",
    "facing",
    "visibility",
    "reveal_when",
    "reference",
    "contained_in",
    "room",
    "corridor",
    "tags",
];
const ELEMENT_PLACEMENT: &[&str] = &["wall", "offset", "anchor"];
const NOTE_ATTRS: &[&str] = &[
    "text",
    "target",
    "room",
    "position",
    "visibility",
    "reveal_when",
];
const EXPORT_ATTRS: &[&str] = &[
    "preset",
    "format",
    "resolution",
    "cell_px",
    "rotation",
    "revealed",
    "include",
    "exclude",
    "bounds",
    "page",
    "orientation",
    "scale",
];
const RENDER_ATTRS: &[&str] = &["theme", "title", "text", "room_ids"];
const LEGEND_ATTRS: &[&str] = &["mode", "position", "title"];

/// The node a block's items belong to, for source-map paths.
struct Cx {
    id: String,
    prefix: String,
}

impl Cx {
    fn node(id: &str) -> Cx {
        Cx {
            id: id.to_string(),
            prefix: String::new(),
        }
    }
}

enum It {
    Attr(String, Attribute),
    Block(String, Block),
}

struct Lower<'a> {
    cat: &'a Catalog,
    lines: LineIndex<'a>,
    diags: Vec<Diagnostic>,
    rewrites: BTreeMap<u32, String>,
    map: SourceMap,
    seen: BTreeMap<String, u32>,
    /// Comments of blocks not yet lowered, by start offset.
    block_comments: BTreeMap<u32, Comments>,
    notes: Vec<Note>,
    /// Elements that name their space by attribute: space ID and element.
    loose: Vec<(String, Element)>,
    /// Source offset of each element, to keep file order inside a space.
    offsets: BTreeMap<String, u32>,
    /// Templates by name, and whether each comes from a pack.
    templates: BTreeMap<String, (Block, bool)>,
    /// While a template is lowered for an element: that element's ID.
    template_target: Option<String>,
}

/// Deep-merges `own` over `base`: own values win, maps merge, and the
/// comments stay the element's own.
fn merge(base: serde_json::Value, own: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match (base, own) {
        (Value::Object(mut base), Value::Object(own)) => {
            base.remove("comments");
            for (k, v) in own {
                let merged = match base.remove(&k) {
                    Some(b) if k != "comments" => merge(b, v),
                    _ => v,
                };
                base.insert(k, merged);
            }
            Value::Object(base)
        }
        // A required string the block left empty is unset, not empty.
        (base, Value::String(own)) if own.is_empty() => base,
        (_, own) => own,
    }
}

/// Template bodies hold attributes and the sub-blocks of one element: no
/// elements of their own, which would be duplicated on every use.
const TEMPLATE_FORBIDDEN: &[&str] = &[
    "room",
    "corridor",
    "connection",
    "feature",
    "entity",
    "note",
    "export",
    "template",
];

fn start(node: &SyntaxNode) -> u32 {
    node.text_range().start().into()
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && id.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn normalise_id(id: &str) -> String {
    let mut out: String = id
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if !out.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
        out.insert(0, 'x');
    }
    out
}

/// The comments of each item of a body, and those after the last item.
struct Scan {
    items: BTreeMap<u32, Comments>,
    end: Vec<String>,
}

fn scan_comments(parent: &SyntaxNode) -> Scan {
    let mut items: BTreeMap<u32, Comments> = BTreeMap::new();
    let mut pending: Vec<String> = Vec::new();
    let mut last: Option<u32> = None;
    let mut same_line = false;
    for el in parent.children_with_tokens() {
        match el {
            NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::Newline => same_line = false,
                SyntaxKind::Comment => {
                    let text = t.text().trim_end().to_string();
                    let slot = last
                        .filter(|_| same_line)
                        .and_then(|s| items.get_mut(&s))
                        .filter(|c| c.trailing.is_none());
                    match slot {
                        Some(c) => c.trailing = Some(text),
                        None => pending.push(text),
                    }
                }
                _ => {}
            },
            NodeOrToken::Node(n)
                if matches!(n.kind(), SyntaxKind::Attribute | SyntaxKind::Block) =>
            {
                let mut leading = std::mem::take(&mut pending);
                let inner: Vec<NodeOrToken<SyntaxNode, dedalo_syntax::SyntaxToken>> =
                    if n.kind() == SyntaxKind::Block {
                        n.children_with_tokens().collect()
                    } else {
                        n.descendants_with_tokens().collect()
                    };
                leading.extend(
                    inner
                        .into_iter()
                        .filter_map(NodeOrToken::into_token)
                        .filter(|t| t.kind() == SyntaxKind::Comment)
                        .map(|t| t.text().trim_end().to_string()),
                );
                items.insert(
                    start(&n),
                    Comments {
                        leading,
                        ..Comments::default()
                    },
                );
                last = Some(start(&n));
                same_line = true;
            }
            NodeOrToken::Node(_) => {}
        }
    }
    Scan {
        items,
        end: pending,
    }
}

impl Lower<'_> {
    // ----- diagnostics -------------------------------------------------

    fn push(&mut self, cx: &Cx, range: TextRange, d: Diagnostic) {
        let mut d = d.span(Span::from_range(range, &self.lines));
        if !cx.id.is_empty() && !cx.id.contains(':') {
            d = d.element(cx.id.clone());
        }
        self.diags.push(d);
    }

    fn mismatch(&mut self, cx: &Cx, name: &str, expected: &str, e: &Expr) {
        let d = Diagnostic::new(
            Code::TypeMismatch,
            format!("`{name}` expects {expected}, but found {}.", e.describe()),
        );
        self.push(cx, e.range(), d);
    }

    fn unknown_attr(
        &mut self,
        cx: &Cx,
        a: &Attribute,
        block: &str,
        known: &[&str],
        has_properties: bool,
    ) {
        let name = a.name_text();
        let mut message = format!(
            "`{name}` is not an attribute of `{block}`.{} The attributes of `{block}` are {}.",
            did_you_mean(&name, known.iter().copied()),
            known.join(", ")
        );
        if has_properties {
            message.push_str(" Game and descriptive data go in a `properties { }` block.");
        }
        // A catalog property written as an attribute is the commonest slip.
        let owner = if !has_properties {
            None
        } else if matches!(block, "room" | "corridor" | "connection") {
            self.cat
                .structure_property(block, &name)
                .map(|_| block.to_string())
        } else {
            let ty = a.0.parent().and_then(|body| {
                ast::items(&body).find_map(|item| match item {
                    Item::Attr(t) if t.name_text() == "type" => t.value()?.symbol_text(),
                    _ => None,
                })
            });
            ty.filter(|t| self.cat.property(t, &name).is_some())
        };
        let mut diagnostic = Diagnostic::new(Code::UnknownAttribute, message);
        if let Some(owner) = owner {
            let value = a
                .value()
                .map(|v| v.node().text().to_string())
                .unwrap_or_default();
            diagnostic.message = format!(
                "`{name}` is a property of `{owner}`, not an attribute of the `{block}` block. Properties are written inside a `properties {{ }}` block."
            );
            diagnostic = diagnostic.suggest(format!(
                "Replace the line with `properties {{ {name} = {value} }}`, or move it into the existing `properties` block."
            ));
        }
        let range = a.name().map_or(a.0.text_range(), |t| t.text_range());
        self.push(cx, range, diagnostic);
    }

    fn unknown_block(&mut self, cx: &Cx, b: &Block, parent: &str, known: &[&str]) {
        let keyword = b.keyword_text();
        let message = if keyword == "template" || keyword == "level" {
            format!(
                "`{keyword}` blocks are reserved for a later release and are not supported yet."
            )
        } else if known.is_empty() {
            format!("`{parent}` does not take nested blocks, but found `{keyword}`.")
        } else {
            format!(
                "`{keyword}` is not a block allowed in `{parent}`.{} The blocks allowed in `{parent}` are {}.",
                did_you_mean(&keyword, known.iter().copied()),
                known.join(", ")
            )
        };
        self.push(
            cx,
            b.header_range(),
            Diagnostic::new(Code::UnknownBlock, message),
        );
    }

    // ----- items and comments -------------------------------------------

    /// Reads the items of a body: rejects attributes set twice, records where
    /// each attribute is, and files the comments under the owning node.
    fn items(&mut self, cx: &Cx, parent: &SyntaxNode, cm: &mut Comments) -> Vec<It> {
        let scan = scan_comments(parent);
        let mut out = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for item in ast::items(parent) {
            match item {
                Item::Attr(a) => {
                    let name = a.name_text();
                    if names.contains(&name) {
                        let d = Diagnostic::new(
                            Code::InvalidValue,
                            format!("`{name}` is set twice in this block; keep one of the two."),
                        );
                        self.push(cx, a.0.text_range(), d);
                        continue;
                    }
                    names.push(name.clone());
                    let path = format!("{}{name}", cx.prefix);
                    self.map.insert(
                        &cx.id,
                        &path,
                        a.value().map_or(a.0.text_range(), |v| v.range()),
                    );
                    if let Some(c) = scan.items.get(&start(&a.0)).filter(|c| !c.is_empty()) {
                        cm.attrs.insert(path, c.clone());
                    }
                    if a.value().is_some() {
                        out.push(It::Attr(name, a));
                    }
                }
                Item::Block(b) => {
                    if let Some(c) = scan.items.get(&start(&b.0)) {
                        self.block_comments.insert(start(&b.0), c.clone());
                    }
                    out.push(It::Block(b.keyword_text(), b));
                }
            }
        }
        if !scan.end.is_empty() {
            if cx.prefix.is_empty() {
                cm.end = scan.end;
            } else {
                cm.attrs
                    .entry(cx.prefix.trim_end_matches('.').to_string())
                    .or_default()
                    .end = scan.end;
            }
        }
        out
    }

    /// Opens a sub-block that is part of its parent node, such as `layout`.
    fn sub(&mut self, cx: &Cx, key: &str, b: &Block, cm: &mut Comments) -> (Cx, Vec<It>) {
        let path = format!("{}{key}", cx.prefix);
        self.map.insert(&cx.id, &path, b.header_range());
        if let Some(c) = self
            .block_comments
            .remove(&start(&b.0))
            .filter(|c| !c.is_empty())
        {
            cm.attrs.insert(path.clone(), c);
        }
        if let Some(label) = b.labels().first() {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!("A `{key}` block takes no label; remove {}.", label.text()),
            );
            self.push(cx, label.text_range(), d);
        }
        let inner = Cx {
            id: cx.id.clone(),
            prefix: format!("{path}."),
        };
        let items = match b.body() {
            Some(body) => self.items(&inner, &body, cm),
            None => Vec::new(),
        };
        (inner, items)
    }

    fn node_comments(&mut self, b: &Block) -> Comments {
        self.block_comments.remove(&start(&b.0)).unwrap_or_default()
    }

    /// Reads the ID label of a block and registers it.
    fn id_label(&mut self, b: &Block, example: &str) -> Option<String> {
        let labels = b.labels();
        let keyword = b.keyword_text();
        let cx = Cx::node("");
        let Some(token) = labels.first() else {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "A `{keyword}` block needs an ID label, for example `{keyword} \"{example}\" {{ }}`."
                ),
            );
            self.push(&cx, b.header_range(), d);
            return None;
        };
        if let Some(target) = &self.template_target {
            return Some(target.clone());
        }
        if let Some(extra) = labels.get(1) {
            let using = extra.text() == "using"
                && labels.len() == 3
                && matches!(
                    keyword.as_str(),
                    "room" | "corridor" | "connection" | "feature" | "entity"
                );
            if !using {
                let message = if extra.text() == "using" {
                    format!(
                        "`using` takes one template name, for example `{keyword} \"id\" using \"standard\" {{ }}`; rooms, corridors, connections, features and entities can use templates."
                    )
                } else {
                    format!(
                        "A `{keyword}` block takes a single ID label; remove {}.",
                        extra.text()
                    )
                };
                self.push(
                    &cx,
                    extra.text_range(),
                    Diagnostic::new(Code::InvalidValue, message),
                );
            }
        }
        let id = ast::label_text(token);
        if token.kind() == SyntaxKind::Ident {
            self.rewrites
                .insert(token.text_range().start().into(), ast::quote(&id));
        }
        if !valid_id(&id) {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!("`{id}` is not a valid ID. An ID starts with a lowercase letter and continues with lowercase letters, digits or underscores, up to 64 characters."),
            )
            .suggest(format!("Use `{}`.", normalise_id(&id)));
            self.push(&cx, token.text_range(), d);
        }
        let count = self.seen.entry(id.clone()).or_insert(0);
        *count += 1;
        let path = if *count == 1 {
            String::new()
        } else {
            format!("#{count}")
        };
        self.map.insert(&id, &path, token.text_range());
        Some(id)
    }

    // ----- values -----------------------------------------------------------

    /// The value of an attribute, or nothing when it is `null`.
    fn val(&self, a: &Attribute) -> Option<Expr> {
        a.value().filter(|e| !e.is_null())
    }

    fn e_string(&mut self, cx: &Cx, name: &str, e: &Expr) -> Option<String> {
        let text = match e.literal_token() {
            Some(t) if t.kind() == SyntaxKind::Str => ast::unquote(t.text()),
            Some(t) if t.kind() == SyntaxKind::Heredoc => {
                let value = ast::heredoc_value(t.text());
                if value.contains("${") || value.contains("%{") {
                    Err("`${` and `%{` start a template, which Dedalo language version 1 does not support. Remove it or rephrase the text.".to_string())
                } else {
                    Ok(value)
                }
            }
            _ => {
                self.mismatch(cx, name, "a quoted string", e);
                return None;
            }
        };
        match text {
            Ok(s) => Some(s),
            Err(message) => {
                self.push(cx, e.range(), Diagnostic::new(Code::SyntaxError, message));
                None
            }
        }
    }

    /// A symbol. Lenient: a quoted string is accepted and marked for rewrite.
    fn e_symbol(&mut self, cx: &Cx, name: &str, e: &Expr) -> Option<String> {
        if let Some(s) = e.symbol_text() {
            return Some(s);
        }
        if let Some(t) = e.literal_token()
            && t.kind() == SyntaxKind::Str
            && let Ok(s) = ast::unquote(t.text())
            && ast::is_symbol(&s)
        {
            self.rewrites.insert(start(e.node()), s.clone());
            return Some(s);
        }
        self.mismatch(cx, name, "a symbol such as `door`", e);
        None
    }

    /// A reference. Lenient: a bare identifier is accepted and marked for rewrite.
    fn e_ref(&mut self, cx: &Cx, name: &str, e: &Expr) -> Option<String> {
        if let Some(t) = e.literal_token()
            && t.kind() == SyntaxKind::Str
        {
            return self.e_string(cx, name, e);
        }
        if let Some(s) = e.symbol_text().filter(|s| !s.contains('.')) {
            self.rewrites.insert(start(e.node()), ast::quote(&s));
            return Some(s);
        }
        self.mismatch(cx, name, "a quoted element ID such as \"entrance\"", e);
        None
    }

    fn e_num(&mut self, cx: &Cx, name: &str, e: &Expr) -> Option<Num> {
        match e.literal_token() {
            Some(t) if t.kind() == SyntaxKind::Number => match Num::parse(t.text()) {
                Ok(n) => Some(n),
                Err(message) => {
                    self.push(cx, e.range(), Diagnostic::new(Code::InvalidValue, message));
                    None
                }
            },
            _ => {
                self.mismatch(cx, name, "a number", e);
                None
            }
        }
    }

    fn e_point(&mut self, cx: &Cx, name: &str, e: &Expr) -> Option<Point> {
        if let Expr::List(_) = e {
            let parts = e.elements();
            if parts.len() == 2 {
                let x = self.e_num(cx, name, &parts[0]);
                let y = self.e_num(cx, name, &parts[1]);
                return Some([x?, y?]);
            }
        }
        self.mismatch(cx, name, "a point such as [3, 2]", e);
        None
    }

    fn e_list<T>(
        &mut self,
        cx: &Cx,
        name: &str,
        e: &Expr,
        each: fn(&mut Self, &Cx, &str, &Expr) -> Option<T>,
    ) -> Option<Vec<T>> {
        if let Expr::List(_) = e {
            return Some(
                e.elements()
                    .iter()
                    .filter_map(|item| each(self, cx, name, item))
                    .collect(),
            );
        }
        self.mismatch(cx, name, "a list such as [a, b]", e);
        None
    }

    /// Any value, for `properties` and shape arguments.
    fn value(&mut self, cx: &Cx, name: &str, e: &Expr) -> Value {
        match e {
            Expr::Literal(_) => match e.literal_token() {
                Some(t) if t.kind() == SyntaxKind::Number => {
                    self.e_num(cx, name, e).map_or(Value::Null, Value::Num)
                }
                Some(t) if t.kind() == SyntaxKind::Ident => match t.text() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => Value::Null,
                },
                _ => self.e_string(cx, name, e).map_or(Value::Null, Value::Str),
            },
            Expr::Symbol(_) => Value::Str(e.symbol_text().unwrap_or_default()),
            Expr::List(_) => Value::List(
                e.elements()
                    .iter()
                    .map(|item| self.value(cx, name, item))
                    .collect(),
            ),
            Expr::Object(_) => Value::Map(
                e.entries()
                    .into_iter()
                    .filter_map(|(key, value)| {
                        Some((ast::label_text(&key), self.value(cx, name, &value?)))
                    })
                    .collect(),
            ),
            Expr::Call(_) => Value::Map(BTreeMap::from([
                (
                    "fn".to_string(),
                    Value::Str(e.call_name().unwrap_or_default()),
                ),
                (
                    "args".to_string(),
                    Value::List(
                        e.elements()
                            .iter()
                            .map(|item| self.value(cx, name, item))
                            .collect(),
                    ),
                ),
            ])),
        }
    }

    // Attribute-level getters: they skip `null`, which unsets an attribute.

    fn string(&mut self, cx: &Cx, a: &Attribute) -> Option<String> {
        let e = self.val(a)?;
        self.e_string(cx, &a.name_text(), &e)
    }

    fn symbol(&mut self, cx: &Cx, a: &Attribute) -> Option<String> {
        let e = self.val(a)?;
        self.e_symbol(cx, &a.name_text(), &e)
    }

    /// A string or a symbol, stored as text: tags and reveal conditions.
    fn word(&mut self, cx: &Cx, a: &Attribute) -> Option<String> {
        let e = self.val(a)?;
        Self::e_word(self, cx, &a.name_text(), &e)
    }

    fn e_word(&mut self, cx: &Cx, name: &str, e: &Expr) -> Option<String> {
        match e.symbol_text() {
            Some(s) => Some(s),
            None => self.e_string(cx, name, e),
        }
    }

    fn reference(&mut self, cx: &Cx, a: &Attribute) -> Option<String> {
        let e = self.val(a)?;
        self.e_ref(cx, &a.name_text(), &e)
    }

    fn num(&mut self, cx: &Cx, a: &Attribute) -> Option<Num> {
        let e = self.val(a)?;
        self.e_num(cx, &a.name_text(), &e)
    }

    fn int(&mut self, cx: &Cx, a: &Attribute) -> Option<i64> {
        let e = self.val(a)?;
        let n = self.e_num(cx, &a.name_text(), &e)?;
        if n.as_int().is_none() {
            self.mismatch(cx, &a.name_text(), "a whole number", &e);
        }
        n.as_int()
    }

    fn boolean(&mut self, cx: &Cx, a: &Attribute) -> Option<bool> {
        let e = self.val(a)?;
        match e
            .literal_token()
            .filter(|t| t.kind() == SyntaxKind::Ident)
            .map(|t| t.text().to_string())
            .as_deref()
        {
            Some("true") => Some(true),
            Some("false") => Some(false),
            _ => {
                self.mismatch(cx, &a.name_text(), "true or false", &e);
                None
            }
        }
    }

    fn point(&mut self, cx: &Cx, a: &Attribute) -> Option<Point> {
        let e = self.val(a)?;
        self.e_point(cx, &a.name_text(), &e)
    }

    fn list<T>(
        &mut self,
        cx: &Cx,
        a: &Attribute,
        each: fn(&mut Self, &Cx, &str, &Expr) -> Option<T>,
    ) -> Vec<T> {
        let Some(e) = self.val(a) else {
            return Vec::new();
        };
        self.e_list(cx, &a.name_text(), &e, each)
            .unwrap_or_default()
    }

    fn shape(&mut self, cx: &Cx, a: &Attribute) -> Option<ShapeCall> {
        let e = self.val(a)?;
        let name = a.name_text();
        if let Expr::Call(_) = e {
            let args = e
                .elements()
                .iter()
                .map(|item| self.value(cx, &name, item))
                .collect();
            return Some(ShapeCall {
                func: e.call_name().unwrap_or_default(),
                args,
            });
        }
        self.mismatch(cx, &name, "a shape such as rectangle(8, 6)", &e);
        None
    }

    fn choice<T>(
        &mut self,
        cx: &Cx,
        a: &Attribute,
        parse: fn(&str) -> Option<T>,
        names: &[&str],
    ) -> Option<T> {
        let e = self.val(a)?;
        let name = a.name_text();
        let text = self.e_symbol(cx, &name, &e)?;
        let parsed = parse(&text);
        if parsed.is_none() {
            let message = format!(
                "`{name}` is `{text}`, but must be {}.{}",
                one_of(names),
                did_you_mean(&text, names.iter().copied())
            );
            self.push(cx, e.range(), Diagnostic::new(Code::InvalidValue, message));
        }
        parsed
    }

    fn quarter_turn(&mut self, cx: &Cx, a: &Attribute) -> Option<i64> {
        let value = self.int(cx, a)?;
        if ![0, 90, 180, 270].contains(&value) {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "`rotation` is {value}, but must be 0, 90, 180 or 270 degrees. Arbitrary angles arrive after 1.0."
                ),
            );
            self.push(cx, a.0.text_range(), d);
            return None;
        }
        Some(value)
    }

    /// A `properties` block. Values are typed by the catalog in the check
    /// pass; here only the lenient spellings are recorded.
    fn properties(
        &mut self,
        cx: &Cx,
        b: &Block,
        cm: &mut Comments,
        props: &mut Props,
        schema: &dyn Fn(&str) -> Option<PropSpec>,
    ) {
        let (inner, items) = self.sub(cx, "properties", b, cm);
        for item in items {
            match item {
                It::Attr(name, a) => {
                    let Some(e) = a.value() else { continue };
                    if let Some(spec) = schema(&name) {
                        self.property_spelling(&spec, spec.ty, &e);
                    }
                    let value = self.value(&inner, &name, &e);
                    props.insert(name, value);
                }
                It::Block(_, blk) => self.unknown_block(&inner, &blk, "properties", &[]),
            }
        }
    }

    fn property_spelling(&mut self, spec: &PropSpec, ty: PropType, e: &Expr) {
        match ty {
            PropType::Symbol => {
                if let Some(t) = e.literal_token()
                    && t.kind() == SyntaxKind::Str
                    && let Ok(s) = ast::unquote(t.text())
                    && ast::is_symbol(&s)
                {
                    self.rewrites.insert(start(e.node()), s);
                }
            }
            PropType::Reference => {
                if let Some(s) = e.symbol_text() {
                    self.rewrites.insert(start(e.node()), ast::quote(&s));
                }
            }
            PropType::List => {
                if let (Some(of), Expr::List(_)) = (spec.of, e) {
                    for item in e.elements() {
                        self.property_spelling(spec, of, &item);
                    }
                }
            }
            _ => {}
        }
    }

    fn visibility_attr(
        &mut self,
        cx: &Cx,
        name: &str,
        a: &Attribute,
        visibility: &mut Option<Visibility>,
        reveal_when: &mut Option<String>,
    ) -> bool {
        match name {
            "visibility" => *visibility = self.choice(cx, a, Visibility::parse, Visibility::NAMES),
            "reveal_when" => *reveal_when = self.word(cx, a),
            _ => return false,
        }
        true
    }

    // ----- blocks -----------------------------------------------------------

    fn file(&mut self, root: &SyntaxNode) -> Option<Document> {
        let depth = root
            .descendants()
            .filter(|n| {
                matches!(
                    n.kind(),
                    SyntaxKind::Block | SyntaxKind::List | SyntaxKind::Object | SyntaxKind::Call
                )
            })
            .map(|n| {
                n.ancestors()
                    .filter(|p| {
                        matches!(
                            p.kind(),
                            SyntaxKind::Block
                                | SyntaxKind::List
                                | SyntaxKind::Object
                                | SyntaxKind::Call
                        )
                    })
                    .count()
            })
            .max()
            .unwrap_or(0);
        let cx = Cx::node("");
        if depth > 16 {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!("The file nests blocks and values {depth} levels deep; the limit is 16."),
            );
            self.push(&cx, TextRange::empty(0.into()), d);
        }
        self.collect_templates(root);
        let mut doc = Document::default();
        let mut found = false;
        let mut comments = Comments::default();
        for item in self.items(&cx, root, &mut comments) {
            match item {
                It::Attr(name, a) => {
                    let d = Diagnostic::new(
                        Code::UnknownAttribute,
                        format!(
                            "`{name}` is set outside any block. A file holds an optional `dedalo` block and one `dungeon` block; put attributes inside them."
                        ),
                    );
                    self.push(&cx, a.0.text_range(), d);
                }
                It::Block(keyword, b) => match keyword.as_str() {
                    "dedalo" => doc.dedalo = Some(self.header(&b)),
                    "dungeon" if found => {
                        let d = Diagnostic::new(
                            Code::InvalidValue,
                            "A file holds exactly one `dungeon` block; remove or merge this one.",
                        );
                        self.push(&cx, b.header_range(), d);
                    }
                    "template" => {}
                    "dungeon" => {
                        if let Some(dungeon) = self.dungeon(&b) {
                            doc.dungeon = dungeon;
                            found = true;
                        }
                    }
                    k if DUNGEON_BLOCKS.contains(&k) => {
                        let d = Diagnostic::new(
                            Code::UnknownBlock,
                            format!(
                                "`{k}` must be written inside the `dungeon` block, not at the top of the file."
                            ),
                        );
                        self.push(&cx, b.header_range(), d);
                    }
                    _ => self.unknown_block(&cx, &b, "a file", &["dedalo", "dungeon"]),
                },
            }
        }
        doc.comments = comments;
        if !found {
            if self.diags.is_empty() {
                let d = Diagnostic::new(
                    Code::InvalidValue,
                    "The file has no `dungeon` block. Start with `dungeon \"my_dungeon\" { }`.",
                );
                self.push(&cx, TextRange::empty(0.into()), d);
            }
            return None;
        }
        Some(doc)
    }

    fn header(&mut self, b: &Block) -> Header {
        let mut header = Header {
            comments: self.node_comments(b),
            ..Header::default()
        };
        let cx = Cx::node("");
        let Some(body) = b.body() else { return header };
        for item in self.items(&cx, &body, &mut header.comments) {
            match item {
                It::Attr(name, a) if name == "version" => {
                    header.version = self.int(&cx, &a).and_then(|v| u32::try_from(v).ok());
                    if header
                        .version
                        .is_some_and(|v| v == 0 || v > LANGUAGE_VERSION)
                    {
                        let d = Diagnostic::new(
                            Code::InvalidValue,
                            format!(
                                "Language version {} is not supported; this tool reads versions 1 to {LANGUAGE_VERSION}.",
                                header.version.unwrap_or(0)
                            ),
                        );
                        self.push(&cx, a.0.text_range(), d);
                    }
                }
                It::Attr(_, a) => self.unknown_attr(&cx, &a, "dedalo", &["version"], false),
                It::Block(keyword, blk) if keyword == "plugin" => {
                    let Some(label) = blk.labels().first().cloned() else {
                        let d = Diagnostic::new(
                            Code::InvalidValue,
                            "A `plugin` block needs the plugin ID as a label, for example `plugin \"hand_drawn\" { }`.",
                        );
                        self.push(&cx, blk.header_range(), d);
                        continue;
                    };
                    let mut req = PluginRequirement {
                        id: ast::label_text(&label),
                        version: None,
                    };
                    let mut ignored = Comments::default();
                    if let Some(body) = blk.body() {
                        for inner in self.items(&cx, &body, &mut ignored) {
                            match inner {
                                It::Attr(name, a) if name == "version" => {
                                    req.version = self.string(&cx, &a)
                                }
                                It::Attr(_, a) => {
                                    self.unknown_attr(&cx, &a, "plugin", &["version"], false)
                                }
                                It::Block(_, nested) => {
                                    self.unknown_block(&cx, &nested, "plugin", &[])
                                }
                            }
                        }
                    }
                    header.plugins.push(req);
                }
                It::Block(_, blk) => self.unknown_block(&cx, &blk, "dedalo", &["plugin"]),
            }
        }
        header
    }

    fn dungeon(&mut self, b: &Block) -> Option<Dungeon> {
        let comments = self.node_comments(b);
        // The dungeon ID is not part of the element namespace.
        let label = b.labels().first().cloned();
        let top = Cx::node("");
        let Some(label) = label else {
            let d = Diagnostic::new(
                Code::InvalidValue,
                "The `dungeon` block needs an ID label, for example `dungeon \"forgotten_crypt\" { }`.",
            );
            self.push(&top, b.header_range(), d);
            return None;
        };
        let id = ast::label_text(&label);
        if label.kind() == SyntaxKind::Ident {
            self.rewrites
                .insert(label.text_range().start().into(), ast::quote(&id));
        }
        if !valid_id(&id) {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!("`{id}` is not a valid ID. An ID starts with a lowercase letter and continues with lowercase letters, digits or underscores, up to 64 characters."),
            )
            .suggest(format!("Use `{}`.", normalise_id(&id)));
            self.push(&top, label.text_range(), d);
        }
        let cx = Cx::node(&format!("dungeon:{id}"));
        self.map.insert(&cx.id, "", label.text_range());
        let mut d = Dungeon {
            id,
            comments,
            ..Dungeon::default()
        };
        let body = b.body()?;
        let mut cm = std::mem::take(&mut d.comments);
        for item in self.items(&cx, &body, &mut cm) {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "name" => d.name = self.string(&cx, &a),
                    _ => self.unknown_attr(&cx, &a, "dungeon", &["name"], true),
                },
                It::Block(keyword, blk) => match keyword.as_str() {
                    "grid" => d.grid = Some(self.grid(&cx, &blk, &mut cm)),
                    "settings" => d.settings = Some(self.settings(&cx, &blk, &mut cm)),
                    "render" => d.render = Some(self.render(&cx, &blk, &mut cm)),
                    "legend" => d.legend = Some(self.legend(&cx, &blk, &mut cm)),
                    "properties" => {
                        let cat = self.cat;
                        self.properties(&cx, &blk, &mut cm, &mut d.properties, &|n| {
                            cat.common.properties.get(n).cloned()
                        });
                    }
                    "room" => d.spaces.extend(self.room(&blk)),
                    "corridor" => d.spaces.extend(self.corridor(&blk)),
                    "connection" => d.connections.extend(self.connection(&blk)),
                    "feature" | "entity" => {
                        let class = if keyword == "feature" {
                            Class::Feature
                        } else {
                            Class::Entity
                        };
                        if let Some((element, space)) = self.element(&blk, class, None) {
                            match space {
                                Some(space) => self.loose.push((space, element)),
                                None => d.elements.push(element),
                            }
                        }
                    }
                    "note" => self.note(&blk, None),
                    "export" => d.exports.extend(self.export(&blk)),
                    "template" => {}
                    _ => self.unknown_block(&cx, &blk, "dungeon", DUNGEON_BLOCKS),
                },
            }
        }
        d.comments = cm;
        // Elements that named their space by attribute join it in file order.
        for (space_id, element) in std::mem::take(&mut self.loose) {
            match d.spaces.iter_mut().find(|s| s.id == space_id) {
                Some(space) => space.elements.push(element),
                None => {
                    let known: Vec<&str> = d.spaces.iter().map(|s| s.id.as_str()).collect();
                    let diag = Diagnostic::new(
                        Code::UnknownReference,
                        format!("`{}` is placed in `{space_id}`, but no room or corridor has that ID.{}", element.id, did_you_mean(&space_id, known)),
                    )
                    .element(element.id.clone())
                    .at("room");
                    self.diags.push(diag);
                    d.elements.push(element);
                }
            }
        }
        for space in &mut d.spaces {
            space
                .elements
                .sort_by_key(|e| self.offsets.get(&e.id).copied().unwrap_or(u32::MAX));
        }
        d.notes = std::mem::take(&mut self.notes);
        let lines = &self.lines;
        self.map.attach(&mut self.diags, lines);
        Some(d)
    }

    /// Gathers the `template` blocks of the file and of the dungeon.
    fn collect_templates(&mut self, root: &SyntaxNode) {
        let mut blocks: Vec<Block> = Vec::new();
        for item in ast::items(root) {
            if let Item::Block(b) = item {
                match b.keyword_text().as_str() {
                    "template" => blocks.push(b),
                    "dungeon" => {
                        if let Some(body) = b.body() {
                            blocks.extend(ast::items(&body).filter_map(|i| match i {
                                Item::Block(t) if t.keyword_text() == "template" => Some(t),
                                _ => None,
                            }));
                        }
                    }
                    _ => {}
                }
            }
        }
        let cx = Cx::node("");
        for b in blocks {
            let Some(label) = b.labels().first().cloned() else {
                let d = Diagnostic::new(
                    Code::InvalidValue,
                    "A `template` block needs a name, for example `template \"standard_door\" { }`.",
                );
                self.push(&cx, b.header_range(), d);
                continue;
            };
            let name = ast::label_text(&label);
            if label.kind() == SyntaxKind::Ident {
                self.rewrites
                    .insert(label.text_range().start().into(), ast::quote(&name));
            }
            if self
                .templates
                .get(&name)
                .is_some_and(|(_, external)| !external)
            {
                let d = Diagnostic::new(
                    Code::DuplicateId,
                    format!("Two templates are named `{name}`; rename one of them."),
                );
                self.push(&cx, label.text_range(), d);
                continue;
            }
            for inner in b.body().iter().flat_map(ast::items) {
                if let Item::Block(nested) = inner
                    && TEMPLATE_FORBIDDEN.contains(&nested.keyword_text().as_str())
                {
                    let d = Diagnostic::new(
                        Code::UnknownBlock,
                        format!(
                            "Template `{name}` holds a `{}` block. Templates hold attributes and `layout`, `placement` and `properties` blocks only, since anything with an ID would repeat on every use.",
                            nested.keyword_text()
                        ),
                    );
                    self.push(&cx, nested.header_range(), d);
                }
            }
            self.templates.insert(name, (b, false));
        }
    }

    /// The template a block uses, with the range of its name.
    fn using(b: &Block) -> Option<(String, TextRange)> {
        let labels = b.labels();
        (labels.len() == 3 && labels[1].text() == "using")
            .then(|| (ast::label_text(&labels[2]), labels[2].text_range()))
    }

    /// Applies the template a block uses: the template body is lowered as
    /// if it were this block, and the block's own values are merged over it.
    fn apply_template<T>(
        &mut self,
        b: &Block,
        own: T,
        again: &dyn Fn(&mut Self, &Block) -> Option<T>,
    ) -> T
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let Some((name, range)) = Self::using(b) else {
            return own;
        };
        let cx = Cx::node("");
        let Some((template, external)) = self.templates.get(&name).cloned() else {
            let known: Vec<String> = self.templates.keys().cloned().collect();
            let d = Diagnostic::new(
                Code::UnknownReference,
                format!("There is no template named `{name}`.{}", did_you_mean(&name, known.iter().map(String::as_str))),
            )
            .suggest("Declare it with `template \"name\" { ... }` at the top of the file or in the dungeon, or load the pack that provides it.");
            self.push(&cx, range, d);
            return own;
        };
        let id = b.labels().first().map(ast::label_text).unwrap_or_default();
        // A pack's template lives in another text: its spans and spellings
        // do not belong to this file.
        let saved = external.then(|| (self.rewrites.clone(), self.map.clone(), self.diags.len()));
        self.template_target = Some(id);
        let base = again(self, &template);
        self.template_target = None;
        if let Some((rewrites, map, count)) = saved {
            self.rewrites = rewrites;
            self.map = map;
            let problems = self.diags.split_off(count);
            if let Some(first) = problems
                .iter()
                .find(|d| d.severity == crate::diag::Severity::Error)
            {
                let d = Diagnostic::new(
                    first.code,
                    format!(
                        "The template `{name}` does not fit this block: {}",
                        first.message
                    ),
                );
                self.push(&cx, range, d);
            }
        }
        let Some(base) = base else { return own };
        let (Ok(base_json), Ok(own_json)) =
            (serde_json::to_value(&base), serde_json::to_value(&own))
        else {
            return own;
        };
        serde_json::from_value(merge(base_json, own_json)).unwrap_or(own)
    }

    fn grid(&mut self, cx: &Cx, b: &Block, cm: &mut Comments) -> Grid {
        let mut grid = Grid::default();
        let (inner, items) = self.sub(cx, "grid", b, cm);
        for item in items {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "unit" => grid.unit = self.string(&inner, &a),
                    "type" => {
                        grid.ty = self.choice(
                            &inner,
                            &a,
                            |s| (s == "square").then(|| s.to_string()),
                            &["square"],
                        )
                    }
                    _ => self.unknown_attr(&inner, &a, "grid", &["unit", "type"], false),
                },
                It::Block(_, blk) => self.unknown_block(&inner, &blk, "grid", &[]),
            }
        }
        grid
    }

    fn settings(&mut self, cx: &Cx, b: &Block, cm: &mut Comments) -> Settings {
        let mut settings = Settings::default();
        let (inner, items) = self.sub(cx, "settings", b, cm);
        for item in items {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "default_wall" => settings.default_wall = self.symbol(&inner, &a),
                    "default_floor" => settings.default_floor = self.symbol(&inner, &a),
                    "seed" => settings.seed = self.int(&inner, &a),
                    "max_stub" => settings.max_stub = self.num(&inner, &a),
                    _ => self.unknown_attr(
                        &inner,
                        &a,
                        "settings",
                        &["default_wall", "default_floor", "seed", "max_stub"],
                        false,
                    ),
                },
                It::Block(_, blk) => self.unknown_block(&inner, &blk, "settings", &[]),
            }
        }
        settings
    }

    fn render(&mut self, cx: &Cx, b: &Block, cm: &mut Comments) -> RenderOptions {
        let mut render = RenderOptions::default();
        let (inner, items) = self.sub(cx, "render", b, cm);
        for item in items {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "theme" => render.theme = self.symbol(&inner, &a),
                    "title" => render.title = self.boolean(&inner, &a),
                    "text" => {
                        render.text = self.choice(&inner, &a, TextMode::parse, TextMode::NAMES)
                    }
                    "room_ids" => render.room_ids = self.boolean(&inner, &a),
                    _ => self.unknown_attr(&inner, &a, "render", RENDER_ATTRS, false),
                },
                It::Block(keyword, blk) if keyword == "plugin" => {
                    let Some(label) = blk.labels().first().cloned() else {
                        continue;
                    };
                    let mut options = Props::new();
                    let mut ignored = Comments::default();
                    if let Some(body) = blk.body() {
                        for option in self.items(&inner, &body, &mut ignored) {
                            if let It::Attr(name, a) = option
                                && let Some(e) = a.value()
                            {
                                let value = self.value(&inner, &name, &e);
                                options.insert(name, value);
                            }
                        }
                    }
                    render.plugins.insert(ast::label_text(&label), options);
                }
                It::Block(_, blk) => self.unknown_block(&inner, &blk, "render", &["plugin"]),
            }
        }
        render
    }

    fn legend(&mut self, cx: &Cx, b: &Block, cm: &mut Comments) -> Legend {
        let mut legend = Legend::default();
        let (inner, items) = self.sub(cx, "legend", b, cm);
        for item in items {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "mode" => {
                        legend.mode = self.choice(&inner, &a, LegendMode::parse, LegendMode::NAMES)
                    }
                    "position" => {
                        legend.position =
                            self.choice(&inner, &a, LegendPosition::parse, LegendPosition::NAMES)
                    }
                    "title" => legend.title = self.string(&inner, &a),
                    _ => self.unknown_attr(&inner, &a, "legend", LEGEND_ATTRS, false),
                },
                It::Block(keyword, blk) if keyword == "entry" => {
                    let Some(label) = blk.labels().first().cloned() else {
                        let d = Diagnostic::new(
                            Code::InvalidValue,
                            "An `entry` block needs a key label, for example `entry \"trap\" { label = \"Trappola\" }`.",
                        );
                        self.push(&inner, blk.header_range(), d);
                        continue;
                    };
                    let mut entry = LegendEntry {
                        key: ast::label_text(&label),
                        label: None,
                    };
                    let mut ignored = Comments::default();
                    if let Some(body) = blk.body() {
                        for field in self.items(&inner, &body, &mut ignored) {
                            match field {
                                It::Attr(name, a) if name == "label" => {
                                    entry.label = self.string(&inner, &a)
                                }
                                It::Attr(_, a) => {
                                    self.unknown_attr(&inner, &a, "entry", &["label"], false)
                                }
                                It::Block(_, nested) => {
                                    self.unknown_block(&inner, &nested, "entry", &[])
                                }
                            }
                        }
                    }
                    legend.entries.push(entry);
                }
                It::Block(_, blk) => self.unknown_block(&inner, &blk, "legend", &["entry"]),
            }
        }
        legend
    }

    fn space_flag(&mut self, cx: &Cx, name: &str, a: &Attribute, space: &mut Space) -> bool {
        match name {
            "name" => space.name = self.string(cx, a),
            "tags" => space.tags = self.list(cx, a, Self::e_word),
            _ => {
                return self.visibility_attr(
                    cx,
                    name,
                    a,
                    &mut space.visibility,
                    &mut space.reveal_when,
                );
            }
        }
        true
    }

    /// The blocks a room and a corridor share.
    fn space_block(
        &mut self,
        cx: &Cx,
        keyword: &str,
        blk: &Block,
        space: &mut Space,
        cm: &mut Comments,
    ) -> bool {
        match keyword {
            "properties" => {
                let (cat, kind) = (self.cat, space.kind.as_str());
                self.properties(cx, blk, cm, &mut space.properties, &|n| {
                    cat.structure_property(kind, n).cloned()
                });
            }
            "feature" | "entity" => {
                let class = if keyword == "feature" {
                    Class::Feature
                } else {
                    Class::Entity
                };
                if let Some((element, _)) = self.element(blk, class, Some(&space.id)) {
                    space.elements.push(element);
                }
            }
            "note" => self.note(blk, Some(&space.id)),
            _ => return false,
        }
        true
    }

    fn room(&mut self, b: &Block) -> Option<Space> {
        let mut cm = self.node_comments(b);
        let id = self.id_label(b, "entrance")?;
        let cx = Cx::node(&id);
        let mut room = Space::new(SpaceKind::Room, id);
        for item in self.items(&cx, &b.body()?, &mut cm) {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "shape" => room.shape = self.shape(&cx, &a),
                    "rotation" => room.rotation = self.quarter_turn(&cx, &a),
                    n if SPACE_FLAGS.contains(&n) => {
                        self.space_flag(&cx, n, &a, &mut room);
                    }
                    _ => self.unknown_attr(&cx, &a, "room", ROOM_ATTRS, true),
                },
                It::Block(keyword, blk) => match keyword.as_str() {
                    "layout" => room.layout = Some(self.layout(&cx, &blk, &mut cm)),
                    k => {
                        if !self.space_block(&cx, k, &blk, &mut room, &mut cm) {
                            self.unknown_block(
                                &cx,
                                &blk,
                                "room",
                                &["layout", "properties", "feature", "entity", "note"],
                            );
                        }
                    }
                },
            }
        }
        room.comments = cm;
        if self.template_target.is_some() {
            return Some(room);
        }
        Some(self.apply_template(b, room, &|l: &mut Self, t: &Block| l.room(t)))
    }

    fn layout(&mut self, cx: &Cx, b: &Block, cm: &mut Comments) -> Layout {
        let mut layout = Layout::default();
        let (inner, items) = self.sub(cx, "layout", b, cm);
        for item in items {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "mode" => {
                        layout.mode = self.choice(&inner, &a, LayoutMode::parse, LayoutMode::NAMES)
                    }
                    "position" => layout.position = self.point(&inner, &a),
                    "adjacent_to" => layout.adjacent_to = self.reference(&inner, &a),
                    "direction" => {
                        layout.direction =
                            self.choice(&inner, &a, Direction::parse, Direction::NAMES)
                    }
                    "align" => layout.align = self.choice(&inner, &a, Align::parse, Align::NAMES),
                    "offset" => layout.offset = self.num(&inner, &a),
                    "gap" => layout.gap = self.num(&inner, &a),
                    "north_of" | "south_of" | "east_of" | "west_of" => {
                        // Alias of `adjacent_to` plus `direction` (spec 5.10).
                        if layout.adjacent_to.is_some() || layout.direction.is_some() {
                            let d = Diagnostic::new(
                                Code::InvalidValue,
                                format!(
                                    "`{name}` conflicts with another placement relation in this `layout` block; keep a single one."
                                ),
                            );
                            self.push(&inner, a.0.text_range(), d);
                            continue;
                        }
                        layout.adjacent_to = self.reference(&inner, &a);
                        layout.direction = Direction::parse(name.trim_end_matches("_of"));
                        let (from, to) = (
                            format!("{}{name}", inner.prefix),
                            format!("{}adjacent_to", inner.prefix),
                        );
                        if let Some(c) = cm.attrs.remove(&from) {
                            cm.attrs.insert(to.clone(), c);
                        }
                        if let Some(range) = self.map.lookup(&inner.id, &from) {
                            self.map.insert(&inner.id, &to, range);
                        }
                    }
                    n if RESERVED_RELATIONS.contains(&n) => {
                        let d = Diagnostic::new(
                            Code::InvalidValue,
                            format!(
                                "The relation `{n}` is reserved for the post-1.0 layout engine and is not supported yet. Use `adjacent_to` with `direction`, or `north_of`, `south_of`, `east_of`, `west_of`."
                            ),
                        );
                        self.push(&inner, a.0.text_range(), d);
                    }
                    _ => self.unknown_attr(&inner, &a, "layout", LAYOUT_ATTRS, false),
                },
                It::Block(_, blk) => self.unknown_block(&inner, &blk, "layout", &[]),
            }
        }
        layout
    }

    fn corridor(&mut self, b: &Block) -> Option<Space> {
        let mut cm = self.node_comments(b);
        let id = self.id_label(b, "hall")?;
        let cx = Cx::node(&id);
        let mut corridor = Space::new(SpaceKind::Corridor, id);
        for item in self.items(&cx, &b.body()?, &mut cm) {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "connects" => corridor.connects = self.list(&cx, &a, Self::e_ref),
                    "width" => corridor.width = self.num(&cx, &a),
                    "path" => corridor.path = self.list(&cx, &a, Self::e_point),
                    "length" => corridor.length = self.num(&cx, &a),
                    "offset" => corridor.offset = self.num(&cx, &a),
                    n if SPACE_FLAGS.contains(&n) => {
                        self.space_flag(&cx, n, &a, &mut corridor);
                    }
                    _ => self.unknown_attr(&cx, &a, "corridor", CORRIDOR_ATTRS, true),
                },
                It::Block(keyword, blk) => {
                    if !self.space_block(&cx, &keyword, &blk, &mut corridor, &mut cm) {
                        self.unknown_block(
                            &cx,
                            &blk,
                            "corridor",
                            &["properties", "feature", "entity", "note"],
                        );
                    }
                }
            }
        }
        corridor.comments = cm;
        if self.template_target.is_some() {
            return Some(corridor);
        }
        Some(self.apply_template(b, corridor, &|l: &mut Self, t: &Block| l.corridor(t)))
    }

    fn connection(&mut self, b: &Block) -> Option<Connection> {
        let mut cm = self.node_comments(b);
        let id = self.id_label(b, "entrance_door")?;
        let cx = Cx::node(&id);
        let mut c = Connection {
            id: id.clone(),
            ..Connection::default()
        };
        let mut locked_alias = false;
        for item in self.items(&cx, &b.body()?, &mut cm) {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "name" => c.name = self.string(&cx, &a),
                    "from" => c.from = self.reference(&cx, &a).unwrap_or_default(),
                    "to" => c.to = self.reference(&cx, &a),
                    "type" => match self.symbol(&cx, &a).as_deref() {
                        // Aliases of `door` with a flag (spec 5.10).
                        Some("locked_door") => {
                            c.ty = "door".to_string();
                            locked_alias = true;
                        }
                        Some("secret_door") => {
                            c.ty = "door".to_string();
                            c.secret.get_or_insert(true);
                        }
                        Some(other) => c.ty = other.to_string(),
                        None => {}
                    },
                    "secret" => c.secret = self.boolean(&cx, &a),
                    "position" => c.position = self.point(&cx, &a),
                    "tags" => c.tags = self.list(&cx, &a, Self::e_word),
                    n => {
                        if !self.visibility_attr(&cx, n, &a, &mut c.visibility, &mut c.reveal_when)
                        {
                            self.unknown_attr(&cx, &a, "connection", CONNECTION_ATTRS, true);
                        }
                    }
                },
                It::Block(keyword, blk) => match keyword.as_str() {
                    "placement" => {
                        let mut p = ConnectionPlacement::default();
                        let (inner, items) = self.sub(&cx, "placement", &blk, &mut cm);
                        for field in items {
                            match field {
                                It::Attr(name, a) => match name.as_str() {
                                    "wall" => {
                                        p.wall = self.choice(
                                            &inner,
                                            &a,
                                            Direction::parse,
                                            Direction::NAMES,
                                        )
                                    }
                                    "edge" => p.edge = self.int(&inner, &a),
                                    "bearing" => p.bearing = self.num(&inner, &a),
                                    "offset" => p.offset = self.num(&inner, &a),
                                    "width" => p.width = self.num(&inner, &a),
                                    _ => self.unknown_attr(
                                        &inner,
                                        &a,
                                        "placement",
                                        CONNECTION_PLACEMENT,
                                        false,
                                    ),
                                },
                                It::Block(_, nested) => {
                                    self.unknown_block(&inner, &nested, "placement", &[])
                                }
                            }
                        }
                        c.placement = Some(p);
                    }
                    "properties" => {
                        let cat = self.cat;
                        self.properties(&cx, &blk, &mut cm, &mut c.properties, &|n| {
                            cat.structure_property("connection", n).cloned()
                        });
                    }
                    "note" => self.note(&blk, Some(&id)),
                    _ => self.unknown_block(
                        &cx,
                        &blk,
                        "connection",
                        &["placement", "properties", "note"],
                    ),
                },
            }
        }
        if locked_alias {
            c.properties
                .entry("locked".to_string())
                .or_insert(Value::Bool(true));
        }
        c.comments = cm;
        if self.template_target.is_some() {
            return Some(c);
        }
        let mut c = self.apply_template(b, c, &|l: &mut Self, t: &Block| l.connection(t));
        let cm = std::mem::take(&mut c.comments);
        if c.from.is_empty() {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "Connection `{id}` has no `from`. Name the space it starts in, for example `from = \"entrance\"`."
                ),
            );
            self.push(&cx, b.header_range(), d);
        }
        if c.ty.is_empty() {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "Connection `{id}` has no `type`. Set `type` to {}.",
                    one_of(CONNECTION_TYPES)
                ),
            );
            self.push(&cx, b.header_range(), d);
        }
        c.comments = cm;
        (!c.from.is_empty() && !c.ty.is_empty()).then_some(c)
    }

    /// A feature or entity. Returns the element and, for one written at
    /// dungeon level, the space it names by attribute.
    fn element(
        &mut self,
        b: &Block,
        class: Class,
        parent: Option<&str>,
    ) -> Option<(Element, Option<String>)> {
        let mut cm = self.node_comments(b);
        let id = self.id_label(
            b,
            if class == Class::Feature {
                "wall_torch"
            } else {
                "skeletons"
            },
        )?;
        if self.template_target.is_none() {
            self.offsets.insert(id.clone(), start(&b.0));
        }
        let cx = Cx::node(&id);
        let mut el = Element::new(class, id.clone());
        let mut space: Option<String> = None;
        let items = self.items(&cx, &b.body()?, &mut cm);
        // The type comes first: it selects the property schema.
        let ty = items.iter().find_map(|item| match item {
            It::Attr(name, a) if name == "type" => a.value().and_then(|e| {
                e.symbol_text()
                    .or_else(|| e.literal_token().and_then(|t| ast::unquote(t.text()).ok()))
            }),
            _ => None,
        });
        for item in items {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "type" => el.ty = self.symbol(&cx, &a).unwrap_or_default(),
                    "subtype" => el.subtype = self.symbol(&cx, &a),
                    "name" => el.name = self.string(&cx, &a),
                    "position" => el.position = self.point(&cx, &a),
                    "size" => el.size = self.point(&cx, &a),
                    "shape" => el.shape = self.shape(&cx, &a),
                    "rotation" => el.rotation = self.quarter_turn(&cx, &a),
                    "facing" => el.facing = self.choice(&cx, &a, Facing::parse, Facing::NAMES),
                    "reference" => el.reference = self.string(&cx, &a),
                    "contained_in" => el.contained_in = self.reference(&cx, &a),
                    "tags" => el.tags = self.list(&cx, &a, Self::e_word),
                    "room" | "corridor" => {
                        let Some(target) = self.reference(&cx, &a) else {
                            continue;
                        };
                        // The attribute is folded into nesting, so its comments move to the element.
                        if let Some(c) = cm.attrs.remove(&name) {
                            cm.leading.extend(c.leading);
                            cm.leading.extend(c.trailing);
                        }
                        match parent {
                            Some(p) if p != target => {
                                let d = Diagnostic::new(
                                    Code::InvalidValue,
                                    format!(
                                        "`{id}` is written inside `{p}` but sets `{name} = \"{target}\"`. Remove the attribute, or move the block."
                                    ),
                                );
                                self.push(&cx, a.0.text_range(), d);
                            }
                            Some(_) => {}
                            None if space.as_ref().is_some_and(|s| *s != target) => {
                                let d = Diagnostic::new(
                                    Code::InvalidValue,
                                    format!(
                                        "`{id}` names two different spaces; keep either `room` or `corridor`."
                                    ),
                                );
                                self.push(&cx, a.0.text_range(), d);
                            }
                            None => space = Some(target),
                        }
                    }
                    n => {
                        if !self.visibility_attr(
                            &cx,
                            n,
                            &a,
                            &mut el.visibility,
                            &mut el.reveal_when,
                        ) {
                            self.unknown_attr(&cx, &a, class.as_str(), ELEMENT_ATTRS, true);
                        }
                    }
                },
                It::Block(keyword, blk) => match keyword.as_str() {
                    "placement" => {
                        let mut p = ElementPlacement::default();
                        let (inner, fields) = self.sub(&cx, "placement", &blk, &mut cm);
                        for field in fields {
                            match field {
                                It::Attr(name, a) => match name.as_str() {
                                    "wall" => {
                                        p.wall = self.choice(
                                            &inner,
                                            &a,
                                            Direction::parse,
                                            Direction::NAMES,
                                        )
                                    }
                                    "offset" => p.offset = self.num(&inner, &a),
                                    "anchor" => {
                                        p.anchor =
                                            self.choice(&inner, &a, Anchor::parse, Anchor::NAMES)
                                    }
                                    _ => self.unknown_attr(
                                        &inner,
                                        &a,
                                        "placement",
                                        ELEMENT_PLACEMENT,
                                        false,
                                    ),
                                },
                                It::Block(_, nested) => {
                                    self.unknown_block(&inner, &nested, "placement", &[])
                                }
                            }
                        }
                        el.placement = Some(p);
                    }
                    "properties" => {
                        let (cat, ty) = (self.cat, ty.clone().unwrap_or_default());
                        self.properties(&cx, &blk, &mut cm, &mut el.properties, &|n| {
                            cat.property(&ty, n).cloned()
                        });
                    }
                    "note" => self.note(&blk, Some(&id)),
                    _ => self.unknown_block(
                        &cx,
                        &blk,
                        class.as_str(),
                        &["placement", "properties", "note"],
                    ),
                },
            }
        }
        el.comments = std::mem::take(&mut cm);
        if self.template_target.is_some() {
            return Some((el, space));
        }
        let mut el = self.apply_template(b, el, &|l: &mut Self, t: &Block| {
            l.element(t, class, parent).map(|(e, _)| e)
        });
        cm = std::mem::take(&mut el.comments);
        if el.ty.is_empty() {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!(
                    "`{id}` has no `type`. Set `type` to a catalog type such as {}.",
                    one_of(
                        &self
                            .cat
                            .types
                            .keys()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                    )
                ),
            );
            self.push(&cx, b.header_range(), d);
            return None;
        }
        el.comments = cm;
        Some((el, space))
    }

    fn note(&mut self, b: &Block, parent: Option<&str>) {
        let mut cm = self.node_comments(b);
        let Some(id) = self.id_label(b, "entrance_note") else {
            return;
        };
        let cx = Cx::node(&id);
        let mut note = Note {
            id: id.clone(),
            target: parent.map(str::to_string),
            ..Note::default()
        };
        let Some(body) = b.body() else { return };
        for item in self.items(&cx, &body, &mut cm) {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "text" => note.text = self.string(&cx, &a).unwrap_or_default(),
                    "position" => note.position = self.point(&cx, &a),
                    "target" | "room" => {
                        let Some(target) = self.reference(&cx, &a) else {
                            continue;
                        };
                        if name == "room" {
                            // Alias of `target` (spec 5.10).
                            if let Some(c) = cm.attrs.remove("room") {
                                cm.attrs.insert("target".to_string(), c);
                            }
                            if let Some(range) = self.map.lookup(&id, "room") {
                                self.map.insert(&id, "target", range);
                            }
                        }
                        match (&note.target, parent) {
                            (Some(existing), _) if *existing != target => {
                                let d = Diagnostic::new(
                                    Code::InvalidValue,
                                    format!(
                                        "Note `{id}` is attached to `{existing}` but sets `{name} = \"{target}\"`. Keep a single target."
                                    ),
                                );
                                self.push(&cx, a.0.text_range(), d);
                            }
                            (_, Some(_)) => {
                                // Same target as the parent block: the attribute is redundant.
                                cm.attrs.remove("target");
                            }
                            _ => note.target = Some(target),
                        }
                    }
                    n => {
                        if !self.visibility_attr(
                            &cx,
                            n,
                            &a,
                            &mut note.visibility,
                            &mut note.reveal_when,
                        ) {
                            self.unknown_attr(&cx, &a, "note", NOTE_ATTRS, false);
                        }
                    }
                },
                It::Block(_, blk) => self.unknown_block(&cx, &blk, "note", &[]),
            }
        }
        if note.text.is_empty() {
            let d = Diagnostic::new(
                Code::InvalidValue,
                format!("Note `{id}` has no `text`. Add `text = \"...\"`."),
            );
            self.push(&cx, b.header_range(), d);
        }
        note.comments = cm;
        self.notes.push(note);
    }

    fn export(&mut self, b: &Block) -> Option<Export> {
        let mut cm = self.node_comments(b);
        let labels = b.labels();
        let name = labels
            .first()
            .map_or_else(|| "default".to_string(), ast::label_text);
        if let Some(label) = labels.first().filter(|t| t.kind() == SyntaxKind::Ident) {
            self.rewrites
                .insert(label.text_range().start().into(), ast::quote(&name));
        }
        let cx = Cx::node(&format!("export:{name}"));
        self.map.insert(&cx.id, "", b.header_range());
        let mut ex = Export {
            name,
            ..Export::default()
        };
        for item in self.items(&cx, &b.body()?, &mut cm) {
            match item {
                It::Attr(name, a) => match name.as_str() {
                    "preset" => ex.preset = self.choice(&cx, &a, Preset::parse, Preset::NAMES),
                    "format" => ex.format = self.symbol(&cx, &a),
                    "resolution" => {
                        ex.resolution = self.choice(&cx, &a, Resolution::parse, Resolution::NAMES)
                    }
                    "cell_px" => {
                        ex.cell_px = self.int(&cx, &a);
                        if ex.cell_px.is_some_and(|v| !(16..=512).contains(&v)) {
                            let d = Diagnostic::new(
                                Code::InvalidValue,
                                format!(
                                    "`cell_px` is {}, but must be between 16 and 512 pixels per cell.",
                                    ex.cell_px.unwrap_or(0)
                                ),
                            );
                            self.push(&cx, a.0.text_range(), d);
                            ex.cell_px = None;
                        }
                    }
                    "rotation" => ex.rotation = self.quarter_turn(&cx, &a),
                    "revealed" => ex.revealed = self.list(&cx, &a, Self::e_word),
                    "include" => ex.include = self.list(&cx, &a, Self::e_ref),
                    "exclude" => ex.exclude = self.list(&cx, &a, Self::e_ref),
                    "bounds" => ex.bounds = self.choice(&cx, &a, Bounds::parse, Bounds::NAMES),
                    "page" => ex.page = self.choice(&cx, &a, Page::parse, Page::NAMES),
                    "orientation" => {
                        ex.orientation =
                            self.choice(&cx, &a, Orientation::parse, Orientation::NAMES)
                    }
                    "scale" => {
                        ex.scale = self.choice(&cx, &a, PrintScale::parse, PrintScale::NAMES)
                    }
                    _ => self.unknown_attr(&cx, &a, "export", EXPORT_ATTRS, false),
                },
                It::Block(keyword, blk) => match keyword.as_str() {
                    "visibility" => {
                        let (inner, toggles) = self.sub(&cx, "visibility", &blk, &mut cm);
                        for toggle in toggles {
                            match toggle {
                                It::Attr(name, a) if TOGGLES.contains(&name.as_str()) => {
                                    if let Some(on) = self.boolean(&inner, &a) {
                                        ex.visibility.insert(name, on);
                                    }
                                }
                                It::Attr(_, a) => {
                                    self.unknown_attr(&inner, &a, "visibility", TOGGLES, false)
                                }
                                It::Block(_, nested) => {
                                    self.unknown_block(&inner, &nested, "visibility", &[])
                                }
                            }
                        }
                    }
                    "legend" => ex.legend = Some(self.legend(&cx, &blk, &mut cm)),
                    "render" => ex.render = Some(self.render(&cx, &blk, &mut cm)),
                    _ => {
                        self.unknown_block(&cx, &blk, "export", &["visibility", "legend", "render"])
                    }
                },
            }
        }
        ex.comments = cm;
        Some(ex)
    }
}
