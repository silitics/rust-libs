//! Version parsing and requirement matching for any versioning scheme.
//!
//! Software in the wild is versioned in many ways. Libraries use [Semantic
//! Versioning], operating system images often use [Calendar Versioning], firmware
//! counts builds, and CI pipelines stamp timestamps. A system that combines
//! components from many sources, such as an embedded Linux device running a system
//! image, apps, and firmware, has to compare all of these versions without knowing
//! in advance which scheme each component follows.
//!
//! `anyver` covers these schemes with one grammar and one order. It parses a version
//! without assuming a scheme, compares versions numerically, and matches them against
//! requirements such as `>=2.20,<3` or `2026.6.*`:
//!
//! ```
//! use anyver::{Version, VersionReq};
//!
//! # fn main() -> Result<(), anyver::ParseError> {
//! let compose: Version = "2.27.0".parse()?;
//! let requirement: VersionReq = ">=2.20,<3".parse()?;
//! assert!(requirement.matches(&compose));
//!
//! // Calendar versions and serial numbers use the same grammar.
//! let image: Version = "2026.06.25".parse()?;
//! assert!(VersionReq::parse("2026.6.*")?.matches(&image));
//! assert!(Version::parse("20260625071935")? > Version::parse("20260624235959")?);
//! # Ok(())
//! # }
//! ```
//!
//! [Semantic Versioning]: https://semver.org
//! [Calendar Versioning]: https://calver.org
//!
//! # Versions
//!
//! A [`Version`] consists of one or more numeric _release components_, an optional
//! _prerelease_, and optional _build metadata_:
//!
//! ```text
//! version    = release [ "-" prerelease ] [ "+" build ]
//! release    = number { "." number }
//! prerelease = identifier { "." identifier }
//! build      = identifier { "." identifier }
//! number     = digit { digit }
//! identifier = ( letter | digit | "-" ) { letter | digit | "-" }
//! ```
//!
//! Numbers, including numeric prerelease identifiers, must fit into a [`u64`].
//! Letters are ASCII letters. Leading zeros are allowed, and whitespace around a
//! version is ignored. The grammar covers common schemes as they are usually
//! written:
//!
//! | Scheme                  | Examples                                   |
//! | ----------------------- | ------------------------------------------ |
//! | Semantic Versioning     | `1.4.2`, `2.0.0-rc.1`, `1.4.2+build.7`     |
//! | Calendar Versioning     | `2026.6`, `2026.06.25`, `24.04`            |
//! | Serial numbers          | `42`, `1337`                               |
//! | Timestamps              | `20260625071935`                           |
//! | Dates with a revision   | `2026.6.25.2`                              |
//!
//! Versions in other formats must be converted first. For example, a leading `v`,
//! as in `v1.2`, is not part of the grammar. Debian versions, such as `1:2.27~rc1-3`,
//! use `~` for prereleases and `-` for package revisions, so they need translating
//! rather than parsing.
//!
//! ## Precedence
//!
//! Versions are ordered by _precedence_, which follows Semantic Versioning and
//! generalizes it to any number of release components:
//!
//! 1. Release components compare numerically from left to right. Missing components count
//!    as zero, so `5`, `5.0`, and `5.0.0` have the same precedence, and `5.10` follows
//!    `5.9`.
//! 2. With equal release components, a prerelease precedes the release, so `1.0-rc.1`
//!    precedes `1.0`.
//! 3. Two prereleases compare identifier by identifier. Numeric identifiers compare
//!    numerically and precede alphanumeric identifiers, which compare by ASCII order. If
//!    all identifiers of one prerelease start the other, the shorter one precedes.
//! 4. Leading zeros and build metadata do not affect precedence.
//!
//! ```
//! use anyver::Version;
//!
//! # fn main() -> Result<(), anyver::ParseError> {
//! let ordered = [
//!     "1.0-alpha", "1.0-alpha.1", "1.0-alpha.beta", "1.0-beta", "1.0-beta.2",
//!     "1.0-beta.11", "1.0-rc.1", "1.0", "1.0.1", "1.1", "1.10",
//! ];
//! for pair in ordered.windows(2) {
//!     assert!(Version::parse(pair[0])? < Version::parse(pair[1])?);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Identity
//!
//! A [`Version`] keeps the exact spelling it was parsed from, and [`Display`] returns
//! it unchanged. Equality and hashing compare this spelling, so `5`, `5.0`, and
//! `5+build.7` are different values, even though they have the same precedence.
//! [`Ord`] orders by precedence and breaks ties by spelling, which keeps it
//! consistent with equality. Sorting therefore puts versions in precedence order and
//! keeps every distinct spelling:
//!
//! ```
//! use std::collections::BTreeSet;
//!
//! use anyver::{Version, VersionReq};
//!
//! # fn main() -> Result<(), anyver::ParseError> {
//! let versions: BTreeSet<Version> = ["1.0", "0.9", "1", "1.0-rc.1"]
//!     .into_iter()
//!     .map(Version::parse)
//!     .collect::<Result<_, _>>()?;
//! let sorted: Vec<String> = versions.iter().map(ToString::to_string).collect();
//! assert_eq!(sorted, ["0.9", "1.0-rc.1", "1", "1.0"]);
//!
//! // Whether two versions have the same precedence is a question for a requirement.
//! assert_ne!(Version::parse("1")?, Version::parse("1.0")?);
//! assert!(VersionReq::parse("==1")?.matches(&Version::parse("1.0+build.7")?));
//! # Ok(())
//! # }
//! ```
//!
//! [`Display`]: std::fmt::Display
//!
//! # Requirements
//!
//! A [`VersionReq`] is a comma-separated list of [`Comparator`]s, all of which must
//! match, such as `>=2.20,<3`. A requirement consisting of `*` alone matches every
//! version. A comparator is an [`Operator`] followed by a version, where `=` is the
//! same as `==` and a version without an operator means `==`. Cargo and npm
//! shorthands such as `^` and `~` are not supported, because they presume a scheme.
//!
//! ## Lines
//!
//! Requirements are usually written before the versions they refer to exist. Suppose
//! an app works with version 3 of a runtime and declares `>=3,<4` while version 3 is
//! still in development. Testing the app with the release candidate `3.0-rc.1`
//! should not require changing the declaration. Conversely, `4.0-rc.1` already
//! contains the changes of version 4 and should not satisfy `<4`. Plain precedence
//! gets both cases wrong, because `3.0-rc.1` precedes `3` and `4.0-rc.1` precedes `4`.
//!
//! A comparator's version without a prerelease therefore names a _line_: all versions
//! whose release components, padded with zeros, start with the comparator's release
//! components, including their prereleases. The line `3` contains `3.0-rc.1`, `3`,
//! and `3.5.1`. The line `3.0` contains `3.0-rc.1` and `3.0.5`, but not `3.1`. Each
//! operator refers to the start or the end of its line:
//!
//! | Comparator | Meaning                          | Example Match       | Example Mismatch    |
//! | ---------- | -------------------------------- | ------------------- | ------------------- |
//! | `>=3`      | From the start of the line `3`   | `3.0-rc.1`, `3.9`   | `2.9`               |
//! | `<4`       | Before the start of the line `4` | `3.9`               | `4.0-rc.1`          |
//! | `>3`       | After the end of the line `3`    | `4.0-rc.1`          | `3.1`               |
//! | `<=3`      | Up to the end of the line `3`    | `3.9`               | `4.0-rc.1`          |
//! | `3.*`      | Within the line `3`              | `3.0-rc.1`, `3.5.1` | `4.0-rc.1`          |
//! | `!=3.*`    | Outside the line `3`             | `4.0`               | `3.2`               |
//! | `==3`      | Exactly the precedence of `3`    | `3.0`, `3+build.7`  | `3.0-rc.1`, `3.0.1` |
//! | `!=3`      | Any other precedence             | `3.0.1`             | `3.0`               |
//!
//! For `>` and `<=`, the number of release components matters: `>3` does not match
//! `3.1`, but `>3.0` does. For `>=` and `<`, it does not, because every line starts
//! with its first prerelease, so `>=3` and `>=3.0` are the same. `==` and `!=`
//! compare precedence, so a version without an operator pins exactly one version.
//! Wildcards, such as `3.*`, are only allowed with `=`, `==`, and `!=`, and their
//! version must not have a prerelease or build metadata.
//!
//! A comparator's version with a prerelease, such as `>=3-rc.2` or `<4-rc.2`,
//! compares by precedence instead. Use it to admit only later release candidates or
//! to let prereleases of the next line in:
//!
//! ```
//! use anyver::{Version, VersionReq};
//!
//! # fn main() -> Result<(), anyver::ParseError> {
//! let v = |input: &str| Version::parse(input);
//!
//! let line = VersionReq::parse(">=3,<4")?;
//! assert!(line.matches(&v("3.0-rc.1")?));
//! assert!(line.matches(&v("3.9.2")?));
//! assert!(!line.matches(&v("4.0-rc.1")?));
//!
//! // `>` and `<=` respect the precision of their version.
//! assert!(!VersionReq::parse(">3")?.matches(&v("3.1")?));
//! assert!(VersionReq::parse(">3.0")?.matches(&v("3.1")?));
//!
//! // Wildcards match a whole line and combine with other comparators.
//! let june = VersionReq::parse("2026.6.*")?;
//! assert!(june.matches(&v("2026.06.30")?));
//! assert!(!june.matches(&v("2026.7.1")?));
//! assert!(VersionReq::parse("3.*,>=3.2")?.matches(&v("3.2-rc.1")?));
//!
//! // Prerelease bounds compare by precedence.
//! assert!(!VersionReq::parse(">=3-rc.2")?.matches(&v("3-rc.1")?));
//! assert!(VersionReq::parse("<4-rc.2")?.matches(&v("4-rc.1")?));
//! # Ok(())
//! # }
//! ```
//!
//! # Errors
//!
//! Parsing fails with a [`ParseError`] that names the input and the problem:
//!
//! ```
//! use anyver::{Version, VersionReq};
//!
//! let error = Version::parse("1.x").unwrap_err();
//! assert_eq!(error.input(), "1.x");
//! assert_eq!(error.message(), "release components must be numeric");
//!
//! assert!(VersionReq::parse(">=3.*").is_err());
//! assert!(VersionReq::parse("^1.2").is_err());
//! ```
//!
//! # Serde
//!
//! With the `serde` feature, enabled by default, [`Version`], [`VersionReq`],
//! [`Comparator`], and [`Operator`] serialize as strings. Deserialization applies the
//! same grammar as parsing, so invalid versions are rejected when the surrounding
//! document is read:
//!
//! ```
//! # #[cfg(feature = "serde")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use anyver::{Version, VersionReq};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Dependency {
//!     id: String,
//!     version: VersionReq,
//! }
//!
//! let dependency: Dependency =
//!     serde_json::from_str(r#"{ "id": "docker.compose", "version": ">=2.20,<3" }"#)?;
//! assert_eq!(dependency.id, "docker.compose");
//! assert!(dependency.version.matches(&Version::parse("2.27.0")?));
//!
//! assert!(serde_json::from_str::<Version>(r#""1..2""#).is_err());
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "serde"))]
//! # fn main() {}
//! ```

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

