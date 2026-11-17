//! Dungeons kept inside Markdown notes, as fenced code blocks whose info
//! string starts with `dedalo`:
//!
//! ````markdown
//! ```dedalo preset=player level=ground
//! dungeon "crypt" { ... }
//! ```
//! ````
//!
//! A note may hold several blocks; they are counted from 1 in reading order.
//! The words after `dedalo` are render options (`OPTIONS`), so a note can
//! show the same dungeon to the DM and to the players.
//!
//! Tools read a block through [`Block::masked`]: the whole note with every
//! line outside the block blanked. The parser then reports the note's own
//! line and column numbers, with no translation.

use std::collections::BTreeMap;
use std::path::Path;

/// The language name that marks a block.
pub const LANGUAGE: &str = "dedalo";

/// The options a fence may carry, with what each sets.
pub const OPTIONS: &[(&str, &str)] = &[
    ("export", "the export block to render"),
    (
        "preset",
        "dm, player or clean; a bare `dm`, `player` or `clean` also works",
    ),
    ("level", "one level of a multi-level dungeon"),
    ("rotation", "0, 90, 180 or 270"),
    ("lang", "the language of default labels: en or it"),
    ("grid", "true or false"),
    ("labels", "room labels, true or false"),
    ("notes", "the notes panel, true or false"),
];

/// One `dedalo` block of a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// Position among the note's `dedalo` blocks, from 1.
    pub index: usize,
    /// The 1-based line of the opening fence.
    pub fence_line: usize,
    /// Byte range of the content, between the fences.
    pub start: usize,
    pub end: usize,
    /// Spaces before the opening fence, which content lines may share.
    pub indent: usize,
    /// The words after `dedalo`, as written.
    pub info: String,
    /// The options parsed from `info`.
    pub options: BTreeMap<String, String>,
    /// False when the note ends before the closing fence.
    pub closed: bool,
}

/// Whether a path names a Markdown note.
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

struct Fence {
    marker: char,
    length: usize,
    indent: usize,
}

/// An opening or closing fence: up to three spaces, then three or more
/// backticks or tildes. Returns the fence and the text after it.
fn fence(line: &str) -> Option<(Fence, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let marker = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let length = rest.len() - rest.trim_start_matches(marker).len();
    if length < 3 {
        return None;
    }
    let info = &rest[length..];
    // A backtick fence's info string cannot hold a backtick.
    if marker == '`' && info.contains('`') {
        return None;
    }
    Some((
        Fence {
            marker,
            length,
            indent,
        },
        info,
    ))
}

/// Parses the words after `dedalo`: `key=value` pairs, values optionally in
/// double quotes, and a bare preset name.
pub fn parse_options(info: &str) -> BTreeMap<String, String> {
    let mut options = BTreeMap::new();
    let mut chars = info.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '=' {
                break;
            }
            key.push(c);
            chars.next();
        }
        let mut value = None;
        if chars.peek() == Some(&'=') {
            chars.next();
            let mut text = String::new();
            if chars.peek() == Some(&'"') {
                chars.next();
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    text.push(c);
                }
            } else {
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    text.push(c);
                    chars.next();
                }
            }
            value = Some(text);
        }
        match value {
            Some(value) => {
                options.insert(key, value);
            }
            None if matches!(key.as_str(), "dm" | "player" | "clean") => {
                options.insert("preset".to_string(), key);
            }
            None if !key.is_empty() => {
                options.insert(key, "true".to_string());
            }
            None => {}
        }
    }
    options
}

/// Every `dedalo` block of a note, in order. Fences of other languages are
/// skipped whole, so an example inside a longer fence is not a block.
pub fn blocks(note: &str) -> Vec<Block> {
    let mut found = Vec::new();
    let mut open: Option<(Fence, Option<Block>)> = None;
    let mut offset = 0;
    for (number, line) in note.split_inclusive('\n').enumerate() {
        let next = offset + line.len();
        let text = line.trim_end_matches(['\n', '\r']);
        match open.take() {
            None => {
                if let Some((f, info)) = fence(text) {
                    let info = info.trim();
                    let language = info.split_whitespace().next().unwrap_or("");
                    let block = (language == LANGUAGE).then(|| {
                        let rest = info[language.len()..].trim().to_string();
                        Block {
                            index: found.len() + 1,
                            fence_line: number + 1,
                            start: next,
                            end: next,
                            indent: f.indent,
                            options: parse_options(&rest),
                            info: rest,
                            closed: false,
                        }
                    });
                    open = Some((f, block));
                }
            }
            Some((f, block)) => {
                let closes = fence(text).is_some_and(|(c, rest)| {
                    c.marker == f.marker && c.length >= f.length && rest.trim().is_empty()
                });
                if closes {
                    if let Some(mut block) = block {
                        block.end = offset;
                        block.closed = true;
                        found.push(block);
                    }
                } else {
                    open = Some((f, block));
                }
            }
        }
        offset = next;
    }
    if let Some((_, Some(mut block))) = open {
        block.end = note.len();
        found.push(block);
    }
    found
}

