//! Typed views over the untyped syntax tree.

use rowan::TextRange;

use crate::{NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken};

/// `keyword "label" { ... }`.
#[derive(Debug, Clone)]
pub struct Block(pub SyntaxNode);

/// `name = value`.
#[derive(Debug, Clone)]
pub struct Attribute(pub SyntaxNode);

#[derive(Debug, Clone)]
pub enum Item {
    Attr(Attribute),
    Block(Block),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(SyntaxNode),
    Symbol(SyntaxNode),
    List(SyntaxNode),
    Object(SyntaxNode),
    Call(SyntaxNode),
}

fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
    node.children_with_tokens()
        .filter_map(NodeOrToken::into_token)
}

/// The attributes and blocks directly inside a body or the file root.
pub fn items(parent: &SyntaxNode) -> impl Iterator<Item = Item> + use<> {
    parent.children().filter_map(|n| match n.kind() {
        SyntaxKind::Attribute => Some(Item::Attr(Attribute(n))),
        SyntaxKind::Block => Some(Item::Block(Block(n))),
        _ => None,
    })
}

impl Block {
    pub fn keyword(&self) -> Option<SyntaxToken> {
        tokens(&self.0).find(|t| t.kind() == SyntaxKind::Ident)
    }

    pub fn keyword_text(&self) -> String {
        self.keyword()
            .map(|t| t.text().to_string())
            .unwrap_or_default()
    }

    /// The label tokens, each a string or an identifier.
    pub fn labels(&self) -> Vec<SyntaxToken> {
        self.0
            .children()
            .filter(|n| n.kind() == SyntaxKind::Label)
            .filter_map(|n| n.first_token())
            .collect()
    }

    pub fn body(&self) -> Option<SyntaxNode> {
        self.0.children().find(|n| n.kind() == SyntaxKind::Body)
    }

    /// Range of the keyword and labels, without the body.
    pub fn header_range(&self) -> TextRange {
        let start = self.0.text_range().start();
        let end = self
            .labels()
            .last()
            .map(|t| t.text_range().end())
            .or_else(|| self.keyword().map(|t| t.text_range().end()));
        TextRange::new(start, end.unwrap_or(start))
    }
}

impl Attribute {
    pub fn name(&self) -> Option<SyntaxToken> {
        tokens(&self.0).find(|t| t.kind() == SyntaxKind::Ident)
    }

