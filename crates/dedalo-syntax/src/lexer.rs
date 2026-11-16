use crate::SyntaxKind::{self, *};

pub(crate) struct Token {
    pub kind: SyntaxKind,
    pub len: usize,
}

/// A lexical error: message, start offset, end offset.
pub(crate) type LexError = (String, usize, usize);

/// Splits the source into tokens. Every byte ends up in exactly one token, so
/// concatenating the tokens gives back the source.
pub(crate) fn lex(src: &str) -> (Vec<Token>, Vec<LexError>) {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let rest = &src[i..];
        let c = rest.chars().next().expect("non-empty remainder");
        let kind = match c {
            '\n' => {
                i += 1;
                Newline
            }
            ' ' | '\t' | '\r' | '\u{feff}' => {
                i += rest
                    .find(|ch| !matches!(ch, ' ' | '\t' | '\r' | '\u{feff}'))
                    .unwrap_or(rest.len());
                Whitespace
            }
            '#' => {
                i = line_end(src, i);
                Comment
            }
            '/' if rest.starts_with("//") => {
                i = line_end(src, i);
                Comment
            }
            '/' if rest.starts_with("/*") => {
                match rest[2..].find("*/") {
                    Some(p) => i += p + 4,
                    None => {
                        i = src.len();
                        errors.push((
                            "This block comment is never closed; add `*/`.".into(),
                            start,
                            i,
                        ));
                    }
                }
                Comment
            }
            '"' => {
                i = lex_string(src, i, &mut errors);
                Str
            }
            '<' if rest.starts_with("<<") => match lex_heredoc(src, i, &mut errors) {
                Some(end) => {
                    i = end;
                    Heredoc
                }
                None => {
                    i += 1;
                    Unknown
                }
            },
            '0'..='9' => {
                i = lex_number(bytes, i);
                Number
            }
            '-' if bytes.get(i + 1).is_some_and(u8::is_ascii_digit) => {
                i = lex_number(bytes, i + 1);
                Number
            }
            c if c == '_' || c.is_alphabetic() => {
                i += rest
                    .find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '-'))
                    .unwrap_or(rest.len());
                Ident
            }
            '{' => single(&mut i, LBrace),
            '}' => single(&mut i, RBrace),
            '[' => single(&mut i, LBrack),
            ']' => single(&mut i, RBrack),
            '(' => single(&mut i, LParen),
            ')' => single(&mut i, RParen),
            '=' => single(&mut i, Eq),
            ',' => single(&mut i, Comma),
            '.' => single(&mut i, Dot),
            other => {
                i += other.len_utf8();
                Unknown
            }
        };
        tokens.push(Token {
            kind,
            len: i - start,
        });
    }
    (tokens, errors)
}

fn single(i: &mut usize, kind: SyntaxKind) -> SyntaxKind {
    *i += 1;
    kind
}

/// Offset of the end of the line, excluding the line break and a carriage
/// return before it.
fn line_end(src: &str, from: usize) -> usize {
    let end = src[from..].find('\n').map_or(src.len(), |p| from + p);
    if end > from && src.as_bytes()[end - 1] == b'\r' {
        end - 1
    } else {
        end
    }
}

fn lex_string(src: &str, start: usize, errors: &mut Vec<LexError>) -> usize {
    let bytes = src.as_bytes();
    let mut j = start + 1;
    loop {
        match bytes.get(j) {
            None | Some(b'\n') => {
                errors.push((
                    "This string is never closed; add `\"` before the end of the line.".into(),
                    start,
                    j,
                ));
                return j;
            }
            Some(b'"') => return j + 1,
            Some(b'\\') => {
                j += 1;
                // Skip the escaped character, whatever its width.
                if let Some(ch) = src[j..].chars().next()
                    && ch != '\n'
                {
                    j += ch.len_utf8();
                }
            }
            Some(_) => j += 1,
        }
    }
}

/// Lexes `<<TAG` or `<<-TAG` through the line that holds only `TAG`. Returns
/// `None` when the text after `<<` is not a heredoc introducer.
fn lex_heredoc(src: &str, start: usize, errors: &mut Vec<LexError>) -> Option<usize> {
    let rest = &src[start..];
    let mut k = 2;
    if rest[k..].starts_with('-') {
        k += 1;
    }
    let tag_len = rest[k..]
        .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .unwrap_or(rest.len() - k);
    if tag_len == 0 {
        return None;
    }
    let tag = &rest[k..k + tag_len];
    let after = &rest[k + tag_len..];
    let first_break = after.find('\n')?;
    if !after[..first_break].trim().is_empty() {
        return None;
    }
    let mut pos = start + k + tag_len + first_break + 1;
    loop {
        if pos >= src.len() {
            errors.push((
                format!("This heredoc is never closed; end it with a line holding only `{tag}`."),
                start,
                src.len(),
            ));
            return Some(src.len());
        }
        let end = src[pos..].find('\n').map_or(src.len(), |p| pos + p);
        let line = &src[pos..end];
        if line.trim() == tag {
            let indent = line.len() - line.trim_start().len();
            return Some(pos + indent + tag.len());
        }
        pos = end + 1;
    }
}

fn lex_number(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    i
}
