//! A small, pragmatic subset of Maven version ordering and version ranges.
//!
//! Forge declares dependency ranges using Maven's range syntax. Vibe needs this
//! for dependency diagnostics, not to reproduce Maven's entire version algebra.

use std::cmp::Ordering;

use crate::error::{Result, VibeError};

#[derive(Debug, Clone)]
pub struct Version {
    parts: Vec<Part>,
    raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Numeric(u64),
    Text(String),
}

impl Version {
    pub fn parse(value: &str) -> Self {
        let raw = value.trim().trim_start_matches(['v', 'V']).to_string();
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut numeric = None::<bool>;
        let flush = |current: &mut String, numeric: &mut Option<bool>, parts: &mut Vec<Part>| {
            if current.is_empty() {
                return;
            }
            if *numeric == Some(true) {
                if let Ok(n) = current.parse::<u64>() {
                    parts.push(Part::Numeric(n));
                } else {
                    parts.push(Part::Text(current.to_ascii_lowercase()));
                }
            } else {
                parts.push(Part::Text(current.to_ascii_lowercase()));
            }
            current.clear();
            *numeric = None;
        };
        for ch in raw.chars() {
            if ch.is_ascii_digit() {
                if numeric == Some(false) {
                    flush(&mut current, &mut numeric, &mut parts);
                }
                numeric = Some(true);
                current.push(ch);
            } else if ch.is_ascii_alphabetic() {
                if numeric == Some(true) {
                    flush(&mut current, &mut numeric, &mut parts);
                }
                numeric = Some(false);
                current.push(ch);
            } else {
                flush(&mut current, &mut numeric, &mut parts);
            }
        }
        flush(&mut current, &mut numeric, &mut parts);
        Self { parts, raw }
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let max = self.parts.len().max(other.parts.len());
        for i in 0..max {
            match (self.parts.get(i), other.parts.get(i)) {
                (Some(Part::Numeric(a)), Some(Part::Numeric(b))) => match a.cmp(b) {
                    Ordering::Equal => continue,
                    other => return other,
                },
                (Some(Part::Text(a)), Some(Part::Text(b))) => match compare_qualifiers(a, b) {
                    Ordering::Equal => continue,
                    other => return other,
                },
                (Some(Part::Numeric(a)), Some(Part::Text(b))) => {
                    return compare_numeric_qualifier(*a, b)
                }
                (Some(Part::Text(a)), Some(Part::Numeric(b))) => {
                    return compare_numeric_qualifier(*b, a).reverse()
                }
                // A missing part is a release. A trailing numeric zero is
                // equivalent to no part (`1.0.0` == `1.0`); a text qualifier
                // sorts before the release (`1.0-rc1` < `1.0`), except `sp`.
                (Some(Part::Numeric(n)), None) => match n.cmp(&0) {
                    Ordering::Equal => continue,
                    other => return other,
                },
                (None, Some(Part::Numeric(n))) => match n.cmp(&0) {
                    Ordering::Equal => continue,
                    other => return other.reverse(),
                },
                (Some(Part::Text(text)), None) => match qualifier_rank(text).cmp(&0) {
                    Ordering::Equal => continue,
                    other => return other,
                },
                (None, Some(Part::Text(text))) => match qualifier_rank(text).cmp(&0) {
                    Ordering::Equal => continue,
                    other => return other.reverse(),
                },
                (None, None) => break,
            }
        }
        Ordering::Equal
    }
}

/// Maven's well-known qualifier order, collapsed to the cases mod metadata
/// actually uses. Unknown text sorts as a release and then lexicographically.
fn qualifier_rank(text: &str) -> i8 {
    match text {
        "alpha" | "a" => -5,
        "beta" | "b" => -4,
        "milestone" | "m" => -3,
        "rc" | "cr" => -2,
        "snapshot" | "dev" => -1,
        "final" | "ga" | "release" => 0,
        "sp" => 1,
        _ => 0,
    }
}

fn compare_qualifiers(left: &str, right: &str) -> Ordering {
    let ordering = qualifier_rank(left).cmp(&qualifier_rank(right));
    if ordering == Ordering::Equal {
        left.cmp(right)
    } else {
        ordering
    }
}

