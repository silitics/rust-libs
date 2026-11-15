//! Scheme-agnostic version parsing and requirement matching.
//!
//! Versions use one ordered grammar covering `SemVer`, `CalVer`, serial versions, and
//! date-plus-revision versions without assuming any particular scheme up front:
//!
//! - one or more dot-separated numeric release components, e.g. `5`, `5.2`, `5.2.0`,
//!   `2026.6.25`;
//! - an optional prerelease suffix introduced by `-`, e.g. `5.2.0-rc.1`;
//! - optional build metadata introduced by `+`, e.g. `5.2.0+build.7`.
//!
//! Versions are ordered by _precedence_. Release components compare numerically
//! with trailing zero components ignored, so `5`, `5.0`, and `5.0.0` have the
//! same precedence. Prerelease versions precede the corresponding final release.
//! Numeric release and prerelease identifiers may contain leading zeroes, which
//! do not affect precedence. Build metadata does not affect precedence either.
//! Requirement matching uses precedence, so `==5` matches `5.0` and `5+build.7`.
//!
//! [`Version`] values keep their exact spelling. Equality and hashing compare
//! the spelling, so `5`, `5.0`, and `5+build.7` are different values. [`Ord`]
//! orders by precedence and breaks ties by spelling, so it is consistent with
//! equality.
//!
//! A [`VersionReq`] is a comma-separated conjunction of [`Comparator`]s. A
//! comparator's version without a prerelease names a _line_: all versions whose
//! release components start with the comparator's release components, including
//! their prereleases. The line `3` contains `3.0-rc.1`, `3`, and `3.5.1`, and the
//! line `3.0` contains `3.0-rc.1` and `3.0.5`, but not `3.1`. Comparators refer
//! to the start or the end of the line:
//!
//! - `>=3` matches every version from the start of the line `3`, including `3.0-rc.1`.
//! - `<4` matches every version before the start of the line `4`, so it does not match
//!   `4.0-rc.1`.
//! - `>3` matches every version after the end of the line `3`, so it matches `4.0-rc.1`,
//!   but not `3.1`. `>3.0` matches `3.1`.
//! - `<=3` matches every version up to the end of the line `3`, such as `3.9`.
//! - `3.*`, or `==3.*`, matches every version of the line `3`, and `!=3.*` matches every
//!   other version.
//! - `==3` and `!=3` compare exact versions, so `==3` matches `3.0`, but not `3.0-rc.1`
//!   or `3.0.1`. A bare version, such as `3`, means `==3`.
//!
//! A requirement like `>=3,<4` therefore matches the whole line `3`, including
//! its prereleases, but not `4.0-rc.1`. A comparator's version with a
//! prerelease, such as `>=3-rc.2` or `<4-rc.2`, compares in the total order.
//!
//! Serde support (behind the `serde` feature, enabled by default) uses string
//! representations for [`Version`], [`VersionReq`], [`Comparator`], and
//! [`Operator`]. Deserialization validates the same grammar as the
//! corresponding parser.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

#[cfg(feature = "serde")]
use serde::Deserialize;
#[cfg(feature = "serde")]
use serde::Deserializer;
#[cfg(feature = "serde")]
use serde::Serialize;
#[cfg(feature = "serde")]
use serde::Serializer;

/// A normalized version.
#[derive(Debug, Clone)]
pub struct Version {
    display: String,
    release: Vec<u64>,
    prerelease: Vec<PrereleaseIdentifier>,
    build_metadata: Vec<String>,
}

impl Version {
    /// Parse a version.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if `input` does not follow the version grammar
    /// described at the crate root.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        input.parse()
    }

    /// Numeric release components as parsed.
    ///
    /// Trailing zero components are preserved here but do not affect precedence.
    #[must_use]
    pub fn release(&self) -> &[u64] {
        &self.release
    }

    /// Whether this version has a prerelease suffix.
    #[must_use]
    pub fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }

    /// Build metadata identifiers.
    ///
    /// Build metadata does not affect precedence or requirement matching.
    #[must_use]
    pub fn build_metadata(&self) -> &[String] {
        &self.build_metadata
    }
}

