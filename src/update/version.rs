//! Release versions. Tags are two-part (`v0.10`, `1.0`) or three-part for a
//! fix release (`v0.10.1`); Cargo and the download names use three parts.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// The running game's version.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("Cargo version is three numbers")
    }

    /// `v0.10`, `0.10`, `v0.10.1` or `0.10.0`. Anything else, pre-release
    /// suffixes included, is `None`.
    pub fn parse(tag: &str) -> Option<Self> {
        let parts: Vec<&str> = tag.strip_prefix('v').unwrap_or(tag).split('.').collect();
        if !(2..=3).contains(&parts.len()) {
            return None;
        }
        let mut n = [0u32; 3];
        for (slot, part) in n.iter_mut().zip(&parts) {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *slot = part.parse().ok()?;
        }
        Some(Self(n[0], n[1], n[2]))
    }

    /// The name players see: `0.11`, or `0.11.1` for a fix release.
    pub fn short(self) -> String {
        match self {
            Self(major, minor, 0) => format!("{major}.{minor}"),
            v => v.to_string(),
        }
    }
}

/// Cargo's form, `0.11.0`, as used in download names.
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_tags() {
        assert_eq!(Version::parse("v0.10"), Some(Version(0, 10, 0)));
        assert_eq!(Version::parse("0.10"), Some(Version(0, 10, 0)));
        assert_eq!(Version::parse("1.0"), Some(Version(1, 0, 0)));
        assert_eq!(Version::parse("v0.10.1"), Some(Version(0, 10, 1)));
        assert_eq!(Version::parse("0.9.0"), Some(Version(0, 9, 0)));
        for bad in ["", "v", "1", "v1.2.3.4", "v0.10-rc1", "0.x", "0..1", "v-1.0", " 0.10", "release"] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn orders_numerically() {
        assert!(Version(0, 10, 0) > Version(0, 9, 0));
        assert!(Version(0, 10, 1) > Version(0, 10, 0));
        assert!(Version(1, 0, 0) > Version(0, 19, 3));
        assert_eq!(Version(0, 11, 0).to_string(), "0.11.0");
        assert_eq!(Version(0, 11, 0).short(), "0.11");
        assert_eq!(Version(0, 11, 2).short(), "0.11.2");
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
