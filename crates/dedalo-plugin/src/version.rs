//! Version numbers and requirements: `1.2.0`, `^1.2`, `~1.2`, `>=0.2`, `=1.0.1`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let core = text.trim().split(['-', '+']).next()?;
        let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
        let major = parts.next()??;
        let minor = parts.next().unwrap_or(Some(0))?;
        let patch = parts.next().unwrap_or(Some(0))?;
        if parts.next().is_some() {
            return None;
        }
        Some(Version(major, minor, patch))
    }
}

/// Whether `version` satisfies `requirement`. Several comparators may be
/// joined with commas; all must hold.
pub fn matches(requirement: &str, version: &str) -> Result<bool, String> {
    let v = Version::parse(version).ok_or_else(|| format!("`{version}` is not a version"))?;
    for part in requirement
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let (op, rest) = ["^", "~", ">=", "<=", ">", "<", "="]
            .iter()
            .find_map(|op| part.strip_prefix(op).map(|r| (*op, r)))
            .unwrap_or(("^", part));
        let r = Version::parse(rest)
            .ok_or_else(|| format!("`{requirement}` is not a version requirement such as ^1.2"))?;
        let parts = rest.trim().split('.').count();
        let ok = match op {
            "^" => {
                // Compatible: same left-most non-zero component.
                let upper = if r.0 > 0 || parts == 1 {
                    Version(r.0 + 1, 0, 0)
                } else if r.1 > 0 || parts == 2 {
                    Version(0, r.1 + 1, 0)
                } else {
                    Version(0, 0, r.2 + 1)
                };
                v >= r && v < upper
            }
            "~" => {
                let upper = if parts == 1 {
                    Version(r.0 + 1, 0, 0)
                } else {
                    Version(r.0, r.1 + 1, 0)
                };
                v >= r && v < upper
            }
            ">=" => v >= r,
            "<=" => v <= r,
            ">" => v > r,
            "<" => v < r,
            _ => v == r,
        };
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn requirements() {
        assert!(matches("^1.2", "1.4.0").unwrap());
        assert!(!matches("^1.2", "2.0.0").unwrap());
        assert!(!matches("^1.2", "1.1.9").unwrap());
        assert!(matches("^0.3", "0.3.7").unwrap());
        assert!(!matches("^0.3", "0.4.0").unwrap());
        assert!(matches("~1.2", "1.2.9").unwrap());
        assert!(!matches("~1.2", "1.3.0").unwrap());
        assert!(matches(">=0.2, <1", "0.9.1").unwrap());
        assert!(matches("1.2.0", "1.2.5").unwrap());
        assert!(matches("=1.2.0", "1.2.0").unwrap());
        assert!(matches("x", "1.0.0").is_err());
    }
}