impl FromStr for Version {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ParseError::new(input, "version must not be empty"));
        }
        let (version_part, build_metadata_part) = match input.split_once('+') {
            Some((_, "")) => {
                return Err(ParseError::new(input, "build metadata must not be empty"));
            }
            Some((version, build_metadata)) => (version, build_metadata),
            None => (input, ""),
        };
        let build_metadata = parse_build_metadata(input, build_metadata_part)?;

        let (release_part, prerelease_part) = match version_part.split_once('-') {
            Some((_, "")) => {
                return Err(ParseError::new(input, "prerelease part must not be empty"));
            }
            Some((release, prerelease)) => (release, prerelease),
            None => (version_part, ""),
        };
        if release_part.is_empty() {
            return Err(ParseError::new(input, "release part must not be empty"));
        }

        let mut release = Vec::new();
        for component in release_part.split('.') {
            if component.is_empty() {
                return Err(ParseError::new(
                    input,
                    "release components must not be empty",
                ));
            }
            if !component.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ParseError::new(input, "release components must be numeric"));
            }
            release.push(
                component.parse::<u64>().map_err(|_| {
                    ParseError::new(input, "release component does not fit into u64")
                })?,
            );
        }

        let prerelease = if prerelease_part.is_empty() {
            Vec::new()
        } else {
            let mut identifiers = Vec::new();
            for identifier in prerelease_part.split('.') {
                if identifier.is_empty() {
                    return Err(ParseError::new(
                        input,
                        "prerelease identifiers must not be empty",
                    ));
                }
                if !identifier
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                {
                    return Err(ParseError::new(
                        input,
                        "prerelease identifiers must be ASCII alphanumeric or hyphenated",
                    ));
                }
                identifiers.push(PrereleaseIdentifier::parse(identifier, input)?);
            }
            identifiers
        };

        Ok(Self {
            display: input.to_owned(),
            release,
            prerelease,
            build_metadata,
        })
    }
}

impl Version {
    /// Compare by precedence, ignoring spelling and build metadata.
    fn precedence(&self, other: &Self) -> Ordering {
        match compare_release(&self.release, &other.release) {
            Ordering::Equal => {}
            non_equal => return non_equal,
        }

        match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => self.prerelease.cmp(&other.prerelease),
        }
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.display == other.display
    }
}

impl Eq for Version {}

impl Hash for Version {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.display.hash(state);
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.precedence(other)
            .then_with(|| self.display.cmp(&other.display))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display)
    }
}

#[cfg(feature = "serde")]
impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.display)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

/// A conjunction of version comparators.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionReq {
    comparators: Vec<Comparator>,
}

impl VersionReq {
    /// A requirement matching every version.
    #[must_use]
    pub fn any() -> Self {
        Self::default()
    }

    /// Parse a version requirement.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if `input` is not a comma-separated list of
    /// comparators (or `*`) following the grammar described at the crate root.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        input.parse()
    }

    /// Comparators in this requirement.
    #[must_use]
    pub fn comparators(&self) -> &[Comparator] {
        &self.comparators
    }

    /// Whether this requirement matches every version.
    #[must_use]
    pub fn is_any(&self) -> bool {
        self.comparators.is_empty()
    }

    /// Whether this requirement matches the given version.
    #[must_use]
    pub fn matches(&self, version: &Version) -> bool {
        self.comparators
            .iter()
            .all(|comparator| comparator.matches(version))
    }
}

impl FromStr for VersionReq {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input == "*" {
            return Ok(Self::any());
        }
        if input.is_empty() {
            return Err(ParseError::new(
                input,
                "version requirement must not be empty",
            ));
        }

