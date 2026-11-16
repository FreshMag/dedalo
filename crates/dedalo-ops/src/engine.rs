//! The edit engine: operations applied to the syntax tree (spec 13.3).
//!
//! Every operation is a sequence of small text edits located on a fresh
//! parse of the current text. Text, comments and formatting outside the
//! touched nodes are preserved byte for byte; inserted blocks are written
//! in canonical form.

use std::collections::{BTreeMap, BTreeSet};

use dedalo_catalog::Catalog;
use dedalo_model::ops::Op;
use dedalo_model::{Code, Diagnostic, Document, Num, SpaceKind, Value};
use dedalo_syntax::ast::{self, Attribute, Block, Item, quote};
use dedalo_syntax::edit::{TextEdit, apply};
use dedalo_syntax::{SyntaxKind, SyntaxNode};

use crate::value::to_dsl;

/// The block keywords whose label is an element ID.
pub const ELEMENT_KEYWORDS: &[&str] = &[
    "room",
    "corridor",
    "connection",
    "feature",
    "entity",
    "note",
];
/// Rough order of blocks at dungeon level, used to place an added block.
const ORDER: &[&str] = &[
    "grid",
    "settings",
    "render",
    "legend",
    "properties",
    "room",
    "corridor",
    "feature",
    "entity",
    "connection",
    "note",
    "export",
];

/// The text after a batch, and the IDs it touched.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub source: String,
    pub changed: Vec<String>,
}

type R<T> = Result<T, Diagnostic>;

fn fail(code: Code, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(code, message)
}

fn kind_of(op: &Op) -> &'static str {
    match op {
        Op::Add { .. } => "add",
        Op::Update { .. } => "update",
        Op::Remove { .. } => "remove",
        Op::Move { .. } => "move",
        Op::Connect { .. } => "connect",
    }
}

/// Applies a batch in order. A batch is atomic: on the first failure nothing
/// is returned but the diagnostic.
pub fn apply_ops(source: &str, ops: &[Op], cat: &Catalog) -> R<Applied> {
    let parsed = dedalo_syntax::parse(source);
    if !parsed.errors.is_empty() {
        return Err(fail(
            Code::SyntaxError,
            "The file has syntax errors; fix them before applying edit operations. `dedalo validate` lists them.",
        ));
    }
    let mut e = Editor {
        src: source.to_string(),
        cat,
        changed: Vec::new(),
    };
    for (k, op) in ops.iter().enumerate() {
        e.op(op).map_err(|mut d| {
            d.message = format!(
                "Operation {} ({}) failed: {}",
                k + 1,
                kind_of(op),
                d.message
            );
            d
        })?;
        if !dedalo_syntax::parse(&e.src).errors.is_empty() {
            return Err(fail(
                Code::SyntaxError,
                format!(
                    "Operation {} ({}) produced invalid text; check its values.",
                    k + 1,
                    kind_of(op)
                ),
            ));
        }
    }
    let mut seen = BTreeSet::new();
    e.changed.retain(|id| seen.insert(id.clone()));
    Ok(Applied {
        source: e.src,
        changed: e.changed,
    })
}

// ----- text helpers ---------------------------------------------------------------

fn line_start(src: &str, off: usize) -> usize {
    src[..off].rfind('\n').map_or(0, |p| p + 1)
}

/// The end of the line holding `off`, after its line break.
fn line_end(src: &str, off: usize) -> usize {
    src[off..].find('\n').map_or(src.len(), |p| off + p + 1)
}