    pub fn name_text(&self) -> String {
        self.name()
            .map(|t| t.text().to_string())
            .unwrap_or_default()
    }

    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl Expr {
    pub fn cast(node: SyntaxNode) -> Option<Expr> {
        Some(match node.kind() {
            SyntaxKind::Literal => Expr::Literal(node),
            SyntaxKind::Symbol => Expr::Symbol(node),
            SyntaxKind::List => Expr::List(node),
            SyntaxKind::Object => Expr::Object(node),
            SyntaxKind::Call => Expr::Call(node),
            _ => return None,
        })
    }

    pub fn node(&self) -> &SyntaxNode {
        match self {
            Expr::Literal(n)
            | Expr::Symbol(n)
            | Expr::List(n)
            | Expr::Object(n)
            | Expr::Call(n) => n,
        }
    }

    pub fn range(&self) -> TextRange {
        self.node().text_range()
    }

    /// The single token of a literal.
    pub fn literal_token(&self) -> Option<SyntaxToken> {
        match self {
            Expr::Literal(n) => n.first_token(),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        self.literal_token()
            .is_some_and(|t| t.kind() == SyntaxKind::Ident && t.text() == "null")
    }

    /// The text of a symbol, for example `door` or `hand_drawn.sepia`.
    pub fn symbol_text(&self) -> Option<String> {
        match self {
            Expr::Symbol(n) => Some(
                tokens(n)
                    .filter(|t| !t.kind().is_trivia())
                    .map(|t| t.text().to_string())
                    .collect(),
            ),
            _ => None,
        }
    }

    /// The elements of a list or the arguments of a call.
    pub fn elements(&self) -> Vec<Expr> {
        match self {
            Expr::List(n) | Expr::Call(n) => n.children().filter_map(Expr::cast).collect(),
            _ => Vec::new(),
        }
    }

    pub fn call_name(&self) -> Option<String> {
        match self {
            Expr::Call(n) => tokens(n)
                .find(|t| t.kind() == SyntaxKind::Ident)
                .map(|t| t.text().to_string()),
            _ => None,
        }
    }

    /// The `key = value` entries of an object.
    pub fn entries(&self) -> Vec<(SyntaxToken, Option<Expr>)> {
        match self {
            Expr::Object(n) => n
                .children()
                .filter(|e| e.kind() == SyntaxKind::Entry)
                .filter_map(|e| {
                    let key = tokens(&e)
                        .find(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::Str))?;
                    Some((key, e.children().find_map(Expr::cast)))
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// A short description for error messages.
    pub fn describe(&self) -> String {
        match self {
            Expr::Literal(n) => match n.first_token() {
                Some(t) if t.kind() == SyntaxKind::Number => format!("the number {}", t.text()),
                Some(t) if t.kind() == SyntaxKind::Str => format!("the string {}", t.text()),
                Some(t) if t.kind() == SyntaxKind::Heredoc => "a heredoc string".to_string(),
                Some(t) => format!("`{}`", t.text()),
                None => "nothing".to_string(),
            },
            Expr::Symbol(_) => format!("the symbol `{}`", self.symbol_text().unwrap_or_default()),
            Expr::List(_) => "a list".to_string(),
            Expr::Object(_) => "an object".to_string(),
            Expr::Call(_) => format!("the call `{}(...)`", self.call_name().unwrap_or_default()),
        }
    }
}

/// The text of a block label or object key: unquoted when written as a string.
pub fn label_text(token: &SyntaxToken) -> String {
    if token.kind() == SyntaxKind::Str {
        unquote(token.text()).unwrap_or_else(|_| token.text().trim_matches('"').to_string())
    } else {
        token.text().to_string()
    }
}

/// Decodes a quoted string literal. Rejects unknown escapes and the template
/// introducers `${` and `%{`, which version 1 reserves (spec 5.2).
pub fn unquote(raw: &str) -> Result<String, String> {
    let inner = raw.strip_prefix('"').unwrap_or(raw);
    let inner = inner.strip_suffix('"').unwrap_or(inner);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16)
                        .ok()
                        .filter(|_| hex.len() == 4);
                    match code.and_then(char::from_u32) {
                        Some(ch) => out.push(ch),
                        None => {
                            return Err(format!(
                                "`\\u{hex}` is not a valid escape; use four hexadecimal digits, for example `\\u00e8`."
                            ));
                        }
                    }
                }
                Some(other) => {
                    return Err(format!(
                        "`\\{other}` is not a valid escape; the escapes are \\n, \\t, \\\", \\\\ and \\uXXXX."
                    ));
                }
                None => return Err("A string cannot end with a single backslash.".to_string()),
            },
            '$' | '%' if chars.peek() == Some(&'{') => {
                return Err(format!(
                    "`{c}{{` starts a template, which Dedalo language version 1 does not support. Remove it or rephrase the text."
                ));
            }
            _ => out.push(c),
        }
    }
    Ok(out)
}

/// Encodes text as a quoted string literal.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Decodes a heredoc token. Each content line keeps its line break; the
/// `<<-` form strips the indentation common to all non-blank lines.
pub fn heredoc_value(raw: &str) -> String {
    let indented = raw.starts_with("<<-");
    let mut lines: Vec<&str> = raw
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    if lines.len() < 2 {
        return String::new();
    }
    lines.remove(0);
    lines.pop();
    let strip = if indented {
        lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start_matches([' ', '\t']).len())
            .min()
            .unwrap_or(0)
    } else {
        0
    };
    let mut out = String::new();
    for line in lines {
        let cut = strip.min(line.len() - line.trim_start_matches([' ', '\t']).len());
        out.push_str(&line[cut..]);
        out.push('\n');
    }
    out
}

/// True when `text` can be written as a bare symbol, dotted or not.
pub fn is_symbol(text: &str) -> bool {
    !text.is_empty()
        && !matches!(text, "true" | "false" | "null")
        && text.split('.').all(|part| {
            let mut chars = part.chars();
            chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
                && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        })
}
