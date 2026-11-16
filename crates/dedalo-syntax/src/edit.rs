//! Source edits: replacing byte ranges while leaving all other text intact.

/// Replaces `start..end` with `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

/// Applies non-overlapping edits. Text outside the edited ranges is preserved
/// byte for byte. Returns `None` when two edits overlap or one is out of range.
pub fn apply(source: &str, edits: &[TextEdit]) -> Option<String> {
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| (e.start, e.end));
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for e in sorted {
        let (start, end) = (e.start as usize, e.end as usize);
        if start < cursor
            || end < start
            || end > source.len()
            || !source.is_char_boundary(start)
            || !source.is_char_boundary(end)
        {
            return None;
        }
        out.push_str(&source[cursor..start]);
        out.push_str(&e.text);
        cursor = end;
    }
    out.push_str(&source[cursor..]);
    Some(out)
}
