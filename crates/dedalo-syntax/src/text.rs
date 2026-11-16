//! Byte offsets to line and column positions.

/// Maps byte offsets to 1-based lines and columns. Columns count characters,
/// not bytes, so they match what an editor shows.
#[derive(Debug, Clone)]
pub struct LineIndex<'s> {
    src: &'s str,
    line_starts: Vec<usize>,
}

impl<'s> LineIndex<'s> {
    pub fn new(src: &'s str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            src.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self { src, line_starts }
    }

    /// `[line, column]` of a byte offset, both starting at 1.
    pub fn line_col(&self, offset: u32) -> [u32; 2] {
        let offset = (offset as usize).min(self.src.len());
        let line = self.line_starts.partition_point(|&s| s <= offset) - 1;
        let col = self.src[self.line_starts[line]..offset].chars().count();
        [line as u32 + 1, col as u32 + 1]
    }

    /// The text of a 1-based line, without its line break.
    pub fn line_text(&self, line: u32) -> &'s str {
        let i = (line as usize).saturating_sub(1);
        let Some(&start) = self.line_starts.get(i) else {
            return "";
        };
        let end = self
            .line_starts
            .get(i + 1)
            .map_or(self.src.len(), |&e| e - 1);
        self.src[start..end.max(start)].trim_end_matches('\r')
    }
}