/// A version, such as `1.4.2`, `2026.06.25`, or `2.0.0-rc.1+build.7`.
///
/// The [crate documentation](crate#versions) defines the grammar, the precedence
/// rules, and how equality differs from precedence.
///
/// # Examples
///
/// ```
/// use anyver::Version;
///
/// # fn main() -> Result<(), anyver::ParseError> {
/// let version = Version::parse("2.0.0-rc.1+build.7")?;
/// assert_eq!(version.release(), [2, 0, 0]);
/// assert!(version.is_prerelease());
/// assert_eq!(version.build_metadata(), ["build", "7"]);
/// assert_eq!(version.to_string(), "2.0.0-rc.1+build.7");
///
/// assert!(version < Version::parse("2.0.0")?);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Version {
    display: String,
    release: Vec<u64>,
    prerelease: Vec<PrereleaseIdentifier>,
    build_metadata: Vec<String>,
}

impl Version {
    /// Parse a version, ignoring surrounding whitespace.
    ///
    /// This is the same as [`str::parse`].
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if `input` does not follow the
    /// [version grammar](crate#versions).
    ///
    /// # Examples
    ///
    /// ```
    /// use anyver::Version;
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// assert_eq!(Version::parse(" 5.2 ")?.to_string(), "5.2");
    /// assert!(Version::parse("v5.2").is_err());
    /// assert!(Version::parse("5.2-").is_err());
    /// # Ok(())
    /// # }
    /// ```
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        input.parse()
    }

    /// Release components as numbers, including trailing zeros.
    ///
    /// ```
    /// use anyver::Version;
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// assert_eq!(Version::parse("05.2.0")?.release(), [5, 2, 0]);
    /// assert_eq!(Version::parse("20260625071935")?.release(), [20_260_625_071_935]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn release(&self) -> &[u64] {
        &self.release
    }

    /// Whether this version has a prerelease, such as `rc.1` in `2.0.0-rc.1`.
    ///
    /// ```
    /// use anyver::Version;
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// assert!(Version::parse("2.0.0-rc.1")?.is_prerelease());
    /// assert!(!Version::parse("2.0.0+build.7")?.is_prerelease());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }

    /// Build metadata identifiers, such as `["build", "7"]` for `1.0+build.7`.
    ///
    /// Build metadata does not affect precedence or requirement matching.
    ///
    /// ```
    /// use anyver::Version;
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// assert_eq!(Version::parse("1.0+build.7")?.build_metadata(), ["build", "7"]);
    /// assert!(Version::parse("1.0")?.build_metadata().is_empty());
    /// # Ok(())
    /// # }
    /// ```
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