fn indent_at(src: &str, off: usize) -> String {
    let ls = line_start(src, off);
    src[ls..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// Prefixes every non-empty line with `indent`.
fn reindent(text: &str, indent: &str) -> String {
    text.lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("{indent}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn range(node: &SyntaxNode) -> (usize, usize) {
    (
        u32::from(node.text_range().start()) as usize,
        u32::from(node.text_range().end()) as usize,
    )
}

/// The text of a block re-based to column zero: the indentation of its
/// header line is removed from every following line.
fn dedent_block(src: &str, node: &SyntaxNode) -> String {
    let (s, e) = range(node);
    let indent = indent_at(src, s);
    let text = &src[s..e];
    text.lines()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                l
            } else {
                l.strip_prefix(indent.as_str()).unwrap_or(l.trim_start())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Canonical form of a block given as text at column zero.
fn canonical(text: &str) -> String {
    let parsed = dedalo_syntax::parse(text);
    if parsed.errors.is_empty() {
        dedalo_syntax::fmt::format(&parsed.syntax(), &BTreeMap::new())
            .trim_end()
            .to_string()
    } else {
        text.trim_end().to_string()
    }
}

// ----- tree helpers ---------------------------------------------------------------

fn items(block: &Block) -> Vec<Item> {
    block
        .body()
        .map(|b| ast::items(&b).collect())
        .unwrap_or_default()
}

fn attr(block: &Block, name: &str) -> Option<Attribute> {
    items(block).into_iter().find_map(|i| match i {
        Item::Attr(a) if a.name_text() == name => Some(a),
        _ => None,
    })
}

fn sub_block(block: &Block, keyword: &str) -> Option<Block> {
    items(block).into_iter().find_map(|i| match i {
        Item::Block(b) if b.keyword_text() == keyword && b.labels().is_empty() => Some(b),
        _ => None,
    })
}

fn label(block: &Block) -> Option<String> {
    block.labels().first().map(ast::label_text)
}

fn dungeon(root: &SyntaxNode) -> Option<Block> {
    ast::items(root).find_map(|i| match i {
        Item::Block(b) if b.keyword_text() == "dungeon" => Some(b),
        _ => None,
    })
}

/// The block declaring `id`, or the dungeon block when `id` is its ID.
fn find(root: &SyntaxNode, id: &str) -> Option<Block> {
    root.descendants()
        .filter(|n| n.kind() == SyntaxKind::Block)
        .map(Block)
        .find(|b| {
            ELEMENT_KEYWORDS.contains(&b.keyword_text().as_str()) && label(b).as_deref() == Some(id)
        })
        .or_else(|| dungeon(root).filter(|d| label(d).as_deref() == Some(id)))
}

/// The block whose body holds `block`.
fn parent_block(block: &Block) -> Option<Block> {
    block
        .0
        .parent()
        .and_then(|body| body.parent())
        .filter(|n| n.kind() == SyntaxKind::Block)
        .map(Block)
}

/// The value text of an attribute, as written.
fn value_text(a: &Attribute) -> Option<String> {
    let v = a.value()?;
    v.symbol_text()
        .or_else(|| v.literal_token().map(|t| ast::label_text(&t)))
}

fn known_ids(root: &SyntaxNode) -> BTreeSet<String> {
    root.descendants()
        .filter(|n| n.kind() == SyntaxKind::Block)
        .map(Block)
        .filter(|b| ELEMENT_KEYWORDS.contains(&b.keyword_text().as_str()))
        .filter_map(|b| label(&b))
        .collect()
}

fn unknown_id(root: &SyntaxNode, id: &str) -> Diagnostic {
    let ids = known_ids(root);
    let hint = dedalo_model::suggest::did_you_mean(id, ids.iter().map(String::as_str));
    fail(
        Code::UnknownReference,
        format!("No element has the ID `{id}`.{hint}"),
    )
    .element(id)
}

/// Splits a dotted path, validating its segments.
fn segments(path: &str) -> R<Vec<String>> {
    let parts: Vec<String> = path.split('.').map(str::to_string).collect();
    if parts.iter().any(|p| p.is_empty() || !ast::is_symbol(p)) {
        return Err(fail(
            Code::InvalidValue,
            format!(
                "`{path}` is not an attribute path; use dotted names such as `placement.offset`."
            ),
        ));
    }
    Ok(parts)
}

struct Editor<'a> {
    src: String,
    cat: &'a Catalog,
    changed: Vec<String>,
}

impl Editor<'_> {
    fn root(&self) -> SyntaxNode {
        dedalo_syntax::parse(&self.src).syntax()
    }

    fn edit(&mut self, edits: Vec<TextEdit>) -> R<()> {
        self.src = apply(&self.src, &edits)
            .ok_or_else(|| fail(Code::InvalidValue, "internal error: overlapping text edits"))?;
        Ok(())
    }

    fn block(&self, id: &str) -> R<Block> {
        let root = self.root();
        find(&root, id).ok_or_else(|| unknown_id(&root, id))
    }

    fn op(&mut self, op: &Op) -> R<()> {
        match op {
            Op::Update { id, set, unset } => {
                for path in unset {
                    self.unset(id, path)?;
                }
                for (path, value) in set {
                    self.set(id, path, value)?;
                }
                self.changed.push(id.clone());
            }
            Op::Remove { id, cascade } => self.remove(id, *cascade)?,
            Op::Add {
                parent,
                source,
                kind,
                id,
                set,
            } => self.add(
                parent.as_deref(),
                source.as_deref(),
                kind.as_deref(),
                id.as_deref(),
                set,
            )?,
            Op::Connect { id, from, to, ty } => {
                let set = BTreeMap::from([
                    ("from".to_string(), Value::Str(from.clone())),
                    ("to".to_string(), Value::Str(to.clone())),
                    ("type".to_string(), Value::Str(ty.clone())),
                ]);
                self.add(None, None, Some("connection"), Some(id), &set)?;
            }
            Op::Move { id, to } => self.move_to(id, to)?,
        }
        Ok(())
    }

    // ----- inserting and removing text ------------------------------------------------

    /// Inserts an item (an attribute or a block, at column zero) at the end
    /// of a block's body.
    fn insert_item(&self, block: &Block, item: &str) -> R<TextEdit> {
        let src = &self.src;
        let body = block
            .body()
            .ok_or_else(|| fail(Code::InvalidValue, "the block has no body"))?;
        let (bs, be) = range(&body);
        let indent = indent_at(src, range(&block.0).0);
        let inner = format!("{indent}    ");
        let item = reindent(item, &inner);
        let body_text = &src[bs..be];
        if !body_text.contains('\n') {
            // A one-line body is rewritten onto several lines.
            let mut text = String::from("{\n");
            for existing in ast::items(&body) {
                let node = match existing {
                    Item::Attr(a) => a.0,
                    Item::Block(b) => b.0,
                };
                text.push_str(&format!("{inner}{}\n", node.text().to_string().trim()));
            }
            text.push_str(&format!("{item}\n{indent}}}"));
            return Ok(TextEdit {
                start: bs as u32,
                end: be as u32,
                text,
            });
        }
        let close = body
            .last_token()
            .filter(|t| t.kind() == SyntaxKind::RBrace)
            .ok_or_else(|| fail(Code::InvalidValue, "the block is not closed"))?;
        let r = u32::from(close.text_range().start()) as usize;
        let blank = item.contains('\n') && ast::items(&body).next().is_some();
        let gap = if blank { "\n" } else { "" };
        let ls = line_start(src, r);
        if src[ls..r].trim().is_empty() {
            Ok(TextEdit {
                start: ls as u32,
                end: ls as u32,
                text: format!("{gap}{item}\n"),
            })
        } else {
            Ok(TextEdit {
                start: r as u32,
                end: r as u32,
                text: format!("\n{gap}{item}\n{indent}"),
            })
        }
    }

    /// Removes a node together with its line, the comment lines directly
    /// above it, and one of two blank lines that would end up adjacent.
    fn remove_node(&self, node: &SyntaxNode) -> TextEdit {
        let src = &self.src;
        let (s, e) = range(node);
        let (ls, le) = (line_start(src, s), line_end(src, e));
        let rest = src[e..le].trim();
        let whole_lines = src[ls..s].trim().is_empty()
            && (rest.is_empty() || rest.starts_with('#') || rest.starts_with("//") || rest == ",");
        if !whole_lines {
            // Inside a one-line block: take a following comma with it.
            let mut end = e;
            let after = &src[e..];
            if let Some(stripped) = after.strip_prefix(',') {
                end += 1 + (stripped.len() - stripped.trim_start_matches(' ').len());
            }
            return TextEdit {
                start: s as u32,
                end: end as u32,
                text: String::new(),
            };
        }
        let mut start = ls;
        while start > 0 {
            let prev = line_start(src, start - 1);
            let t = src[prev..start].trim();
            if t.starts_with('#') || t.starts_with("//") {
                start = prev;
            } else {
                break;
            }
        }
        let mut end = le;
        let prev_blank = start > 0 && src[line_start(src, start - 1)..start].trim().is_empty();
        if prev_blank {
            let next = line_end(src, end.min(src.len()));
            let next_text = src[end..next].trim();
            if end < src.len() && next_text.is_empty() {
                end = next;
            } else if next_text.starts_with('}') {
                start = line_start(src, start - 1);
            }
        }
        TextEdit {
            start: start as u32,
            end: end as u32,
            text: String::new(),
        }
    }

    // ----- attributes -------------------------------------------------------------

    /// The catalog schema of a property of the element declared by `block`.
    fn prop_spec(&self, block: &Block, name: &str) -> Option<dedalo_catalog::PropSpec> {
        let keyword = block.keyword_text();
        match keyword.as_str() {
            "room" | "corridor" | "connection" => {
                self.cat.structure_property(&keyword, name).cloned()
            }
            _ => {
                let ty = attr(block, "type")
                    .and_then(|a| value_text(&a))
                    .unwrap_or_default();
                self.cat.property(&ty, name).cloned()
            }
        }
    }

    fn set(&mut self, id: &str, path: &str, value: &Value) -> R<()> {
        let parts = segments(path)?;
        if *value == Value::Null {
            return self.unset(id, path);
        }
        let block = self.block(id)?;
        // Aliases (spec 5.10) are rewritten to the attribute being set.
        if parts.len() == 2
            && parts[0] == "layout"
            && matches!(parts[1].as_str(), "adjacent_to" | "direction")
        {
            self.expand_layout_alias(id)?;
        }
        if block.keyword_text() == "note" && path == "target" && attr(&block, "room").is_some() {
            self.unset(id, "room")?;
        }
        if block.keyword_text() == "connection" {
            self.normalise_door_alias(id, path)?;
        }
        let block = self.block(id)?;
        let prop = parts
            .first()
            .filter(|p| *p == "properties")
            .and_then(|_| self.prop_spec(&block, &parts[1]));
        let text = to_dsl(path, value, prop.as_ref());
        let edit = self.set_in(&block, &parts, &text)?;
        self.edit(vec![edit])
    }

    fn set_in(&self, block: &Block, path: &[String], text: &str) -> R<TextEdit> {
        if path.len() == 1 {
            return match attr(block, &path[0]) {
                Some(a) => {
                    let (s, e) = match a.value() {
                        Some(v) => (u32::from(v.range().start()), u32::from(v.range().end())),
                        None => (
                            u32::from(a.0.text_range().end()),
                            u32::from(a.0.text_range().end()),
                        ),
                    };
                    Ok(TextEdit {
                        start: s,
                        end: e,
                        text: text.to_string(),
                    })
                }
                None => self.insert_item(block, &format!("{} = {text}", path[0])),
            };
        }
        match sub_block(block, &path[0]) {
            Some(sb) => self.set_in(&sb, &path[1..], text),
            None => {
                fn nested(path: &[String], text: &str) -> String {
                    if path.len() == 1 {
                        format!("{} = {text}", path[0])
                    } else {
                        format!(
                            "{} {{\n{}\n}}",
                            path[0],
                            reindent(&nested(&path[1..], text), "    ")
                        )
                    }
                }
                self.insert_item(block, &nested(path, text))
            }
        }
    }

    fn unset(&mut self, id: &str, path: &str) -> R<()> {
        let parts = segments(path)?;
        if parts.len() == 2
            && parts[0] == "layout"
            && matches!(parts[1].as_str(), "adjacent_to" | "direction")
        {
            self.expand_layout_alias(id)?;
        }
        let block = self.block(id)?;
        if block.keyword_text() == "connection" {
            self.normalise_door_alias(id, path)?;
        }
        let block = self.block(id)?;
        if let Some(edit) = self.unset_in(&block, &parts) {
            self.edit(vec![edit])?;
        }
        Ok(())
    }

    /// Removes an attribute; a sub-block left empty goes with it.
    fn unset_in(&self, block: &Block, path: &[String]) -> Option<TextEdit> {
        if path.len() == 1 {
            return attr(block, &path[0]).map(|a| self.remove_node(&a.0));
        }
        let sb = sub_block(block, &path[0])?;
        let only = items(&sb).len() == 1;
        let inner = self.unset_in(&sb, &path[1..])?;
        if only && path.len() == 2 {
            Some(self.remove_node(&sb.0))
        } else {
            Some(inner)
        }
    }

    /// `north_of = "x"` becomes `adjacent_to = "x"` and `direction = north`.
    fn expand_layout_alias(&mut self, id: &str) -> R<()> {
        let block = self.block(id)?;
        let Some(layout) = sub_block(&block, "layout") else {
            return Ok(());
        };
        for dir in ["north", "south", "east", "west"] {
            if let Some(a) = attr(&layout, &format!("{dir}_of")) {
                let target = value_text(&a).unwrap_or_default();
                let edit = TextEdit {
                    start: u32::from(a.0.text_range().start()),
                    end: u32::from(a.0.text_range().end()),
                    text: format!("adjacent_to = {}", quote(&target)),
                };
                self.edit(vec![edit])?;
                let block = self.block(id)?;
                let edit = self.set_in(
                    &block,
                    &["layout".to_string(), "direction".to_string()],
                    dir,
                )?;
                return self.edit(vec![edit]);
            }
        }
        Ok(())
    }

    /// `type = locked_door` or `secret_door` becomes `type = door` with the
    /// flag written out, before the flag itself is changed.
    fn normalise_door_alias(&mut self, id: &str, path: &str) -> R<()> {
        let block = self.block(id)?;
        let Some(ty) = attr(&block, "type") else {
            return Ok(());
        };
        let alias = value_text(&ty).unwrap_or_default();
        let (flag, flag_text) = match alias.as_str() {
            "locked_door" => ("properties.locked", "true"),
            "secret_door" => ("secret", "true"),
            _ => return Ok(()),
        };
        if path != flag && path != "type" {
            return Ok(());
        }
        let v = ty.value().expect("a type attribute has a value");
        self.edit(vec![TextEdit {
            start: v.range().start().into(),
            end: v.range().end().into(),
            text: "door".to_string(),
        }])?;
        let block = self.block(id)?;
        if self.unset_in(&block, &segments(flag)?).is_none() {
            let edit = self.set_in(&block, &segments(flag)?, flag_text)?;
            self.edit(vec![edit])?;
        }
        Ok(())
    }

    // ----- adding -------------------------------------------------------------------

    fn add(
        &mut self,
        parent: Option<&str>,
        source: Option<&str>,
        kind: Option<&str>,
        id: Option<&str>,
        set: &BTreeMap<String, Value>,
    ) -> R<()> {
        let (text, keyword, new_id) = match (source, kind) {
            (Some(fragment), _) => {
                let parsed = dedalo_syntax::parse(fragment);
                if !parsed.errors.is_empty() {
                    return Err(fail(
                        Code::SyntaxError,
                        format!(
                            "The `source` fragment is not valid Dedalo text: {}",
                            parsed.errors[0].message
                        ),
                    ));
                }
                let blocks: Vec<Block> = ast::items(&parsed.syntax())
                    .filter_map(|i| match i {
                        Item::Block(b) => Some(b),
                        Item::Attr(_) => None,
                    })
                    .collect();
                let [block] = blocks.as_slice() else {
                    return Err(fail(
                        Code::InvalidValue,
                        "The `source` fragment must hold exactly one block, such as `feature \"brazier\" { ... }`.",
                    ));
                };
                let keyword = block.keyword_text();
                let new_id = label(block).unwrap_or_default();
                (canonical(fragment), keyword, new_id)
            }
            (None, Some(kind)) => {
                let new_id = id
                    .ok_or_else(|| fail(Code::InvalidValue, "An `add` with `kind` needs an `id`."))?
                    .to_string();
                (
                    self.block_text(kind, &new_id, set)?,
                    kind.to_string(),
                    new_id,
                )
            }
            (None, None) => {
                return Err(fail(
                    Code::InvalidValue,
                    "An `add` needs either `source` (a DSL fragment) or `kind`, `id` and `set`.",
                ));
            }
        };
        if !ELEMENT_KEYWORDS.contains(&keyword.as_str()) {
            return Err(fail(
                Code::UnknownBlock,
                format!(
                    "`{keyword}` blocks cannot be added; the kinds are {}.",
                    ELEMENT_KEYWORDS.join(", ")
                ),
            ));
        }
        let root = self.root();
        if known_ids(&root).contains(&new_id) {
            return Err(fail(
                Code::DuplicateId,
                format!("The ID `{new_id}` is already used; choose another."),
            )
            .element(new_id));
        }
        let target = match parent {
            Some(p) => {
                let block = find(&root, p).ok_or_else(|| unknown_id(&root, p))?;
                let pk = block.keyword_text();
                let allowed = match keyword.as_str() {
                    "feature" | "entity" => matches!(pk.as_str(), "room" | "corridor" | "dungeon"),
                    "note" => true,
                    _ => pk == "dungeon",
                };
                if !allowed {
                    return Err(fail(
                        Code::UnknownBlock,
                        format!("A `{keyword}` cannot be nested in the {pk} `{p}`."),
                    ));
                }
                block
            }
            None => dungeon(&root)
                .ok_or_else(|| fail(Code::InvalidValue, "The file has no `dungeon` block."))?,
        };
        let edit = if target.keyword_text() == "dungeon" {
            self.insert_in_dungeon(&target, &keyword, &text)?
        } else {
            self.insert_item(&target, &text)?
        };
        self.edit(vec![edit])?;
        self.changed.push(new_id);
        Ok(())
    }

    /// The canonical text of a new block from attribute paths and values.
    fn block_text(&self, kind: &str, id: &str, set: &BTreeMap<String, Value>) -> R<String> {
        #[derive(Default)]
        struct Tree {
            attrs: Vec<(String, String)>,
            blocks: BTreeMap<String, Tree>,
        }
        fn write(tree: &Tree) -> String {
            let mut lines: Vec<String> = tree
                .attrs
                .iter()
                .map(|(k, v)| format!("{k} = {v}"))
                .collect();
            for (name, sub) in &tree.blocks {
                lines.push(format!("{name} {{\n{}\n}}", reindent(&write(sub), "    ")));
            }
            lines.join("\n")
        }
        // `type` first selects the property schema, and reads best on top.
        let ty = set
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut keys: Vec<&String> = set.keys().collect();
        let rank = |k: &str| {
            ["from", "to", "type", "subtype", "name", "shape"]
                .iter()
                .position(|x| *x == k)
                .unwrap_or(9)
        };
        keys.sort_by_key(|k| (rank(k), k.to_string()));
        let mut tree = Tree::default();
        for key in keys {
            let parts = segments(key)?;
            let prop = match parts.as_slice() {
                [p, name] if p == "properties" => match kind {
                    "room" | "corridor" | "connection" => {
                        self.cat.structure_property(kind, name).cloned()
                    }
                    _ => self.cat.property(&ty, name).cloned(),
                },
                _ => None,
            };
            let text = to_dsl(key, &set[key], prop.as_ref());
            let mut node = &mut tree;
            for p in &parts[..parts.len() - 1] {
                node = node.blocks.entry(p.clone()).or_default();
            }
            node.attrs.push((parts[parts.len() - 1].clone(), text));
        }
        let body = write(&tree);
        let text = if body.is_empty() {
            format!("{kind} {} {{}}", quote(id))
        } else {
            format!("{kind} {} {{\n{}\n}}", quote(id), reindent(&body, "    "))
        };
        Ok(canonical(&text))
    }

    /// Adds a block at dungeon level after the last block of the same or an
    /// earlier kind, separated by a blank line.
    fn insert_in_dungeon(&self, dungeon: &Block, keyword: &str, text: &str) -> R<TextEdit> {
        let rank = |k: &str| ORDER.iter().position(|x| *x == k).unwrap_or(0);
        let mine = rank(keyword);
        let anchor = items(dungeon)
            .into_iter()
            .filter_map(|i| match i {
                Item::Block(b) => Some(b),
                Item::Attr(_) => None,
            })
            .rfind(|b| rank(&b.keyword_text()) <= mine);
        let Some(anchor) = anchor else {
            return self.insert_item(dungeon, text);
        };
        let src = &self.src;
        let end = line_end(src, range(&anchor.0).1);
        let indent = indent_at(src, range(&anchor.0).0);
        Ok(TextEdit {
            start: end as u32,
            end: end as u32,
            text: format!("\n{}\n", reindent(text, &indent)),
        })
    }

    // ----- removing -----------------------------------------------------------------

    fn remove(&mut self, id: &str, cascade: bool) -> R<()> {
        let block = self.block(id)?;
        if block.keyword_text() == "dungeon" {
            return Err(fail(
                Code::InvalidValue,
                "The dungeon itself cannot be removed.",
            ));
        }
        // The element and everything nested in it.
        let gone: BTreeSet<String> = block
            .0
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::Block)
            .map(Block)
            .filter(|b| ELEMENT_KEYWORDS.contains(&b.keyword_text().as_str()))
            .filter_map(|b| label(&b))
            .collect();
        let doc = self.document()?;
        let refs = referrers(&doc, &gone, self.cat);
        if !refs.is_empty() && !cascade {
            let list: Vec<String> = refs
                .iter()
                .map(|r| format!("`{}` ({})", r.id, r.how))
                .collect();
            let mut d = fail(
                Code::ReferencedElement,
                format!("`{id}` cannot be removed: {} refer to it. Remove or update them first, or pass `cascade: true`.", list.join(", ")),
            )
            .element(id);
            for r in &refs {
                d = d.related(r.id.clone(), r.how.clone());
            }
            return Err(d);
        }
        let edit = self.remove_node(&block.0);
        self.edit(vec![edit])?;
        self.changed.push(id.to_string());
        if !refs.is_empty() {
            self.cascade(&doc, &refs)?;
        }
        Ok(())
    }

    fn document(&self) -> R<Document> {
        let root = self.root();
        dedalo_model::lower::lower(&root, &self.src, self.cat)
            .document
            .ok_or_else(|| fail(Code::InvalidValue, "The file has no `dungeon` block."))
    }

    fn cascade(&mut self, before: &Document, refs: &[Referrer]) -> R<()> {
        let resolved = dedalo_geom::resolve(before, self.cat).0;
        for r in refs {
            match &r.fix {
                Fix::Remove => {
                    if find(&self.root(), &r.id).is_some() {
                        self.remove(&r.id, true)?;
                    }
                }
                Fix::Unset(path) => {
                    self.unset(&r.id, path)?;
                    self.changed.push(r.id.clone());
                }
                Fix::Pin => {
                    // A room placed next to a removed room keeps its place.
                    let Some(space) = resolved.spaces.iter().find(|s| s.id == r.id) else {
                        continue;
                    };
                    let block = self.block(&r.id)?;
                    if let Some(layout) = sub_block(&block, "layout") {
                        let edit = self.remove_node(&layout.0);
                        self.edit(vec![edit])?;
                    }
                    let point = Value::List(vec![
                        Value::Num(Num(space.bbox.x0)),
                        Value::Num(Num(space.bbox.y0)),
                    ]);
                    self.set(&r.id, "layout.position", &point)?;
                    self.changed.push(r.id.clone());
                }
                Fix::List(field, export) => {
                    self.drop_from_export_list(export, field, &r.removed)?
                }
            }
        }
        Ok(())
    }

    fn drop_from_export_list(
        &mut self,
        export: &str,
        field: &str,
        removed: &BTreeSet<String>,
    ) -> R<()> {
        let root = self.root();
        let Some(d) = dungeon(&root) else {
            return Ok(());
        };
        let block = items(&d).into_iter().find_map(|i| match i {
            Item::Block(b)
                if b.keyword_text() == "export"
                    && label(&b).unwrap_or_else(|| "default".to_string()) == export =>
            {
                Some(b)
            }
            _ => None,
        });
        let Some(a) = block.as_ref().and_then(|b| attr(b, field)) else {
            return Ok(());
        };
        let Some(v) = a.value() else { return Ok(()) };
        let kept: Vec<String> = v
            .elements()
            .iter()
            .filter_map(|e| {
                e.literal_token()
                    .map(|t| ast::label_text(&t))
                    .or_else(|| e.symbol_text())
            })
            .filter(|x| !removed.contains(x))
            .collect();
        let edit = if kept.is_empty() {
            self.remove_node(&a.0)
        } else {
            TextEdit {
                start: v.range().start().into(),
                end: v.range().end().into(),
                text: format!(
                    "[{}]",
                    kept.iter().map(|k| quote(k)).collect::<Vec<_>>().join(", ")
                ),
            }
        };
        self.edit(vec![edit])
    }

    // ----- moving -------------------------------------------------------------------

    fn move_to(&mut self, id: &str, to: &BTreeMap<String, Value>) -> R<()> {
        let block = self.block(id)?;
        let keyword = block.keyword_text();
        match keyword.as_str() {
            "room" => {
                const LAYOUT: &[&str] = &[
                    "position",
                    "adjacent_to",
                    "direction",
                    "align",
                    "offset",
                    "gap",
                    "mode",
                ];
                if let Some(bad) = to.keys().find(|k| !LAYOUT.contains(&k.as_str())) {
                    return Err(fail(
                        Code::UnknownAttribute,
                        format!(
                            "`{bad}` is not a layout field; a room moves with {}.",
                            LAYOUT.join(", ")
                        ),
                    ));
                }
                if let Some(layout) = sub_block(&block, "layout") {
                    let edit = self.remove_node(&layout.0);
                    self.edit(vec![edit])?;
                }
                for (k, v) in to {
                    self.set(id, &format!("layout.{k}"), v)?;
                }
            }
            "feature" | "entity" | "note" => {
                if let Some(bad) = to
                    .keys()
                    .find(|k| !matches!(k.as_str(), "room" | "corridor" | "space" | "position"))
                {
                    return Err(fail(
                        Code::UnknownAttribute,
                        format!(
                            "`{bad}` is not a move target; an element moves with `room` and `position`."
                        ),
                    ));
                }
                let space = ["room", "corridor", "space"]
                    .iter()
                    .find_map(|k| to.get(*k))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(space) = space {
                    self.move_into(id, &space)?;
                }
                if let Some(position) = to.get("position") {
                    self.unset(id, "placement")?;
                    self.set(id, "position", position)?;
                }
            }
            other => {
                return Err(fail(
                    Code::InvalidValue,
                    format!("A {other} cannot be moved; update its attributes instead."),
                ));
            }
        }
        self.changed.push(id.to_string());
        Ok(())
    }

    /// Moves an element's block into another space.
    fn move_into(&mut self, id: &str, space: &str) -> R<()> {
        let block = self.block(id)?;
        let target = self.block(space)?;
        if !matches!(target.keyword_text().as_str(), "room" | "corridor") {
            return Err(fail(
                Code::InvalidValue,
                format!(
                    "`{space}` is a {}, not a room or corridor.",
                    target.keyword_text()
                ),
            ));
        }
        let parent = parent_block(&block);
        if parent.as_ref().and_then(label).as_deref() == Some(space) {
            return Ok(());
        }
        // A dungeon-level element names its space by attribute.
        if parent
            .as_ref()
            .is_some_and(|p| p.keyword_text() == "dungeon")
        {
            for name in ["room", "corridor"] {
                if attr(&block, name).is_some() {
                    self.unset(id, name)?;
                }
            }
            let kind = target.keyword_text();
            return self.set(id, &kind, &Value::Str(space.to_string()));
        }
        let text = dedent_block(&self.src, &block.0);
        let edit = self.remove_node(&block.0);
        self.edit(vec![edit])?;
        let target = self.block(space)?;
        let edit = self.insert_item(&target, &text)?;
        self.edit(vec![edit])
    }
}

// ----- referrers ------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Fix {
    Remove,
    Unset(String),
    /// Replace a relative layout by the room's current absolute position.
    Pin,
    /// Drop IDs from an export's `include` or `exclude` list: field, export.
    List(String, String),
}

