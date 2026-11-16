//! Unified diffs of two texts, line by line.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Line {
    Same(usize, usize),
    Del(usize),
    Ins(usize),
}

/// The edit script between two line sequences: common prefix and suffix,
/// and a longest common subsequence on what is left in between.
fn script(a: &[&str], b: &[&str]) -> Vec<Line> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (ma, mb) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    let mut out: Vec<Line> = (0..prefix).map(|i| Line::Same(i, i)).collect();
    if ma.len() * mb.len() <= 4_000_000 {
        // lcs[i][j]: length of the LCS of ma[i..] and mb[j..].
        let (n, m) = (ma.len(), mb.len());
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if ma[i] == mb[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && ma[i] == mb[j] {
                out.push(Line::Same(prefix + i, prefix + j));
                i += 1;
                j += 1;
            } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
                out.push(Line::Del(prefix + i));
                i += 1;
            } else {
                out.push(Line::Ins(prefix + j));
                j += 1;
            }
        }
    } else {
        out.extend((0..ma.len()).map(|i| Line::Del(prefix + i)));
        out.extend((0..mb.len()).map(|j| Line::Ins(prefix + j)));
    }
    let (sa, sb) = (a.len() - suffix, b.len() - suffix);
    out.extend((0..suffix).map(|k| Line::Same(sa + k, sb + k)));
    out
}

/// A unified diff with three lines of context. Empty when the texts match.
pub fn unified(old: &str, new: &str, name: &str) -> String {
    if old == new {
        return String::new();
    }
    let a: Vec<&str> = old.split_inclusive('\n').collect();
    let b: Vec<&str> = new.split_inclusive('\n').collect();
    let lines = script(&a, &b);
    let changed: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !matches!(l, Line::Same(..)))
        .map(|(k, _)| k)
        .collect();
    let mut out = format!("--- a/{name}\n+++ b/{name}\n");
    let mut k = 0;
    while k < changed.len() {
        // Grow a hunk while the next change is within six lines.
        let start = changed[k].saturating_sub(3);
        let mut end = changed[k];
        while k + 1 < changed.len() && changed[k + 1] <= end + 7 {
            k += 1;
            end = changed[k];
        }
        let end = (end + 4).min(lines.len());
        let hunk = &lines[start..end];
        let a_start = hunk.iter().find_map(|l| match l {
            Line::Same(i, _) | Line::Del(i) => Some(*i),
            Line::Ins(_) => None,
        });
        let b_start = hunk.iter().find_map(|l| match l {
            Line::Same(_, j) | Line::Ins(j) => Some(*j),
            Line::Del(_) => None,
        });
        let a_len = hunk.iter().filter(|l| !matches!(l, Line::Ins(_))).count();
        let b_len = hunk.iter().filter(|l| !matches!(l, Line::Del(_))).count();
        let first = |start: Option<usize>, len: usize| {
            if len == 0 {
                start.unwrap_or(0)
            } else {
                start.unwrap_or(0) + 1
            }
        };
        out.push_str(&format!(
            "@@ -{},{a_len} +{},{b_len} @@\n",
            first(a_start, a_len),
            first(b_start, b_len)
        ));
        for line in hunk {
            let (mark, text) = match *line {
                Line::Same(i, _) => (' ', a[i]),
                Line::Del(i) => ('-', a[i]),
                Line::Ins(j) => ('+', b[j]),
            };
            out.push(mark);
            out.push_str(text.strip_suffix('\n').unwrap_or(text));
            out.push('\n');
            if !text.ends_with('\n') {
                out.push_str("\\ No newline at end of file\n");
            }
        }
        k += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_line_change_gives_one_hunk() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nd\nE\nf\ng\nh\n";
        assert_eq!(
            unified(old, new, "x"),
            "--- a/x\n+++ b/x\n@@ -2,7 +2,7 @@\n b\n c\n d\n-e\n+E\n f\n g\n h\n"
        );
        assert_eq!(unified(old, old, "x"), "");
        assert_eq!(
            unified("", "a\n", "x"),
            "--- a/x\n+++ b/x\n@@ -0,0 +1,1 @@\n+a\n"
        );
    }
}