fn compare_numeric_qualifier(number: u64, text: &str) -> Ordering {
    match qualifier_rank(text).cmp(&0) {
        Ordering::Less => Ordering::Greater,
        Ordering::Greater => Ordering::Less,
        Ordering::Equal => number.cmp(&0),
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRange {
    bounds: Vec<Bound>,
    raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Bound {
    lower: Option<Version>,
    lower_inclusive: bool,
    upper: Option<Version>,
    upper_inclusive: bool,
}

impl VersionRange {
    /// Accepts Maven's single-range form: `[1.0,2.0)`, `(,1.0]`, `[1.0,)`,
    /// `[1.0]`, a bare version, `*`, or a comma-separated union of ranges.
    pub fn parse(value: &str) -> Result<Self> {
        let raw = value.trim().to_string();
        if raw.is_empty() || raw == "*" || raw.eq_ignore_ascii_case("any") {
            return Ok(Self {
                bounds: vec![Bound {
                    lower: None,
                    lower_inclusive: true,
                    upper: None,
                    upper_inclusive: true,
                }],
                raw,
            });
        }
        if !raw.starts_with(['[', '(']) {
            let version = Version::parse(&raw);
            return Ok(Self {
                bounds: vec![Bound {
                    lower: Some(version.clone()),
                    lower_inclusive: true,
                    upper: Some(version),
                    upper_inclusive: true,
                }],
                raw,
            });
        }

        let mut bounds = Vec::new();
        let mut rest = raw.as_str();
        while !rest.is_empty() {
            let start = rest
                .chars()
                .next()
                .filter(|c| matches!(c, '[' | '('))
                .ok_or_else(|| VibeError::Metadata(format!("invalid Maven range {value:?}")))?;
            let end_index = rest[1..].find([']', ')']).map(|i| i + 1).ok_or_else(|| {
                VibeError::Metadata(format!("unterminated Maven range {value:?}"))
            })?;
            let end = rest.as_bytes()[end_index] as char;
            let inner = &rest[1..end_index];
            let (lower_raw, upper_raw) = inner.split_once(',').unwrap_or((inner, inner));
            let lower = if lower_raw.trim().is_empty() {
                None
            } else {
                Some(Version::parse(lower_raw))
            };
            let upper = if upper_raw.trim().is_empty() {
                None
            } else {
                Some(Version::parse(upper_raw))
            };
            // `[1.0]` means exactly 1.0.
            let exact = !inner.contains(',');
            bounds.push(Bound {
                lower: lower.clone(),
                lower_inclusive: start == '[',
                upper: if exact { lower } else { upper },
                upper_inclusive: end == ']',
            });
            rest = &rest[end_index + 1..];
            rest = rest.trim_start_matches(',');
        }
        if bounds.is_empty() {
            return Err(VibeError::Metadata(format!(
                "invalid Maven range {value:?}"
            )));
        }
        Ok(Self { bounds, raw })
    }

    pub fn matches(&self, version: &str) -> bool {
        let version = Version::parse(version);
        self.bounds.iter().any(|bound| {
            let lower_ok = match &bound.lower {
                None => true,
                Some(lower) => match version.cmp(lower) {
                    Ordering::Greater => true,
                    Ordering::Equal => bound.lower_inclusive,
                    Ordering::Less => false,
                },
            };
            let upper_ok = match &bound.upper {
                None => true,
                Some(upper) => match version.cmp(upper) {
                    Ordering::Less => true,
                    Ordering::Equal => bound.upper_inclusive,
                    Ordering::Greater => false,
                },
            };
            lower_ok && upper_ok
        })
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_common_versions() {
        assert!(Version::parse("1.20.1") < Version::parse("1.21"));
        assert!(Version::parse("1.0") > Version::parse("1.0-rc1"));
        assert_eq!(Version::parse("1.0.0"), Version::parse("1.0"));
    }

    #[test]
    fn matches_ranges() {
        assert!(VersionRange::parse("[52,)").unwrap().matches("52.0.1"));
        assert!(!VersionRange::parse("[52,)").unwrap().matches("51"));
        assert!(VersionRange::parse("[1.20,1.21)")
            .unwrap()
            .matches("1.20.1"));
        assert!(!VersionRange::parse("[1.20,1.21)").unwrap().matches("1.21"));
        assert!(VersionRange::parse("[1.20]").unwrap().matches("1.20"));
        assert!(VersionRange::parse("*").unwrap().matches("anything"));
    }
}