/// Picks one block: the one numbered `index`, else the only one.
pub fn select(note: &str, index: Option<usize>) -> Result<Block, String> {
    let all = blocks(note);
    match index {
        Some(i) => all.into_iter().nth(i.wrapping_sub(1)).ok_or_else(|| {
            format!(
                "there is no `dedalo` block {i}; the note has {}",
                blocks(note).len()
            )
        }),
        None => match all.len() {
            0 => Err(format!(
                "the note has no `{LANGUAGE}` code block; write the dungeon between ```{LANGUAGE} and ```"
            )),
            1 => Ok(all.into_iter().next().expect("one block")),
            n => Err(format!(
                "the note has {n} `dedalo` blocks (at lines {}); choose one by number",
                all.iter()
                    .map(|b| b.fence_line.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        },
    }
}

impl Block {
    /// The text between the fences, as written.
    pub fn content<'a>(&self, note: &'a str) -> &'a str {
        &note[self.start..self.end]
    }

    /// The note with every line outside the block emptied. Line breaks are
    /// kept, so positions in it are positions in the note.
    pub fn masked(&self, note: &str) -> String {
        let mut out = String::with_capacity(note.len());
        let mut offset = 0;
        for line in note.split_inclusive('\n') {
            let next = offset + line.len();
            if offset >= self.start && offset < self.end {
                out.push_str(line);
            } else {
                let body = line.trim_end_matches(['\n', '\r']);
                out.push_str(&line[body.len()..]);
            }
            offset = next;
        }
        out
    }

    /// An option of the fence.
    pub fn option(&self, key: &str) -> Option<&str> {
        self.options.get(key).map(String::as_str)
    }

    /// Applies the fence's options, except `export`, which chooses the spec
    /// itself. `levels` are the IDs of the dungeon's levels.
    pub fn apply(&self, spec: &mut crate::ExportSpec, levels: &[&str]) -> Result<(), String> {
        if let Some(p) = self.option("preset") {
            spec.preset = crate::Preset::parse(p)
                .ok_or_else(|| format!("`preset={p}` is not a preset; use dm, player or clean"))?;
        }
        if let Some(r) = self.option("rotation") {
            spec.rotation = r
                .parse()
                .ok()
                .filter(|r| [0, 90, 180, 270].contains(r))
                .ok_or_else(|| format!("`rotation={r}` must be 0, 90, 180 or 270"))?;
        }
        if let Some(l) = self.option("lang") {
            spec.language = dedalo_model::Language::parse(l)
                .ok_or_else(|| format!("`lang={l}` is not a language; use en or it"))?;
        }
        if let Some(level) = self.option("level") {
            if !levels.contains(&level) {
                return Err(if levels.is_empty() {
                    format!("`level={level}`, but the dungeon declares no levels")
                } else {
                    format!("`level={level}`, but the levels are {}", levels.join(", "))
                });
            }
            spec.level = Some(level.to_string());
        }
        for (option, toggle) in [
            ("grid", "grid"),
            ("labels", "room_labels"),
            ("notes", "notes"),
        ] {
            if let Some(value) = self.option(option) {
                let on = match value {
                    "true" | "yes" | "on" => true,
                    "false" | "no" | "off" => false,
                    other => return Err(format!("`{option}={other}` must be true or false")),
                };
                spec.toggles.insert(toggle.to_string(), on);
            }
        }
        Ok(())
    }

    /// Option names this module does not know, to warn about.
    pub fn unknown_options(&self) -> Vec<&str> {
        self.options
            .keys()
            .map(String::as_str)
            .filter(|k| !OPTIONS.iter().any(|(name, _)| name == k))
            .collect()
    }
}

/// The note's line break: `\r\n` when its first line ends with one.
fn newline(note: &str) -> &'static str {
    match note.find('\n') {
        Some(i) if i > 0 && note.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

/// The text without its leading and trailing blank lines. A tool given a
/// masked block returns it with the blanked lines around the dungeon.
pub fn trim_blank_lines(text: &str) -> &str {
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        if !line.trim().is_empty() {
            break;
        }
        start += line.len();
    }
    let rest = &text[start..];
    let end = rest.trim_end().len();
    &rest[..end]
}

/// Lays dungeon text out as a block body: the block's indentation and the
/// note's line breaks, ending with a line break.
fn body(note: &str, indent: usize, text: &str) -> String {
    let nl = newline(note);
    let pad = " ".repeat(indent);
    let mut out = String::new();
    for line in trim_blank_lines(text).lines() {
        let line = line.trim_end_matches('\r');
        if !line.trim().is_empty() {
            out.push_str(&pad);
            out.push_str(line);
        }
        out.push_str(nl);
    }
    out
}

/// The note with the block's content replaced by `text`, which may be a
/// masked note as a tool returns it. An unclosed block gets its fence.
pub fn replace(note: &str, block: &Block, text: &str) -> String {
    let mut out = String::with_capacity(note.len() + text.len());
    out.push_str(&note[..block.start]);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(newline(note));
    }
    out.push_str(&body(note, block.indent, text));
    if block.closed {
        out.push_str(&note[block.end..]);
    } else {
        out.push_str(&" ".repeat(block.indent));
        out.push_str("```");
        out.push_str(newline(note));
    }
    out
}