        let mut comparators = Vec::new();
        for part in input.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(ParseError::new(
                    input,
                    "version requirement contains an empty comparator",
                ));
            }
            comparators.push(parse_comparator(input, part)?);
        }
        Ok(Self { comparators })
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.comparators.is_empty() {
            return f.write_str("*");
        }
        for (idx, comparator) in self.comparators.iter().enumerate() {
            if idx > 0 {
                f.write_str(",")?;
            }
            write!(f, "{comparator}")?;
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl Serialize for VersionReq {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for VersionReq {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

/// A single version comparator.
///
/// The crate root describes how comparators match versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparator {
    /// Comparison operator.
    pub op: Operator,
    /// Version to compare against.
    pub version: Version,
    /// Whether the comparator matches the whole line of its version, as in `3.*`.
    ///
    /// Only [`Operator::Equal`] and [`Operator::NotEqual`] support wildcards, and the
    /// version must not have a prerelease.
    pub wildcard: bool,
}

impl Comparator {
    /// Whether this comparator matches the given version.
    #[must_use]
    pub fn matches(&self, version: &Version) -> bool {
        if self.wildcard {
            let in_line = self.compare_line(version) == Ordering::Equal;
            return match self.op {
                Operator::NotEqual => !in_line,
                _ => in_line,
            };
        }
        if self.version.is_prerelease() {
            return match self.op {
                Operator::Equal => version.precedence(&self.version).is_eq(),
                Operator::NotEqual => version.precedence(&self.version).is_ne(),
                Operator::Greater => version.precedence(&self.version).is_gt(),
                Operator::GreaterEq => version.precedence(&self.version).is_ge(),
                Operator::Less => version.precedence(&self.version).is_lt(),
                Operator::LessEq => version.precedence(&self.version).is_le(),
            };
        }
        match self.op {
            Operator::Equal => version.precedence(&self.version).is_eq(),
            Operator::NotEqual => version.precedence(&self.version).is_ne(),
            Operator::GreaterEq => self.compare_start(version) != Ordering::Less,
            Operator::Less => self.compare_start(version) == Ordering::Less,
            Operator::Greater => self.compare_line(version) == Ordering::Greater,
            Operator::LessEq => self.compare_line(version) != Ordering::Greater,
        }
    }

    /// Compare `version` with the start of this comparator's line.
    ///
    /// Prereleases count as their release, because the line starts with them.
    fn compare_start(&self, version: &Version) -> Ordering {
        compare_release(&version.release, &self.version.release)
    }

    /// Compare `version` with this comparator's line as a whole.
    ///
    /// Only as many release components count as the comparator's version spells.
    fn compare_line(&self, version: &Version) -> Ordering {
        let precision = self.version.release.len();
        let prefix: Vec<u64> = (0..precision)
            .map(|idx| version.release.get(idx).copied().unwrap_or(0))
            .collect();
        compare_release(&prefix, &self.version.release)
    }
}

impl FromStr for Comparator {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ParseError::new(input, "comparator must not be empty"));
        }
        parse_comparator(input, input)
    }
}

impl fmt::Display for Comparator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.op, self.version)?;
        if self.wildcard {
            f.write_str(".*")?;
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl Serialize for Comparator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Comparator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        input.parse().map_err(serde::de::Error::custom)
    }
}

/// Version comparator operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterEq,
    /// `<`
    Less,
    /// `<=`
    LessEq,
}

impl FromStr for Operator {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "==" | "=" => Ok(Self::Equal),
            "!=" => Ok(Self::NotEqual),
            ">" => Ok(Self::Greater),
            ">=" => Ok(Self::GreaterEq),
            "<" => Ok(Self::Less),
            "<=" => Ok(Self::LessEq),
            _ => Err(ParseError::new(
                input,
                "unknown version comparator operator",
            )),
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Greater => ">",
            Self::GreaterEq => ">=",
            Self::Less => "<",
            Self::LessEq => "<=",
        })
    }
}

#[cfg(feature = "serde")]
impl Serialize for Operator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Operator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        input.parse().map_err(serde::de::Error::custom)
    }
}

/// Error returned when parsing a [`Version`] or [`VersionReq`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ParseError {
    input: String,
    message: String,
}

impl ParseError {
    fn new(input: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            message: message.into(),
        }
    }

    /// Input that failed to parse.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Human-readable parse error.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error parsing {:?}: {}", self.input, self.message)
    }
}