/// A version requirement, such as `>=2.20,<3`, `3.*`, or `*`.
///
/// A requirement matches a version if all of its [`Comparator`]s match it. The
/// [crate documentation](crate#requirements) defines the syntax and how each
/// comparator matches.
///
/// [`Display`](std::fmt::Display) writes a requirement in a normalized form: without
/// whitespace, with `==` for comparators without an operator, and with `*` for a
/// requirement matching every version.
///
/// # Examples
///
/// ```
/// use anyver::{Version, VersionReq};
///
/// # fn main() -> Result<(), anyver::ParseError> {
/// let requirement = VersionReq::parse(" >= 2.20 , < 3 ")?;
/// assert!(requirement.matches(&Version::parse("2.27.0")?));
/// assert!(!requirement.matches(&Version::parse("3.0-rc.1")?));
/// assert_eq!(requirement.to_string(), ">=2.20,<3");
///
/// assert_eq!(VersionReq::parse("5.2")?.to_string(), "==5.2");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionReq {
    comparators: Vec<Comparator>,
}

impl VersionReq {
    /// A requirement matching every version, written as `*`.
    ///
    /// ```
    /// use anyver::{Version, VersionReq};
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// assert_eq!(VersionReq::parse("*")?, VersionReq::any());
    /// assert!(VersionReq::any().matches(&Version::parse("0.1-alpha")?));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn any() -> Self {
        Self::default()
    }

    /// Parse a version requirement, ignoring whitespace around comparators.
    ///
    /// This is the same as [`str::parse`].
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if `input` is neither `*` nor a comma-separated list
    /// of [comparators](crate#requirements).
    ///
    /// # Examples
    ///
    /// ```
    /// use anyver::VersionReq;
    ///
    /// assert!(VersionReq::parse(">=3,<4").is_ok());
    /// assert!(VersionReq::parse(">=3,").is_err());
    /// assert!(VersionReq::parse("^3").is_err());
    /// assert!(VersionReq::parse("*,<4").is_err());
    /// ```
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        input.parse()
    }

    /// The comparators of this requirement, in the order they were written.
    ///
    /// ```
    /// use anyver::{Operator, VersionReq};
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// let requirement = VersionReq::parse(">=3,<4")?;
    /// let operators: Vec<Operator> = requirement.comparators().iter().map(|c| c.op).collect();
    /// assert_eq!(operators, [Operator::GreaterEq, Operator::Less]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn comparators(&self) -> &[Comparator] {
        &self.comparators
    }

    /// Whether this requirement matches every version, which is the case for `*`.
    #[must_use]
    pub fn is_any(&self) -> bool {
        self.comparators.is_empty()
    }

    /// Whether all comparators of this requirement match `version`.
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