/// The note with a new block holding `text` added at its end.
pub fn append(note: &str, text: &str, info: &str) -> String {
    let nl = newline(note);
    let mut out = note.to_string();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push_str(nl);
        }
        if !out.ends_with(&format!("{nl}{nl}")) && out != nl {
            out.push_str(nl);
        }
    }
    out.push_str("```");
    out.push_str(LANGUAGE);
    if !info.trim().is_empty() {
        out.push(' ');
        out.push_str(info.trim());
    }
    out.push_str(nl);
    out.push_str(&body(note, 0, text));
    out.push_str("```");
    out.push_str(nl);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "# The crypt\n\nSome text.\n\n```dedalo player level=\"ground floor\"\ndungeon \"a\" {\n    room \"r\" { shape = rectangle(4, 4) }\n}\n```\n\n````markdown\n```dedalo\nnot a block\n```\n````\n\n~~~dedalo\ndungeon \"b\" {}\n~~~\n";

    #[test]
    fn finds_blocks_and_skips_other_fences() {
        let all = blocks(NOTE);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].index, 1);
        assert_eq!(all[0].fence_line, 5);
        assert!(all[0].content(NOTE).starts_with("dungeon \"a\""));
        assert_eq!(all[0].option("preset"), Some("player"));
        assert_eq!(all[0].option("level"), Some("ground floor"));
        assert_eq!(all[1].content(NOTE), "dungeon \"b\" {}\n");
        assert!(all[1].options.is_empty());
    }

    #[test]
    fn masking_keeps_line_numbers() {
        let block = select(NOTE, Some(1)).unwrap();
        let masked = block.masked(NOTE);
        assert_eq!(masked.lines().count(), NOTE.lines().count());
        assert_eq!(masked.lines().nth(5), Some("dungeon \"a\" {"));
        assert_eq!(masked.lines().nth(4), Some(""));
        assert_eq!(masked.lines().nth(8), Some(""));
        assert!(!masked.contains("dungeon \"b\""));
    }

    #[test]
    fn replacing_touches_only_the_block() {
        let block = select(NOTE, Some(2)).unwrap();
        let next = replace(NOTE, &block, "\n\ndungeon \"c\" {\n}\n\n");
        assert_eq!(
            next,
            NOTE.replace("dungeon \"b\" {}\n", "dungeon \"c\" {\n}\n")
        );
        // A masked note as a tool hands it back.
        let first = select(NOTE, Some(1)).unwrap();
        assert_eq!(replace(NOTE, &first, &first.masked(NOTE)), NOTE);
    }

    #[test]
    fn crlf_indentation_and_unclosed_blocks() {
        let note = "- item\r\n\r\n  ```dedalo\r\n  dungeon \"x\" {}\r\n  ```\r\n";
        let block = select(note, None).unwrap();
        assert_eq!(block.indent, 2);
        let next = replace(note, &block, "dungeon \"y\" {\n}\n");
        assert_eq!(
            next,
            "- item\r\n\r\n  ```dedalo\r\n  dungeon \"y\" {\r\n  }\r\n  ```\r\n"
        );
        let open = "```dedalo\ndungeon \"z\" {}";
        let block = select(open, None).unwrap();
        assert!(!block.closed);
        assert_eq!(
            replace(open, &block, "dungeon \"z\" {}\n"),
            "```dedalo\ndungeon \"z\" {}\n```\n"
        );
    }

    #[test]
    fn choosing_a_block() {
        assert!(
            select(NOTE, None)
                .unwrap_err()
                .contains("2 `dedalo` blocks")
        );
        assert!(select(NOTE, Some(3)).is_err());
        assert!(select("no code", None).unwrap_err().contains("no `dedalo`"));
    }

    #[test]
    fn appending_a_block() {
        assert_eq!(
            append("# Note\n", "dungeon \"a\" {}\n", "player"),
            "# Note\n\n```dedalo player\ndungeon \"a\" {}\n```\n"
        );
        assert_eq!(
            append("", "dungeon \"a\" {}", ""),
            "```dedalo\ndungeon \"a\" {}\n```\n"
        );
    }

    #[test]
    fn options() {
        let o = parse_options("clean rotation=90 grid=false export=\"a b\" mystery");
        assert_eq!(o["preset"], "clean");
        assert_eq!(o["rotation"], "90");
        assert_eq!(o["grid"], "false");
        assert_eq!(o["export"], "a b");
        assert_eq!(o["mystery"], "true");
    }
}