impl std::error::Error for ParseError {}

/// A prerelease identifier.
#[derive(Debug, Clone)]
enum PrereleaseIdentifier {
    Numeric { value: u64, display: String },
    Alpha(String),
}

impl PrereleaseIdentifier {
    fn parse(input: &str, version: &str) -> Result<Self, ParseError> {
        if input.bytes().all(|b| b.is_ascii_digit()) {
            Ok(Self::Numeric {
                value: input.parse::<u64>().map_err(|_| {
                    ParseError::new(version, "prerelease number does not fit into u64")
                })?,
                display: input.to_owned(),
            })
        } else {
            Ok(Self::Alpha(input.to_owned()))
        }
    }
}

impl PartialEq for PrereleaseIdentifier {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Numeric { value: left, .. }, Self::Numeric { value: right, .. }) => {
                left == right
            }
            (Self::Alpha(left), Self::Alpha(right)) => left == right,
            _ => false,
        }
    }
}

impl Eq for PrereleaseIdentifier {}

impl Hash for PrereleaseIdentifier {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Numeric { value, .. } => {
                0_u8.hash(state);
                value.hash(state);
            }
            Self::Alpha(value) => {
                1_u8.hash(state);
                value.hash(state);
            }
        }
    }
}

impl Ord for PrereleaseIdentifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Numeric { value: left, .. }, Self::Numeric { value: right, .. }) => {
                left.cmp(right)
            }
            (Self::Numeric { .. }, Self::Alpha(_)) => Ordering::Less,
            (Self::Alpha(_), Self::Numeric { .. }) => Ordering::Greater,
            (Self::Alpha(left), Self::Alpha(right)) => left.cmp(right),
        }
    }
}

impl PartialOrd for PrereleaseIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for PrereleaseIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Numeric { display, .. } => f.write_str(display),
            Self::Alpha(value) => f.write_str(value),
        }
    }
}

fn parse_comparator(input: &str, part: &str) -> Result<Comparator, ParseError> {
    const OPERATORS: &[(&str, Operator)] = &[
        (">=", Operator::GreaterEq),
        ("<=", Operator::LessEq),
        ("==", Operator::Equal),
        ("!=", Operator::NotEqual),
        (">", Operator::Greater),
        ("<", Operator::Less),
        ("=", Operator::Equal),
    ];

    let (op, version) = OPERATORS
        .iter()
        .find_map(|(prefix, op)| {
            part.strip_prefix(prefix)
                .map(|version| (*op, version.trim()))
        })
        .unwrap_or((Operator::Equal, part));
    if version.is_empty() {
        return Err(ParseError::new(input, "comparator is missing a version"));
    }
    let Some(version) = version.strip_suffix(".*") else {
        return Ok(Comparator {
            op,
            version: version.parse()?,
            wildcard: false,
        });
    };
    if !matches!(op, Operator::Equal | Operator::NotEqual) {
        return Err(ParseError::new(
            input,
            "wildcards are only allowed with `==` and `!=`",
        ));
    }
    let version: Version = version.parse()?;
    if version.is_prerelease() || !version.build_metadata.is_empty() {
        return Err(ParseError::new(
            input,
            "wildcard versions must not have a prerelease or build metadata",
        ));
    }
    Ok(Comparator {
        op,
        version,
        wildcard: true,
    })
}

fn compare_release(left: &[u64], right: &[u64]) -> Ordering {
    let max_len = left.len().max(right.len());
    for idx in 0..max_len {
        let left = left.get(idx).copied().unwrap_or(0);
        let right = right.get(idx).copied().unwrap_or(0);
        match left.cmp(&right) {
            Ordering::Equal => {}
            non_equal => return non_equal,
        }
    }
    Ordering::Equal
}

