use rowan::{GreenNode, GreenNodeBuilder, TextRange, TextSize};

use crate::lexer::{Token, lex};
use crate::{MAX_PARSE_DEPTH, SyntaxKind, SyntaxNode};

/// A problem found while lexing or parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    pub range: TextRange,
}

/// The result of parsing: a tree that reproduces the source byte for byte,
/// and every syntax error found.
#[derive(Debug, Clone)]
pub struct Parse {
    green: GreenNode,
    pub errors: Vec<SyntaxError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
}

/// Parses a `.dedalo` source file. Never fails: errors are collected and the
/// parser resynchronises at the next line.
pub fn parse(src: &str) -> Parse {
    let (tokens, lex_errors) = lex(src);
    let mut p = Parser {
        src,
        tokens,
        pos: 0,
        offset: 0,
        depth: 0,
        b: GreenNodeBuilder::new(),
        errors: Vec::new(),
    };
    for (message, start, end) in lex_errors {
        p.errors.push(SyntaxError {
            message,
            range: range(start, end),
        });
    }
    p.b.start_node(SyntaxKind::Root.into());
    p.items(true);
    p.b.finish_node();
    let mut errors = p.errors;
    errors.sort_by_key(|e| (e.range.start(), e.range.end()));
    Parse {
        green: p.b.finish(),
        errors,
    }
}

fn range(start: usize, end: usize) -> TextRange {
    TextRange::new(TextSize::new(start as u32), TextSize::new(end as u32))
}

struct Parser<'s> {
    src: &'s str,
    tokens: Vec<Token>,
    pos: usize,
    offset: usize,
    depth: usize,
    b: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
}

use SyntaxKind::*;