/// One comparison of a [`VersionReq`], such as `>=3`, `<4-rc.2`, or `!=3.*`.
///
/// The [crate documentation](crate#lines) defines how comparators match versions.
/// The fields are public, so comparators can also be built directly.
///
/// # Examples
///
/// ```
/// use anyver::{Comparator, Operator, Version};
///
/// # fn main() -> Result<(), anyver::ParseError> {
/// let at_least_3 = Comparator {
///     op: Operator::GreaterEq,
///     version: Version::parse("3")?,
///     wildcard: false,
/// };
/// assert!(at_least_3.matches(&Version::parse("3.0-rc.1")?));
/// assert_eq!(at_least_3.to_string(), ">=3");
///
/// let not_line_3: Comparator = "!=3.*".parse()?;
/// assert!(not_line_3.wildcard);
/// assert!(!not_line_3.matches(&Version::parse("3.2")?));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparator {
    /// The operator, such as `>=`.
    pub op: Operator,
    /// The version to compare against.
    pub version: Version,
    /// Whether the comparator refers to the whole line of its version, as in `3.*`.
    ///
    /// The parser only accepts wildcards with [`Operator::Equal`] and
    /// [`Operator::NotEqual`] and a version without a prerelease or build metadata.
    /// A directly built wildcard comparator with another operator matches like
    /// [`Operator::Equal`].
    pub wildcard: bool,
}

impl Comparator {
    /// Whether this comparator matches `version`, following the
    /// [line rules](crate#lines).
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

/// The operator of a [`Comparator`].
///
/// Each variant describes how it matches a comparator version without a prerelease.
/// With a prerelease version, all operators compare by precedence. The
/// [crate documentation](crate#lines) explains lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    /// `==` or `=`: the same precedence, or, with a wildcard, within the line.
    Equal,
    /// `!=`: a different precedence, or, with a wildcard, outside the line.
    NotEqual,
    /// `>`: after the end of the line, so `>3` does not match `3.1`.
    Greater,
    /// `>=`: from the start of the line, so `>=3` matches `3.0-rc.1`.
    GreaterEq,
    /// `<`: before the start of the line, so `<4` does not match `4.0-rc.1`.
    Less,
    /// `<=`: up to the end of the line, so `<=3` matches `3.9`.
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

/// An error parsing a [`Version`], [`VersionReq`], [`Comparator`], or [`Operator`].
///
/// [`Display`](std::fmt::Display) combines the input and the problem, as in
/// `error parsing "1.x": release components must be numeric`.
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

    /// The input that failed to parse, without surrounding whitespace.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// What is wrong with the input, such as `release components must be numeric`.
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