fn parse_build_metadata(input: &str, build_metadata_part: &str) -> Result<Vec<String>, ParseError> {
    if build_metadata_part.is_empty() {
        return Ok(Vec::new());
    }

    let mut identifiers = Vec::new();
    for identifier in build_metadata_part.split('.') {
        if identifier.is_empty() {
            return Err(ParseError::new(
                input,
                "build metadata identifiers must not be empty",
            ));
        }
        if !identifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(ParseError::new(
                input,
                "build metadata identifiers must be ASCII alphanumeric or hyphenated",
            ));
        }
        identifiers.push(identifier.to_owned());
    }
    Ok(identifiers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_release_zeroes_do_not_affect_precedence() {
        assert!(version("5").precedence(&version("5.0")).is_eq());
        assert!(version("5.0").precedence(&version("5.0.0")).is_eq());
        assert!(version("05.002.000").precedence(&version("5.2")).is_eq());
        assert!(req("==5").matches(&version("5.0.0")));
        assert_eq!(version("5.0.0").to_string(), "5.0.0");
        assert_eq!(version("2026.05.12").to_string(), "2026.05.12");
    }

    #[test]
    fn orders_numeric_release_components() {
        assert!(version("5.10") > version("5.2"));
        assert!(version("5.2.1") > version("5.2"));
        assert!(version("6") > version("5.99.99"));
    }

    #[test]
    fn orders_prerelease_before_final_release() {
        assert!(version("1.0.0-rc.1") < version("1.0.0"));
        assert!(version("1.0.0-alpha") < version("1.0.0-beta"));
        assert!(version("1.0.0-alpha.2") < version("1.0.0-alpha.10"));
    }

    #[test]
    fn follows_semver_prerelease_separators_and_ordering() {
        assert!(Version::parse("1.0--").is_ok());
        assert!(version("1.0-dev.0.rc.5") < version("1.0-dev.0-rc.5"));
        assert!(version("1.0-RC") < version("1.0-rc"));
        assert_ne!(version("1.0-RC"), version("1.0-rc"));
    }

    #[test]
    fn numeric_prerelease_leading_zeroes_do_not_affect_precedence() {
        assert!(version("1.0-01").precedence(&version("1.0-1")).is_eq());
        assert!(
            version("1.0-alpha.01")
                .precedence(&version("1.0-alpha.1"))
                .is_eq()
        );
        assert!(req("==1.0-1").matches(&version("1.0-01")));
        assert_eq!(version("1.0-01").to_string(), "1.0-01");
        assert_eq!(version("1.0-alpha.01").to_string(), "1.0-alpha.01");
    }

    #[test]
    fn supports_build_metadata_ignored_for_compatibility() {
        let build = version("1.0+build.7");
        assert!(build.precedence(&version("1.0")).is_eq());
        assert!(build.precedence(&version("1.0+build.8")).is_eq());
        assert_eq!(build.to_string(), "1.0+build.7");
        assert_eq!(
            build
                .build_metadata()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["build", "7"]
        );

        assert!(version("1.0-rc.1+build.5") < version("1.0+build.5"));
        assert!(req("==1.0+build.7").matches(&version("1.0+build.8")));
        assert!(req("==1.0").matches(&version("1.0+build.7")));
    }

    #[test]
    fn equality_and_hashing_use_spelling() {
        use std::collections::{BTreeSet, HashSet};

        assert_eq!(version("5.0"), version(" 5.0 "));
        assert_ne!(version("5"), version("5.0"));
        assert_ne!(version("05"), version("5"));
        assert_ne!(version("1.0+build.7"), version("1.0+build.8"));

        let spellings = ["1.0+build.7", "1.0+build.8", "1.0", "1", "1.0+build.7"];
        let hash_set: HashSet<_> = spellings.iter().map(|v| version(v)).collect();
        assert_eq!(hash_set.len(), 4);
        let tree_set: BTreeSet<_> = spellings.iter().map(|v| version(v)).collect();
        assert_eq!(tree_set.len(), 4);
    }

    #[test]
    fn ordering_breaks_precedence_ties_by_spelling() {
        let mut versions: Vec<_> = ["1.0", "0.9", "1", "1.0-rc.1", "1.0+build.7"]
            .iter()
            .map(|v| version(v))
            .collect();
        versions.sort();
        let sorted: Vec<_> = versions.iter().map(ToString::to_string).collect();
        assert_eq!(sorted, ["0.9", "1.0-rc.1", "1", "1.0", "1.0+build.7"]);
    }

    #[test]
    fn supports_calendar_versions() {
        assert!(version("2024.6.25") > version("2024.6.1"));
        assert!(version("2024.10") > version("2024.9"));
        assert!(version("24.10") > version("24.4"));

        let requirement = req(">=2024.6,<2025");
        assert!(requirement.matches(&version("2024.6.25")));
        assert!(requirement.matches(&version("2024.12")));
        assert!(!requirement.matches(&version("2023.12.31")));
        assert!(!requirement.matches(&version("2025.1")));
    }

    #[test]
    fn supports_calendar_versions_with_prereleases() {
        assert!(version("2024.6.25-rc.1") < version("2024.6.25"));
        assert!(version("2024.6.25-beta") < version("2024.6.25-rc.1"));
        assert!(version("24.4-preview.2") < version("24.4"));
    }

    #[test]
    fn supports_variable_length_numeric_schemes() {
        assert!(version("42") < version("43"));
        assert!(version("2024.6.25.2") > version("2024.6.25.1"));
        assert!(version("2024.6.25.1") > version("2024.6.25"));

        let requirement = req(">=2024.6.25,<2024.7");
        assert!(requirement.matches(&version("2024.6.25.3")));
        assert!(!requirement.matches(&version("2024.7")));
    }

    #[test]
    fn supports_compact_timestamp_versions() {
        assert!(version("20260625071935") > version("20260625071934"));
        assert!(version("20260625080000") > version("20260625071935"));

        let requirement = req(">=20260625000000,<20260626000000");
        assert!(requirement.matches(&version("20260625071935")));
        assert!(!requirement.matches(&version("20260626000000")));
    }

    #[test]
    fn parses_and_evaluates_requirements() {
        let requirement = req(">=5,<6");
        assert!(requirement.matches(&version("5")));
        assert!(requirement.matches(&version("5.2.3")));
        assert!(!requirement.matches(&version("4.9.9")));
        assert!(!requirement.matches(&version("6")));
    }

    #[test]
    fn bare_requirement_is_exact_match() {
        let requirement = req("5.0");
        assert!(requirement.matches(&version("5")));
        assert!(!requirement.matches(&version("5.0.1")));
    }

    #[test]
    fn any_requirement_matches_everything() {
        let requirement = req("*");
        assert!(requirement.is_any());
        assert!(requirement.matches(&version("0.1")));
        assert!(requirement.matches(&version("999.999")));
    }

    #[test]
    fn range_matches_whole_line_including_prereleases() {
        let requirement = req(">=3,<4");
        assert!(requirement.matches(&version("3.0-rc.1")));
        assert!(requirement.matches(&version("3.5-beta")));
        assert!(requirement.matches(&version("3.9")));
        assert!(!requirement.matches(&version("4.0-rc.1")));
        assert!(!requirement.matches(&version("4-0")));
        assert!(!requirement.matches(&version("2.9")));
    }

    #[test]
    fn lower_and_exclusive_upper_bounds_ignore_precision() {
        for bound in ["3", "3.0", "3.0.0"] {
            assert!(req(&format!(">={bound}")).matches(&version("3-rc.1")));
            assert!(req(&format!(">={bound}")).matches(&version("3.0.0-rc.1")));
            assert!(!req(&format!("<{bound}")).matches(&version("3.0-rc.1")));
            assert!(req(&format!("<{bound}")).matches(&version("2.9")));
        }
    }

    #[test]
    fn greater_bound_starts_after_its_line() {
        assert!(!req(">3").matches(&version("3.1")));
        assert!(!req(">3").matches(&version("3")));
        assert!(req(">3").matches(&version("4.0-rc.1")));
        assert!(req(">3.0").matches(&version("3.1")));
        assert!(!req(">3.0").matches(&version("3.0.5")));
        assert!(req(">3.0.0").matches(&version("3.0.5")));
        assert!(!req(">3.0").matches(&version("3")));
    }

    #[test]
    fn inclusive_upper_bound_ends_with_its_line() {
        assert!(req("<=3").matches(&version("3.9")));
        assert!(req("<=3").matches(&version("3.0-rc.1")));
        assert!(!req("<=3").matches(&version("4.0-rc.1")));
        assert!(req("<=3.0").matches(&version("3.0.5")));
        assert!(!req("<=3.0").matches(&version("3.1")));
    }

    #[test]
    fn exact_comparators_ignore_lines() {
        assert!(req("==3").matches(&version("3.0")));
        assert!(!req("==3").matches(&version("3.0-rc.1")));
        assert!(!req("==3").matches(&version("3.0.1")));
        assert!(req("!=3").matches(&version("3.0.1")));
        assert!(!req("!=3").matches(&version("3.0.0")));
    }

    #[test]
    fn prerelease_bounds_keep_total_order() {
        assert!(req("<4-rc.2").matches(&version("4-rc.1")));
        assert!(!req("<4-rc.2").matches(&version("4-rc.2")));
        assert!(req("<1-alpha.1").matches(&version("1-alpha")));
        assert!(!req(">=3-rc.2").matches(&version("3-rc.1")));
        assert!(req(">=3-rc.2").matches(&version("3-rc.2")));
        assert!(req(">3-rc.1").matches(&version("3-rc.2")));
        assert!(!req("<=4-rc.1").matches(&version("4-rc.2")));
    }

    #[test]
    fn wildcard_matches_line() {
        let requirement = req("3.*");
        assert!(requirement.matches(&version("3.0-rc.1")));
        assert!(requirement.matches(&version("3")));
        assert!(requirement.matches(&version("3.5.1")));
        assert!(!requirement.matches(&version("4.0-rc.1")));
        assert!(!requirement.matches(&version("2.9")));
        assert!(req("==2024.6.*").matches(&version("2024.6.30")));
        assert!(!req("2024.6.*").matches(&version("2024.7.1")));
        assert!(req("3.0.*").matches(&version("3")));
        assert!(!req("3.0.*").matches(&version("3.1")));
        assert!(!req("!=3.*").matches(&version("3.2")));
        assert!(req("!=3.*").matches(&version("4.0")));
        assert!(req("3.*,>=3.2").matches(&version("3.2-rc.1")));
        assert!(!req("3.*,>=3.2").matches(&version("3.1")));
    }

    #[test]
    fn wildcard_round_trips_through_display() {
        assert_eq!(req("3.*").to_string(), "==3.*");
        assert_eq!(req("!=2024.6.*").to_string(), "!=2024.6.*");
        assert_eq!(req(">=3,<4").to_string(), ">=3,<4");
    }

    #[test]
    fn not_equal_requirement_excludes_matching_versions() {
        let requirement = req(">=5,!=5.1,<6");
        assert!(requirement.matches(&version("5.0")));
        assert!(!requirement.matches(&version("5.1.0")));
        assert!(requirement.matches(&version("5.2")));
    }

    #[test]
    fn rejects_invalid_versions() {
        for input in [
            "",
            ".",
            "1..2",
            "1.x",
            "1-",
            "1.0-alpha..1",
            "1.0-alpha_1",
            "1.0+",
            "1.0+build..1",
            "1.0+build+1",
        ] {
            assert!(Version::parse(input).is_err(), "{input:?} should fail");
        }
    }

    #[test]
    fn rejects_invalid_requirements() {
        for input in [
            "",
            ">=",
            ">=1,",
            ">=1,,<2",
            ">=3.*",
            "<3.*",
            "3.*-rc.1",
            "3-rc.1.*",
            "3.*+build",
            "3.*.*",
            ".*",
        ] {
            assert!(VersionReq::parse(input).is_err(), "{input:?} should fail");
        }
    }

    fn version(input: &str) -> Version {
        Version::parse(input).expect("input must be a valid version")
    }

    fn req(input: &str) -> VersionReq {
        VersionReq::parse(input).expect("input must be a valid version requirement")
    }
}
