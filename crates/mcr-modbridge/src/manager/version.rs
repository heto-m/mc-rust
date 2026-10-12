use std::cmp::Ordering;

const MAX_PARTS: usize = 16;
const MAX_TOKENS: usize = 32;
const MAX_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ident {
    Num(u64),
    Text(String),
}

impl Ord for Ident {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Num(a), Self::Num(b)) => a.cmp(b),
            (Self::Num(_), Self::Text(_)) => Ordering::Less,
            (Self::Text(_), Self::Num(_)) => Ordering::Greater,
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for Ident {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A mod or game version. Semantic versions compare by components; anything else is an opaque
/// string that only matches `*` or itself, like Fabric's fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    raw: String,
    parts: Option<Vec<u64>>,
    pre: Vec<Ident>,
}

impl Version {
    pub fn parse(text: &str) -> Self {
        let raw: String = text.trim().chars().take(MAX_LEN).collect();
        let (core, pre) = split_semver(&raw);
        let parts = parse_parts(core);
        Self { raw, parts, pre }
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    fn compare(&self, other: &Self) -> Option<Ordering> {
        let (a, b) = (self.parts.as_ref()?, other.parts.as_ref()?);
        for i in 0..a.len().max(b.len()) {
            let ord = a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0));
            if ord != Ordering::Equal {
                return Some(ord);
            }
        }
        Some(match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => self.pre.cmp(&other.pre),
        })
    }
}

fn split_semver(raw: &str) -> (&str, Vec<Ident>) {
    let no_build = raw.split('+').next().unwrap_or(raw);
    match no_build.split_once('-') {
        Some((core, pre)) => (
            core,
            pre.split('.')
                .map(|p| {
                    p.parse()
                        .map_or_else(|_| Ident::Text(p.to_owned()), Ident::Num)
                })
                .collect(),
        ),
        None => (no_build, Vec::new()),
    }
}

fn parse_parts(core: &str) -> Option<Vec<u64>> {
    let parts: Option<Vec<u64>> = core.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| p.len() <= MAX_PARTS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Any,
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
    Tilde,
    Caret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Comparator {
    op: Op,
    /// Components before the first wildcard (`x`, `X`, `*`).
    prefix: Vec<u64>,
    wildcard: bool,
    pre: Vec<Ident>,
}

impl Comparator {
    fn parse(token: &str) -> Option<Self> {
        let (op, rest) = [
            (">=", Op::Ge),
            ("<=", Op::Le),
            (">", Op::Gt),
            ("<", Op::Lt),
            ("=", Op::Eq),
            ("~", Op::Tilde),
            ("^", Op::Caret),
        ]
        .iter()
        .find_map(|(p, op)| token.strip_prefix(p).map(|r| (*op, r)))
        .unwrap_or((Op::Eq, token));
        if rest == "*" && op == Op::Eq {
            return Some(Self {
                op: Op::Any,
                prefix: Vec::new(),
                wildcard: true,
                pre: Vec::new(),
            });
        }
        let (core, pre) = split_semver(rest);
        let mut prefix = Vec::new();
        let mut wildcard = false;
        for part in core.split('.') {
            if matches!(part, "x" | "X" | "*") {
                wildcard = true;
                break;
            }
            prefix.push(part.parse().ok()?);
        }
        (prefix.len() <= MAX_PARTS && !(prefix.is_empty() && !wildcard)).then_some(Self {
            op,
            prefix,
            wildcard,
            pre,
        })
    }

    fn bound(&self, parts: &[u64]) -> Version {
        Version {
            raw: String::new(),
            parts: Some(parts.to_vec()),
            pre: Vec::new(),
        }
    }

    fn lower(&self) -> Version {
        Version {
            raw: String::new(),
            parts: Some(self.prefix.clone()),
            pre: if self.wildcard {
                Vec::new()
            } else {
                self.pre.clone()
            },
        }
    }

    /// The first version above everything this comparator's prefix covers.
    fn upper(&self) -> Option<Version> {
        let last = self.prefix.len().checked_sub(1)?;
        let mut parts = self.prefix.clone();
        parts[last] = parts[last].saturating_add(1);
        Some(self.bound(&parts))
    }

    fn matches(&self, v: &Version) -> bool {
        if self.op == Op::Any {
            return true;
        }
        let Some(vparts) = &v.parts else {
            return false;
        };
        let at_least = |b: &Version| v.compare(b).is_some_and(|o| o != Ordering::Less);
        let below = |b: &Version| v.compare(b) == Some(Ordering::Less);
        match self.op {
            Op::Any => true,
            Op::Eq if self.wildcard => self
                .prefix
                .iter()
                .enumerate()
                .all(|(i, p)| vparts.get(i).unwrap_or(&0) == p),
            Op::Eq => v.compare(&self.lower()) == Some(Ordering::Equal),
            Op::Ge => at_least(&self.lower()),
            Op::Gt if self.wildcard => self.upper().is_none_or(|u| at_least(&u)),
            Op::Gt => v.compare(&self.lower()) == Some(Ordering::Greater),
            Op::Lt => below(&self.lower()),
            Op::Le if self.wildcard => self.upper().is_none_or(|u| below(&u)),
            Op::Le => v
                .compare(&self.lower())
                .is_some_and(|o| o != Ordering::Greater),
            Op::Tilde => {
                let idx = if self.prefix.len() >= 2 { 1 } else { 0 };
                let mut up = self.prefix.clone();
                up.truncate(idx + 1);
                if let Some(slot) = up.get_mut(idx) {
                    *slot = slot.saturating_add(1);
                }
                at_least(&self.lower()) && below(&self.bound(&up))
            }
            Op::Caret => {
                let up = [self.prefix.first().copied().unwrap_or(0).saturating_add(1)];
                at_least(&self.lower()) && below(&self.bound(&up))
            }
        }
    }
}

/// A Fabric dependency range: alternatives (array form) of space-separated comparators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    alternatives: Vec<Vec<Comparator>>,
}