impl Parser<'_> {
    fn peek(&self) -> SyntaxKind {
        self.tokens.get(self.pos).map_or(Eof, |t| t.kind)
    }

    fn text(&self) -> &str {
        match self.tokens.get(self.pos) {
            Some(t) => &self.src[self.offset..self.offset + t.len],
            None => "",
        }
    }

    /// Kind of the `n`-th token ahead that is not whitespace or a comment.
    /// Newlines count: they are significant.
    fn nth(&self, n: usize) -> SyntaxKind {
        self.tokens[self.pos.min(self.tokens.len())..]
            .iter()
            .filter(|t| !matches!(t.kind, Whitespace | Comment))
            .nth(n)
            .map_or(Eof, |t| t.kind)
    }

    fn bump(&mut self) {
        if let Some(t) = self.tokens.get(self.pos) {
            self.b
                .token(t.kind.into(), &self.src[self.offset..self.offset + t.len]);
            self.offset += t.len;
            self.pos += 1;
        }
    }

    /// Skips whitespace and comments on the current line.
    fn ws(&mut self) {
        while matches!(self.peek(), Whitespace | Comment) {
            self.bump();
        }
    }

    /// Skips whitespace, comments and line breaks.
    fn ws_nl(&mut self) {
        while matches!(self.peek(), Whitespace | Comment | Newline) {
            self.bump();
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        let len = self.tokens.get(self.pos).map_or(0, |t| t.len);
        self.errors.push(SyntaxError {
            message: message.into(),
            range: range(self.offset, self.offset + len),
        });
    }

    /// Wraps the rest of the line in an error node, leaving a closing brace
    /// for the enclosing block.
    fn recover_line(&mut self) {
        self.b.start_node(Error.into());
        while !matches!(self.peek(), Newline | Eof | RBrace) {
            self.bump();
        }
        self.b.finish_node();
    }

    fn bump_as_error(&mut self) {
        self.b.start_node(Error.into());
        self.bump();
        self.b.finish_node();
    }

    /// Reports excessive nesting once and swallows the rest of the input.
    fn too_deep(&mut self) -> bool {
        if self.depth < MAX_PARSE_DEPTH {
            return false;
        }
        self.error("This file nests blocks or values too deeply.");
        self.b.start_node(Error.into());
        while self.peek() != Eof {
            self.bump();
        }
        self.b.finish_node();
        true
    }

    fn items(&mut self, top: bool) {
        loop {
            self.ws_nl();
            match self.peek() {
                Eof => break,
                RBrace if top => {
                    self.error("This `}` closes nothing; remove it.");
                    self.bump_as_error();
                }
                RBrace => break,
                Ident if self.nth(1) == Eq => self.attribute(),
                Ident => self.block(),
                Unknown => {
                    self.unsupported();
                    self.recover_line();
                }
                _ => {
                    self.error(
                        "Expected an attribute (`name = value`) or a block (`keyword \"id\" { }`).",
                    );
                    let before = self.pos;
                    self.recover_line();
                    if self.pos == before {
                        self.bump_as_error();
                    }
                }
            }
        }
    }

    fn unsupported(&mut self) {
        let found = self.text().to_string();
        self.error(format!(
            "`{found}` is not part of Dedalo language version 1: operators, conditionals, `for` expressions, splats and `${{...}}` templates are unsupported. Write a literal value instead."
        ));
    }

    fn block(&mut self) {
        if self.too_deep() {
            return;
        }
        self.b.start_node(Block.into());
        let keyword = self.text().to_string();
        self.bump();
        loop {
            self.ws();
            match self.peek() {
                Str | Ident => {
                    self.b.start_node(Label.into());
                    self.bump();
                    self.b.finish_node();
                }
                LBrace => break,
                _ => {
                    self.error(format!("Expected `=` or `{{` after `{keyword}`."));
                    self.recover_line();
                    self.b.finish_node();
                    return;
                }
            }
        }
        self.b.start_node(Body.into());
        self.bump();
        self.depth += 1;
        self.items(false);
        self.depth -= 1;
        if self.peek() == RBrace {
            self.bump();
        } else {
            self.error(format!("The `{keyword}` block is never closed; add `}}`."));
        }
        self.b.finish_node();
        self.b.finish_node();
    }

    fn attribute(&mut self) {
        self.b.start_node(Attribute.into());
        self.bump();
        self.ws();
        self.bump(); // `=`, guaranteed by the caller's lookahead
        self.ws();
        self.expr();
        self.b.finish_node();
        match self.nth(0) {
            Newline | Eof | RBrace => {}
            Comma => {
                // Lenient: `placement { wall = north, offset = 2 }`.
                self.ws();
                self.bump();
            }
            Unknown => {
                self.ws();
                self.unsupported();
                self.recover_line();
            }
            _ => {
                self.ws();
                self.error("Expected the end of the line after this value.");
                self.recover_line();
            }
        }
    }

    /// Parses one value. Returns false when nothing was consumed.
    fn expr(&mut self) -> bool {
        if self.too_deep() {
            return true;
        }
        match self.peek() {
            Number | Str | Heredoc => {
                self.b.start_node(Literal.into());
                self.bump();
                self.b.finish_node();
            }
            Ident if self.nth(1) == LParen => self.call(),
            Ident if matches!(self.text(), "true" | "false" | "null") => {
                self.b.start_node(Literal.into());
                self.bump();
                self.b.finish_node();
            }
            Ident => {
                self.b.start_node(Symbol.into());
                self.bump();
                while self.peek() == Dot
                    && self
                        .tokens
                        .get(self.pos + 1)
                        .is_some_and(|t| t.kind == Ident)
                {
                    self.bump();
                    self.bump();
                }
                self.b.finish_node();
            }
            LBrack => self.sequence(List, RBrack, "]"),
            LBrace => self.object(),
            Unknown => {
                self.unsupported();
                return false;
            }
            _ => {
                self.error("Expected a value: a number, a string, true, false, null, a symbol, a list or a shape.");
                return false;
            }
        }
        true
    }

    /// A bracketed, comma-separated sequence: a list or the arguments of a call.
    fn sequence(&mut self, node: SyntaxKind, close: SyntaxKind, close_text: &str) {
        if node == List {
            self.b.start_node(List.into());
        }
        self.bump();
        self.depth += 1;
        loop {
            self.ws_nl();
            match self.peek() {
                k if k == close => {
                    self.bump();
                    break;
                }
                Eof => {
                    self.error(format!("Missing `{close_text}`."));
                    break;
                }
                _ => {}
            }
            let before = self.pos;
            self.expr();
            self.ws_nl();
            match self.peek() {
                Comma => self.bump(),
                Eof => {}
                k if k == close => {}
                Unknown if self.pos != before => {
                    self.unsupported();
                    self.bump_as_error();
                }
                _ => {
                    if self.pos != before {
                        self.error(format!("Expected `,` or `{close_text}` after this value."));
                    }
                    self.bump_as_error();
                }
            }
        }
        self.depth -= 1;
        if node == List {
            self.b.finish_node();
        }
    }

    fn call(&mut self) {
        self.b.start_node(Call.into());
        self.bump();
        self.ws();
        self.sequence(Call, RParen, ")");
        self.b.finish_node();
    }

    fn object(&mut self) {
        self.b.start_node(Object.into());
        self.bump();
        self.depth += 1;
        loop {
            self.ws_nl();
            match self.peek() {
                RBrace => {
                    self.bump();
                    break;
                }
                Eof => {
                    self.error("Missing `}`.");
                    break;
                }
                Comma => self.bump(),
                Ident | Str => {
                    self.b.start_node(Entry.into());
                    self.bump();
                    self.ws();
                    if self.peek() == Eq {
                        self.bump();
                        self.ws();
                        self.expr();
                    } else {
                        self.error("Expected `=` after this key.");
                    }
                    self.b.finish_node();
                }
                _ => {
                    self.error("Expected a key (`name = value`) or `}`.");
                    self.bump_as_error();
                }
            }
        }
        self.depth -= 1;
        self.b.finish_node();
    }
}