#[derive(Debug, Clone)]
struct Referrer {
    id: String,
    how: String,
    fix: Fix,
    removed: BTreeSet<String>,
}

/// Everything outside `gone` that refers to an ID in `gone`.
fn referrers(doc: &Document, gone: &BTreeSet<String>, cat: &Catalog) -> Vec<Referrer> {
    let d = &doc.dungeon;
    let mut out = Vec::new();
    let mut push = |id: &str, how: String, fix: Fix| {
        if !gone.contains(id) && !out.iter().any(|r: &Referrer| r.id == id && r.how == how) {
            out.push(Referrer {
                id: id.to_string(),
                how,
                fix,
                removed: gone.clone(),
            });
        }
    };
    let hit = |x: &str| gone.contains(x);
    for c in &d.connections {
        if hit(&c.from) || c.to.as_deref().is_some_and(hit) {
            push(&c.id, "a connection to it".to_string(), Fix::Remove);
        }
        for (name, v) in &c.properties {
            if is_ref(cat.structure_property("connection", name), v, &hit) {
                push(
                    &c.id,
                    format!("its property `{name}`"),
                    Fix::Unset(format!("properties.{name}")),
                );
            }
        }
    }
    for s in &d.spaces {
        if s.kind == SpaceKind::Corridor && s.connects.iter().any(|x| hit(x)) {
            push(&s.id, "a corridor to it".to_string(), Fix::Remove);
        }
        if s.layout
            .as_ref()
            .and_then(|l| l.adjacent_to.as_deref())
            .is_some_and(hit)
        {
            push(&s.id, "placed next to it".to_string(), Fix::Pin);
        }
    }
    for (_, e) in d.elements() {
        if e.contained_in.as_deref().is_some_and(hit) {
            push(&e.id, "held in it".to_string(), Fix::Remove);
        }
        for (name, v) in &e.properties {
            if is_ref(cat.property(&e.ty, name), v, &hit) {
                push(
                    &e.id,
                    format!("its property `{name}`"),
                    Fix::Unset(format!("properties.{name}")),
                );
            }
        }
    }
    for n in &d.notes {
        if n.target.as_deref().is_some_and(hit) {
            push(&n.id, "a note on it".to_string(), Fix::Remove);
        }
    }
    for x in &d.exports {
        for (field, list) in [("include", &x.include), ("exclude", &x.exclude)] {
            if list.iter().any(|i| hit(i)) {
                push(
                    &format!("export:{}", x.name),
                    format!("listed in `{field}`"),
                    Fix::List(field.to_string(), x.name.clone()),
                );
            }
        }
    }
    out
}