impl Range {
    /// `None` when a comparator is malformed, so the caller can report it instead of guessing.
    pub fn parse<S: AsRef<str>>(alternatives: &[S]) -> Option<Self> {
        let mut out = Vec::new();
        for alt in alternatives.iter().take(MAX_TOKENS) {
            let text = alt.as_ref();
            if text.len() > MAX_LEN * 2 {
                return None;
            }
            let tokens: Vec<&str> = text.split_whitespace().take(MAX_TOKENS).collect();
            if tokens.is_empty() {
                out.push(Vec::new());
                continue;
            }
            out.push(
                tokens
                    .into_iter()
                    .map(Comparator::parse)
                    .collect::<Option<Vec<_>>>()?,
            );
        }
        if out.is_empty() {
            out.push(Vec::new());
        }
        Some(Self { alternatives: out })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.alternatives
            .iter()
            .any(|all| all.iter().all(|c| c.matches(version)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(range: &[&str], version: &str) -> bool {
        Range::parse(range)
            .unwrap_or_else(|| panic!("{range:?} does not parse"))
            .matches(&Version::parse(version))
    }

    #[test]
    fn any_and_exact() {
        assert!(ok(&["*"], "1.2.3"));
        assert!(ok(&["*"], "weird-version"));
        assert!(ok(&["26.3"], "26.3"));
        assert!(ok(&["26.3"], "26.3.0"));
        assert!(!ok(&["26.3"], "26.3.1"));
        assert!(ok(&["=1.0.0"], "1.0.0+build.5"));
        assert!(ok(&["1.0.0-beta"], "1.0.0-beta"));
        assert!(!ok(&["1.0.0"], "1.0.0-beta"));
    }

    #[test]
    fn comparisons() {
        assert!(ok(&[">=0.19.3"], "0.19.5"));
        assert!(ok(&[">=0.19.3"], "0.19.3"));
        assert!(!ok(&[">=0.19.3"], "0.19.2"));
        assert!(ok(&[">=0.19.3"], "1.0"));
        assert!(ok(&[">1.0"], "1.0.1"));
        assert!(!ok(&[">1.0"], "1.0.0"));
        assert!(ok(&["<2"], "1.99.99"));
        assert!(!ok(&["<2"], "2.0.0"));
        assert!(ok(&["<=2"], "2.0.0"));
        assert!(!ok(&["<=2"], "2.0.1"));
        assert!(ok(&["<1.0.0"], "1.0.0-alpha"));
        assert!(ok(&[">=25"], "25"));
    }

    #[test]
    fn tilde() {
        assert!(ok(&["~1.2.3"], "1.2.9"));
        assert!(ok(&["~1.2.3"], "1.2.3"));
        assert!(!ok(&["~1.2.3"], "1.2.2"));
        assert!(!ok(&["~1.2.3"], "1.3.0"));
        assert!(ok(&["~1.2"], "1.2.40"));
        assert!(!ok(&["~1.2"], "1.3"));
        assert!(ok(&["~1"], "1.9"));
        assert!(!ok(&["~1"], "2.0"));
    }

    #[test]
    fn caret() {
        assert!(ok(&["^1.2.3"], "1.9.0"));
        assert!(!ok(&["^1.2.3"], "1.2.2"));
        assert!(!ok(&["^1.2.3"], "2.0.0"));
        assert!(ok(&["^0.5"], "0.9"));
    }

    #[test]
    fn wildcards() {
        assert!(ok(&["1.20.x"], "1.20.4"));
        assert!(ok(&["1.20.x"], "1.20"));
        assert!(!ok(&["1.20.x"], "1.21.0"));
        assert!(ok(&["1.X"], "1.7.1"));
        assert!(!ok(&["1.x"], "2.0"));
        assert!(ok(&[">=1.20.x"], "1.20.0"));
        assert!(!ok(&[">=1.20.x"], "1.19.9"));
        assert!(ok(&[">1.20.x"], "1.21"));
        assert!(!ok(&[">1.20.x"], "1.20.9"));
        assert!(ok(&["<=1.20.x"], "1.20.9"));
        assert!(!ok(&["<=1.20.x"], "1.21"));
        assert!(ok(&["<1.20.x"], "1.19.9"));
        assert!(!ok(&["<1.20.x"], "1.20.0"));
    }

    #[test]
    fn space_is_and_array_is_or() {
        assert!(ok(&[">=1.0 <2.0"], "1.5"));
        assert!(!ok(&[">=1.0 <2.0"], "2.0"));
        assert!(!ok(&[">=1.0 <2.0"], "0.9"));
        assert!(ok(&["1.0", "3.x"], "3.4"));
        assert!(ok(&["1.0", "3.x"], "1.0"));
        assert!(!ok(&["1.0", "3.x"], "2.0"));
    }

    #[test]
    fn prerelease_ordering() {
        assert!(ok(&[">=1.0.0-alpha"], "1.0.0-beta"));
        assert!(ok(&[">1.0.0-beta.2"], "1.0.0-beta.10"));
        assert!(ok(&[">1.0.0-rc"], "1.0.0"));
        assert!(!ok(&[">=1.0.0"], "1.0.0-rc.1"));
    }

    #[test]
    fn non_semantic_versions_only_match_themselves() {
        assert!(!ok(&[">=1.0"], "snapshot-abc"));
        assert!(Range::parse(&["snapshot"]).is_none());
        assert!(!ok(&["1.x"], "release"));
    }

    #[test]
    fn malformed_ranges_are_rejected() {
        assert!(Range::parse(&["foo"]).is_none());
        assert!(Range::parse(&[">=1.a"]).is_none());
        assert!(Range::parse(&["x"]).is_some());
        let long = "1.".repeat(300);
        assert!(Range::parse(&[long.as_str()]).is_none());
    }

    #[test]
    fn empty_range_matches_everything() {
        assert!(ok(&[""], "1.0"));
        let none: [&str; 0] = [];
        assert!(Range::parse(&none).unwrap().matches(&Version::parse("1")));
    }

    #[test]
    fn hostile_versions_do_not_blow_up() {
        let v = Version::parse(&"1.".repeat(5000));
        assert!(v.raw().len() <= MAX_LEN);
        let _ = Range::parse(&["*"]).unwrap().matches(&v);
    }
}
