//! The canonical formatter (spec 5.11).
//!
//! It normalises whitespace only: four-space indentation, one blank line
//! around multi-line sibling blocks, no alignment of `=`. It never reorders
//! blocks or attributes, never drops a comment, and is idempotent.

use std::collections::BTreeMap;

use crate::{NodeOrToken, SyntaxKind, SyntaxNode};

const INDENT: &str = "    ";

/// Formats a tree that parsed without errors.
///
/// `rewrites` maps the start offset of a value or label to its canonical
/// spelling. The lower stage fills it with the lenient spellings of spec 5.4
/// (a quoted string where a symbol is expected, a bare identifier where a
/// reference is expected), because only the schema can tell which is which.
pub fn format(root: &SyntaxNode, rewrites: &BTreeMap<u32, String>) -> String {
    let mut f = Formatter {
        rewrites,
        out: String::new(),
    };
    f.body(root, 0);
    f.out
}

struct Formatter<'a> {
    rewrites: &'a BTreeMap<u32, String>,
    out: String,
}

enum Chunk {
    Comment(String),
    /// An attribute or block, with the comment that trails it on its line.
    Item(SyntaxNode, Option<String>),
}

struct Entry {
    chunk: Chunk,
    /// Whether a blank line precedes this entry.
    blank: bool,
}

fn is_comment(el: &NodeOrToken<SyntaxNode, crate::SyntaxToken>) -> bool {
    el.as_token()
        .is_some_and(|t| t.kind() == SyntaxKind::Comment)
}

fn has_direct_comment(node: &SyntaxNode) -> bool {
    node.children_with_tokens().any(|el| is_comment(&el))
}

/// Comments inside an item that the item's own layout cannot hold. They are
/// printed on the lines above it so that none is ever lost.
fn hoisted_comments(item: &SyntaxNode) -> Vec<String> {
    let comment_text = |el: NodeOrToken<SyntaxNode, crate::SyntaxToken>| {
        el.into_token()
            .filter(|t| t.kind() == SyntaxKind::Comment)
            .map(|t| t.text().trim_end().to_string())
    };
    if item.kind() == SyntaxKind::Block {
        return item
            .children_with_tokens()
            .filter_map(comment_text)
            .collect();
    }
    item.descendants_with_tokens()
        .filter(|el| {
            let parent = el.parent().map(|p| p.kind());
            !matches!(parent, Some(SyntaxKind::List | SyntaxKind::Object))
        })
        .filter_map(comment_text)
        .collect()
}