fn is_ref(
    spec: Option<&dedalo_catalog::PropSpec>,
    value: &Value,
    hit: &dyn Fn(&str) -> bool,
) -> bool {
    use dedalo_catalog::PropType;
    let Some(spec) = spec else { return false };
    match (spec.ty, value) {
        (PropType::Reference, Value::Str(s)) => hit(s),
        (PropType::List, Value::List(items)) if spec.of == Some(PropType::Reference) => {
            items.iter().any(|i| i.as_str().is_some_and(hit))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "# A test dungeon.\ndungeon \"d\" {\n    room \"a\" {\n        shape = rectangle(8, 6) # the hall\n\n        feature \"plate\" {\n            type = trap\n            position = [2, 2]\n        }\n    }\n\n    room \"b\" {\n        shape = square(6)\n        layout { north_of = \"a\" }\n    }\n\n    connection \"door\" {\n        from = \"a\"\n        to = \"b\"\n        type = locked_door\n    }\n}\n";

    fn set(id: &str, path: &str, value: Value) -> Op {
        Op::set(id, path, value)
    }

    fn apply1(op: Op) -> String {
        apply_ops(BASE, &[op], &Catalog::builtin())
            .unwrap_or_else(|d| panic!("{}", d.message))
            .source
    }

    #[test]
    fn update_replaces_values_in_place() {
        let out = apply1(set(
            "a",
            "shape",
            Value::Map(BTreeMap::from([
                ("fn".to_string(), Value::Str("square".to_string())),
                (
                    "args".to_string(),
                    Value::List(vec![Value::Num(Num::int(8))]),
                ),
            ])),
        ));
        assert_eq!(
            out,
            BASE.replace("rectangle(8, 6) # the hall", "square(8) # the hall")
        );
    }

    #[test]
    fn update_inserts_into_one_line_blocks_and_expands_aliases() {
        let out = apply1(set("b", "layout.align", Value::Str("end".to_string())));
        assert!(out.contains("        layout {\n            north_of = \"a\"\n            align = end\n        }\n"), "{out}");
        let out = apply1(set(
            "b",
            "layout.direction",
            Value::Str("south".to_string()),
        ));
        assert!(out.contains("layout {\n            adjacent_to = \"a\"\n            direction = south\n        }"), "{out}");
    }

    #[test]
    fn update_creates_sub_blocks_and_rewrites_door_aliases() {
        let out = apply1(set("door", "placement.offset", Value::Num(Num::int(2))));
        assert!(out.contains("        type = locked_door\n\n        placement {\n            offset = 2\n        }\n    }"), "{out}");
        let op = Op::Update {
            id: "door".into(),
            set: BTreeMap::new(),
            unset: vec!["properties.locked".into()],
        };
        let out = apply1(op);
        assert!(out.contains("type = door\n    }"), "{out}");
    }

    #[test]
    fn remove_refuses_referenced_elements_unless_cascading() {
        let err = apply_ops(
            BASE,
            &[Op::Remove {
                id: "b".into(),
                cascade: false,
            }],
            &Catalog::builtin(),
        )
        .unwrap_err();
        assert_eq!(err.code, Code::ReferencedElement);
        assert!(
            err.message.contains("`door` (a connection to it)"),
            "{}",
            err.message
        );
        let out = apply1(Op::Remove {
            id: "b".into(),
            cascade: true,
        });
        assert_eq!(
            out,
            "# A test dungeon.\ndungeon \"d\" {\n    room \"a\" {\n        shape = rectangle(8, 6) # the hall\n\n        feature \"plate\" {\n            type = trap\n            position = [2, 2]\n        }\n    }\n}\n"
        );
        // Removing `a` pins `b` where it stood.
        let out = apply1(Op::Remove {
            id: "a".into(),
            cascade: true,
        });
        assert!(out.contains("room \"b\" {\n        shape = square(6)\n\n        layout {\n            position = [1, -6]\n        }"), "{out}");
    }

    #[test]
    fn add_and_move() {
        let fragment = "feature \"torch\" {\ntype = lighting\n  subtype = torch\nplacement { wall = north }\n}";
        let add = Op::Add {
            parent: Some("b".into()),
            source: Some(fragment.into()),
            kind: None,
            id: None,
            set: BTreeMap::new(),
        };
        let out = apply1(add);
        assert!(out.contains("        layout { north_of = \"a\" }\n\n        feature \"torch\" {\n            type = lighting\n            subtype = torch\n            placement { wall = north }\n        }\n    }"), "{out}");
        let mv = Op::Move {
            id: "plate".into(),
            to: BTreeMap::from([
                ("room".into(), Value::Str("b".into())),
                (
                    "position".into(),
                    Value::List(vec![Value::Num(Num::int(1)), Value::Num(Num::int(1))]),
                ),
            ]),
        };
        let out = apply1(mv);
        assert!(
            out.contains("        shape = rectangle(8, 6) # the hall\n    }\n"),
            "{out}"
        );
        assert!(out.contains("        feature \"plate\" {\n            type = trap\n            position = [1, 1]\n        }\n    }"), "{out}");
        let connect = Op::Connect {
            id: "arch".into(),
            from: "a".into(),
            to: "b".into(),
            ty: "archway".into(),
        };
        let out = apply1(connect);
        assert!(out.contains("        type = locked_door\n    }\n\n    connection \"arch\" {\n        from = \"a\"\n        to = \"b\"\n        type = archway\n    }\n}\n"), "{out}");
    }
}
