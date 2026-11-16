//! "Did you mean" suggestions by edit distance (spec 10.3).

/// Levenshtein distance between two strings, counted in characters.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut diagonal = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let above = row[j];
            let cost = usize::from(a[i - 1] != b[j - 1]);
            row[j] = (diagonal + cost).min(above + 1).min(row[j - 1] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The candidate closest to `name`, when it is close enough to be a likely
/// misspelling. Ties go to the first candidate.
pub fn closest<'a, I>(name: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let length = name.chars().count();
    let limit = (length / 3).max(2);
    let lower = name.to_lowercase();
    candidates
        .into_iter()
        .map(|c| (distance(&lower, &c.to_lowercase()), c))
        // A candidate that shares no character with the name is not a misspelling.
        .filter(|(d, c)| *d <= limit && *d < c.chars().count().max(length))
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

/// ` Did you mean \`x\`?` or nothing.
pub fn did_you_mean<'a, I>(name: &str, candidates: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    closest(name, candidates)
        .map(|c| format!(" Did you mean `{c}`?"))
        .unwrap_or_default()
}

/// `a`, `a or b`, `a, b or c`.
pub fn one_of(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => only.to_string(),
        [head @ .., last] => format!("{} or {last}", head.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_finds_likely_misspellings_only() {
        let ids = ["entrance", "guard_room", "treasure_room"];
        assert_eq!(closest("gaurd_room", ids), Some("guard_room"));
        assert_eq!(closest("entrence", ids), Some("entrance"));
        assert_eq!(closest("kitchen", ids), None);
        assert_eq!(distance("kitten", "sitting"), 3);
    }
}