impl Formatter<'_> {
    fn chunks(&self, body: &SyntaxNode) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut newlines = 0usize;
        let mut on_item_line = false;
        for el in body.children_with_tokens() {
            match el {
                NodeOrToken::Token(t) => match t.kind() {
                    SyntaxKind::Newline => {
                        newlines += 1;
                        on_item_line = false;
                    }
                    SyntaxKind::Comment => {
                        let text = t.text().trim_end().to_string();
                        if on_item_line
                            && let Some(Entry {
                                chunk: Chunk::Item(_, trailing @ None),
                                ..
                            }) = entries.last_mut()
                        {
                            *trailing = Some(text);
                            continue;
                        }
                        let blank = newlines >= 2 && !entries.is_empty();
                        entries.push(Entry {
                            chunk: Chunk::Comment(text),
                            blank,
                        });
                        newlines = 0;
                        on_item_line = false;
                    }
                    _ => {}
                },
                NodeOrToken::Node(n)
                    if matches!(n.kind(), SyntaxKind::Attribute | SyntaxKind::Block) =>
                {
                    let mut blank = newlines >= 2 && !entries.is_empty();
                    for comment in hoisted_comments(&n) {
                        entries.push(Entry {
                            chunk: Chunk::Comment(comment),
                            blank,
                        });
                        blank = false;
                    }
                    entries.push(Entry {
                        chunk: Chunk::Item(n, None),
                        blank,
                    });
                    newlines = 0;
                    on_item_line = true;
                }
                NodeOrToken::Node(_) => {}
            }
        }
        entries
    }

    fn body(&mut self, body: &SyntaxNode, level: usize) {
        let mut entries = self.chunks(body);
        let big: Vec<bool> = entries
            .iter()
            .map(|e| matches!(&e.chunk, Chunk::Item(n, _) if n.kind() == SyntaxKind::Block && !one_line(n)))
            .collect();
        for i in 0..entries.len() {
            if i > 0 && big[i - 1] {
                entries[i].blank = true;
            }
            if big[i] {
                // The blank line goes above the comments attached to the block.
                let mut j = i;
                while j > 0
                    && matches!(entries[j - 1].chunk, Chunk::Comment(_))
                    && !entries[j].blank
                {
                    j -= 1;
                }
                if j > 0 {
                    entries[j].blank = true;
                }
            }
        }
        for entry in entries {
            if entry.blank {
                self.out.push('\n');
            }
            self.indent(level);
            match entry.chunk {
                Chunk::Comment(text) => self.out.push_str(&text),
                Chunk::Item(node, trailing) => {
                    if node.kind() == SyntaxKind::Attribute {
                        self.attribute(&node, level);
                    } else {
                        self.block(&node, level);
                    }
                    if let Some(text) = trailing {
                        self.out.push(' ');
                        self.out.push_str(&text);
                    }
                }
            }
            self.out.push('\n');
        }
    }

    fn indent(&mut self, level: usize) {
        for _ in 0..level {
            self.out.push_str(INDENT);
        }
    }

    fn block(&mut self, block: &SyntaxNode, level: usize) {
        for el in block.children_with_tokens() {
            match el {
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::Ident => {
                    self.out.push_str(t.text())
                }
                NodeOrToken::Node(n) if n.kind() == SyntaxKind::Label => {
                    self.out.push(' ');
                    let start = u32::from(n.text_range().start());
                    match self.rewrites.get(&start) {
                        Some(text) => self.out.push_str(text),
                        None => self.out.push_str(n.text().to_string().trim()),
                    }
                }
                _ => {}
            }
        }
        let Some(body) = block.children().find(|n| n.kind() == SyntaxKind::Body) else {
            return;
        };
        let items: Vec<SyntaxNode> = body
            .children()
            .filter(|n| matches!(n.kind(), SyntaxKind::Attribute | SyntaxKind::Block))
            .collect();
        if items.is_empty() && !has_direct_comment(&body) {
            self.out.push_str(" {}");
        } else if one_line(block) {
            self.out.push_str(" { ");
            self.attribute(&items[0], level);
            self.out.push_str(" }");
        } else {
            self.out.push_str(" {\n");
            self.body(&body, level + 1);
            self.indent(level);
            self.out.push('}');
        }
    }

    fn attribute(&mut self, attr: &SyntaxNode, level: usize) {
        if let Some(name) = attr
            .children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::Ident)
        {
            self.out.push_str(name.text());
        }
        self.out.push_str(" = ");
        if let Some(value) = attr.children().find(|n| n.kind().is_expr()) {
            let text = self.expr(&value, level);
            self.out.push_str(&text);
        }
    }

    fn expr(&self, node: &SyntaxNode, level: usize) -> String {
        if let Some(text) = self.rewrites.get(&u32::from(node.text_range().start())) {
            return text.clone();
        }
        match node.kind() {
            SyntaxKind::List => self.list(node, level),
            SyntaxKind::Object => self.object(node, level),
            SyntaxKind::Call => {
                let name = node
                    .first_token()
                    .map(|t| t.text().to_string())
                    .unwrap_or_default();
                let args: Vec<String> = node
                    .children()
                    .filter(|n| n.kind().is_expr())
                    .map(|n| self.expr(&n, level))
                    .collect();
                format!("{name}({})", args.join(", "))
            }
            // Literals and symbols: the tokens as written, without trivia.
            _ => node
                .children_with_tokens()
                .filter_map(NodeOrToken::into_token)
                .filter(|t| !t.kind().is_trivia())
                .map(|t| t.text().to_string())
                .collect(),
        }
    }

    fn list(&self, node: &SyntaxNode, level: usize) -> String {
        let items: Vec<SyntaxNode> = node.children().filter(|n| n.kind().is_expr()).collect();
        let comments = has_direct_comment(node);
        if items.is_empty() && !comments {
            return "[]".to_string();
        }
        if !comments && !node.text().to_string().contains('\n') {
            let parts: Vec<String> = items.iter().map(|n| self.expr(n, level)).collect();
            return format!("[{}]", parts.join(", "));
        }
        format!(
            "[\n{}{}]",
            self.lines(node, level + 1, ","),
            INDENT.repeat(level)
        )
    }

    fn object(&self, node: &SyntaxNode, level: usize) -> String {
        let entries: Vec<SyntaxNode> = node
            .children()
            .filter(|n| n.kind() == SyntaxKind::Entry)
            .collect();
        let comments = has_direct_comment(node);
        if entries.is_empty() && !comments {
            return "{}".to_string();
        }
        if !comments && !node.text().to_string().contains('\n') {
            let parts: Vec<String> = entries.iter().map(|n| self.entry(n, level)).collect();
            return format!("{{ {} }}", parts.join(", "));
        }
        format!(
            "{{\n{}{}}}",
            self.lines(node, level + 1, ""),
            INDENT.repeat(level)
        )
    }

    fn entry(&self, entry: &SyntaxNode, level: usize) -> String {
        let key = entry
            .first_token()
            .map(|t| t.text().to_string())
            .unwrap_or_default();
        let value = entry
            .children()
            .find(|n| n.kind().is_expr())
            .map(|n| self.expr(&n, level))
            .unwrap_or_default();
        format!("{key} = {value}")
    }

    /// The members of a multi-line list or object, one per line, with their
    /// comments.
    fn lines(&self, node: &SyntaxNode, level: usize, separator: &str) -> String {
        let pad = INDENT.repeat(level);
        let mut out = String::new();
        let mut pending: Option<String> = None;
        let mut newline_since_item = false;
        for el in node.children_with_tokens() {
            match el {
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::Newline => {
                    newline_since_item = true
                }
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::Comment => {
                    let text = t.text().trim_end();
                    match pending.take() {
                        Some(item) if !newline_since_item => {
                            out.push_str(&format!("{pad}{item}{separator} {text}\n"))
                        }
                        Some(item) => {
                            out.push_str(&format!("{pad}{item}{separator}\n{pad}{text}\n"))
                        }
                        None => out.push_str(&format!("{pad}{text}\n")),
                    }
                }
                NodeOrToken::Node(n) if n.kind().is_expr() || n.kind() == SyntaxKind::Entry => {
                    if let Some(item) = pending.take() {
                        out.push_str(&format!("{pad}{item}{separator}\n"));
                    }
                    pending = Some(if n.kind() == SyntaxKind::Entry {
                        self.entry(&n, level)
                    } else {
                        self.expr(&n, level)
                    });
                    newline_since_item = false;
                }
                _ => {}
            }
        }
        if let Some(item) = pending {
            out.push_str(&format!("{pad}{item}{separator}\n"));
        }
        out
    }
}

/// A block stays on one line when it was written on one line and holds at
/// most a single attribute.
fn one_line(block: &SyntaxNode) -> bool {
    let Some(body) = block.children().find(|n| n.kind() == SyntaxKind::Body) else {
        return true;
    };
    let attrs = body
        .children()
        .filter(|n| n.kind() == SyntaxKind::Attribute)
        .count();
    let blocks = body
        .children()
        .filter(|n| n.kind() == SyntaxKind::Block)
        .count();
    if has_direct_comment(&body) {
        return false;
    }
    if attrs + blocks == 0 {
        return true;
    }
    blocks == 0 && attrs == 1 && !body.text().to_string().contains('\n')
}
