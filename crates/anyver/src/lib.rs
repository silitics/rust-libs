//! Version parsing and requirement matching across versioning schemes.
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
//! Release components and numeric prerelease identifiers must fit into a [`u64`].
//! Build metadata identifiers are kept as text, so they have no numeric limit.
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
//! as in `v1.2`, is not part of the grammar. Debian versions use `~` for prereleases
//! and `-` for package revisions. To convert `1:2.27~rc1-3`, for example, replace `~`
//! with `-`, keep the package revision as build metadata, and drop the epoch `1:`,
//! which gives `2.27-rc1+3`. Dropping the epoch is only safe if all compared versions
//! share it.
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
//!    numerically and precede alphanumeric identifiers, which compare lexicographically
//!    by ASCII code. If one prerelease is a prefix of the other, the shorter one
//!    precedes, so `1.0-alpha` precedes `1.0-alpha.1`.
//! 4. Leading zeros of release components and numeric prerelease identifiers do not
//!    affect precedence, so `05.1-rc.01` and `5.1-rc.1` have the same precedence. Build
//!    metadata does not affect precedence either.
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
//! A [`Version`] keeps the exact spelling it was parsed from, without surrounding
//! whitespace, and [`Display`] returns it unchanged. Equality and hashing compare
//! this spelling, so `5`, `5.0`, and `5+build.7` are different values, even though
//! they have the same precedence. [`Version::cmp_precedence`] compares by precedence
//! alone. [`Ord`] orders by precedence and breaks ties by spelling, which keeps it
//! consistent with equality. Sorting therefore puts versions in precedence order and
//! keeps every distinct spelling:
//!
//! ```
//! use std::collections::BTreeSet;
//!
//! use anyver::Version;
//!
//! # fn main() -> Result<(), anyver::ParseError> {
//! let versions: BTreeSet<Version> = ["1.0", "0.9", "1", "1.0-rc.1"]
//!     .into_iter()
//!     .map(Version::parse)
//!     .collect::<Result<_, _>>()?;
//! let sorted: Vec<String> = versions.iter().map(ToString::to_string).collect();
//! assert_eq!(sorted, ["0.9", "1.0-rc.1", "1", "1.0"]);
//!
//! let (one, one_zero) = (Version::parse("1")?, Version::parse("1.0")?);
//! assert_ne!(one, one_zero);
//! assert!(one.cmp_precedence(&one_zero).is_eq());
//! # Ok(())
//! # }
//! ```
//!
//! [`Display`]: std::fmt::Display
//!
//! # Requirements
//!
//! A [`VersionReq`] is a comma-separated list of [`Comparator`]s that must all
//! match, such as `>=2.20,<3`. Each comparator pairs an [`Operator`] with a version,
//! as in `>=2.20`. The operators are `==`, `!=`, `>`, `>=`, `<`, and `<=`, and `=` is
//! an alias for `==`. A version without an operator, such as `2.20`, means `==2.20`.
//! The requirement `*` matches every version. Cargo and npm shorthands such as `^`
//! and `~` are not supported, because they presume a scheme. Matching ignores how
//! the matched version is spelled, so `==1` matches `1.0` and `1.0+build.7`.
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
//! and `3.5.1`. The line `3.0` contains `3.0-rc.1` and `3.0.5`, but not `3.1`. The
//! ordering operators refer to the start or the end of a line, wildcards to the line
//! as a whole, and `==` and `!=` to precedence:
//!
//! | Comparator | Meaning                          | Example Match       | Example Mismatch    |
//! | ---------- | -------------------------------- | ------------------- | ------------------- |
//! | `>=3`      | From the start of the line `3`   | `3.0-rc.1`, `3.9`   | `2.9`               |
//! | `<4`       | Before the start of the line `4` | `3.9`               | `4.0-rc.1`          |
//! | `>3`       | After the end of the line `3`    | `4.0-rc.1`          | `3.1`               |
//! | `<=3`      | Up to the end of the line `3`    | `3.9`               | `4.0-rc.1`          |
//! | `3.*`      | Within the line `3`              | `3.0-rc.1`, `3.5.1` | `4.0-rc.1`          |
//! | `!=3.*`    | Outside the line `3`             | `4.0`               | `3.2`               |
//! | `==3`      | The same precedence as `3`       | `3.0`, `3+build.7`  | `3.0-rc.1`, `3.0.1` |
//! | `!=3`      | Any other precedence             | `3.0.1`             | `3.0`               |
//!
//! For `>` and `<=`, appending zero release components narrows the line: `>3` does
//! not match `3.1`, but `>3.0` does. For `>=` and `<`, appending zero release
//! components changes nothing, because it does not move where the line starts, so
//! `>=3` and `>=3.0` match the same versions.
//!
//! `==` and `!=` ignore lines and compare precedence, so `==3`, or just `3`, pins
//! version 3 however it is spelled. Wildcards, such as `3.*`, are only allowed with
//! `=`, `==`, and `!=`, and their version must not have a prerelease or build
//! metadata.
//!
//! A comparator whose version has a prerelease compares by precedence. For
//! example, `>=3-rc.2` matches `3-rc.2`, `3`, and every later version, but not
//! `3-rc.1`. `<4-rc.2` matches `4-rc.1`, but not `4-rc.2` or `4`, so it admits early
//! prereleases of the next line:
//!
//! ```
//! use anyver::{Version, VersionReq};
//!
//! # fn main() -> Result<(), anyver::ParseError> {
//! let v = Version::parse;
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
//! let from_rc_2 = VersionReq::parse(">=3-rc.2")?;
//! assert!(from_rc_2.matches(&v("3-rc.2")?) && from_rc_2.matches(&v("3")?));
//! assert!(!from_rc_2.matches(&v("3-rc.1")?));
//! let before_4_rc_2 = VersionReq::parse("<4-rc.2")?;
//! assert!(before_4_rc_2.matches(&v("4-rc.1")?));
//! assert!(!before_4_rc_2.matches(&v("4")?));
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
//! With the `serde` feature, [`Version`], [`VersionReq`], [`Comparator`], and
//! [`Operator`] serialize as strings. Deserialization applies the same grammar as
//! parsing, so invalid versions are rejected when the surrounding document is read:
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

#![cfg_attr(docsrs, feature(doc_cfg))]

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::{fmt, iter};

#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
/// assert_eq!(version.prerelease(), Some("rc.1"));
/// assert_eq!(version.build_metadata(), Some("build.7"));
/// assert_eq!(version.to_string(), "2.0.0-rc.1+build.7");
///
/// assert!(version < Version::parse("2.0.0")?);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Version {
    /// The version as spelled, without surrounding whitespace.
    spelling: String,
    /// The release components, including trailing zeros.
    release: Vec<u64>,
    /// The prerelease identifiers, empty for a release.
    prerelease: Vec<PrereleaseIdentifier>,
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

    /// The prerelease as spelled, such as `rc.1` for `2.0.0-rc.1+build.7`.
    ///
    /// ```
    /// use anyver::Version;
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// assert_eq!(Version::parse("2.0.0-rc.1+build.7")?.prerelease(), Some("rc.1"));
    /// assert_eq!(Version::parse("2.0.0-beta-2")?.prerelease(), Some("beta-2"));
    /// assert_eq!(Version::parse("2.0.0")?.prerelease(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn prerelease(&self) -> Option<&str> {
        split_version(&self.spelling).1
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

    /// The build metadata as spelled, such as `build.7` for `1.0+build.7`.
    ///
    /// Build metadata does not affect precedence or requirement matching.
    ///
    /// ```
    /// use anyver::Version;
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// assert_eq!(Version::parse("1.0+build.7")?.build_metadata(), Some("build.7"));
    /// assert_eq!(Version::parse("1.0")?.build_metadata(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn build_metadata(&self) -> Option<&str> {
        split_version(&self.spelling).2
    }

    /// Compare two versions by [precedence](crate#precedence), ignoring their
    /// spelling and build metadata.
    ///
    /// Versions with the same precedence, such as `5`, `5.0`, and `5+build.7`, compare
    /// as [`Ordering::Equal`]. [`Ord`] orders such versions by spelling.
    ///
    /// ```
    /// use std::cmp::Ordering;
    ///
    /// use anyver::Version;
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// let five = Version::parse("5")?;
    /// assert_eq!(five.cmp_precedence(&Version::parse("5.0+build.7")?), Ordering::Equal);
    /// assert_eq!(five.cmp_precedence(&Version::parse("5.1-rc.1")?), Ordering::Less);
    /// assert_eq!(five.cmp(&Version::parse("5.0")?), Ordering::Less);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cmp_precedence(&self, other: &Self) -> Ordering {
        compare_release(&self.release, &other.release).then_with(|| {
            match (self.is_prerelease(), other.is_prerelease()) {
                (false, false) => Ordering::Equal,
                (false, true) => Ordering::Greater,
                (true, false) => Ordering::Less,
                (true, true) => self.prerelease.cmp(&other.prerelease),
            }
        })
    }
}

impl FromStr for Version {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ParseError::new(input, "version must not be empty"));
        }
        let (release, prerelease, build_metadata) = split_version(input);
        if release.is_empty() {
            return Err(ParseError::new(input, "release must not be empty"));
        }
        let release = release
            .split('.')
            .map(|component| parse_release_component(input, component))
            .collect::<Result<_, _>>()?;
        let prerelease = match prerelease {
            Some(prerelease) => {
                validate_identifiers(input, prerelease, Part::Prerelease)?;
                prerelease
                    .split('.')
                    .map(|identifier| PrereleaseIdentifier::parse(input, identifier))
                    .collect::<Result<_, _>>()?
            }
            None => Vec::new(),
        };
        if let Some(build_metadata) = build_metadata {
            validate_identifiers(input, build_metadata, Part::BuildMetadata)?;
        }
        Ok(Self {
            spelling: input.to_owned(),
            release,
            prerelease,
        })
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.spelling == other.spelling
    }
}

impl Eq for Version {}

impl Hash for Version {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.spelling.hash(state);
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_precedence(other)
            .then_with(|| self.spelling.cmp(&other.spelling))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.spelling)
    }
}

#[cfg(feature = "serde")]
impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.spelling)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_from_str(deserializer)
    }
}

/// A version requirement, such as `>=2.20,<3`, `3.*`, or `*`.
///
/// A requirement matches a version if all of its [`Comparator`]s match it. The
/// [crate documentation](crate#requirements) defines the syntax and how each
/// comparator matches.
///
/// [`Display`](std::fmt::Display) writes a requirement in a normalized form: without
/// whitespace, with `==` for `=` and for comparators without an operator, and as `*`
/// for [`VersionReq::any`]. It keeps the order of the comparators and the spelling of
/// their versions.
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
    /// use anyver::{Comparator, Operator, VersionReq};
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// let requirement = VersionReq::parse(">=3,<4")?;
    /// let operators: Vec<Operator> = requirement.comparators().iter().map(Comparator::op).collect();
    /// assert_eq!(operators, [Operator::GreaterEq, Operator::Less]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn comparators(&self) -> &[Comparator] {
        &self.comparators
    }

    /// Whether this is the requirement `*`, which has no comparators.
    ///
    /// A requirement such as `>=0` also matches every version, but is not `*`.
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

/// Collects comparators into a requirement that matches if all of them match.
///
/// Collecting no comparators gives [`VersionReq::any`].
///
/// ```
/// use anyver::{Comparator, Operator, Version, VersionReq};
///
/// # fn main() -> Result<(), anyver::ParseError> {
/// let requirement: VersionReq = [
///     Comparator::new(Operator::GreaterEq, Version::parse("2.20")?),
///     Comparator::new(Operator::Less, Version::parse("3")?),
/// ]
/// .into_iter()
/// .collect();
/// assert_eq!(requirement.to_string(), ">=2.20,<3");
/// # Ok(())
/// # }
/// ```
impl FromIterator<Comparator> for VersionReq {
    fn from_iter<I: IntoIterator<Item = Comparator>>(iter: I) -> Self {
        Self {
            comparators: iter.into_iter().collect(),
        }
    }
}

/// A requirement with a single comparator.
impl From<Comparator> for VersionReq {
    fn from(comparator: Comparator) -> Self {
        Self {
            comparators: vec![comparator],
        }
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
            return Err(ParseError::new(input, "requirement must not be empty"));
        }
        input
            .split(',')
            .map(|comparator| {
                let comparator = comparator.trim();
                if comparator.is_empty() {
                    return Err(ParseError::new(input, "comparators must not be empty"));
                }
                comparator.parse()
            })
            .collect()
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
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for VersionReq {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_from_str(deserializer)
    }
}

/// One comparison of a [`VersionReq`], such as `>=3`, `<4-rc.2`, or `!=3.*`.
///
/// The [crate documentation](crate#lines) defines how comparators match versions.
/// Comparators come from parsing or from [`Comparator::new`] and
/// [`Comparator::wildcard`], which accept the same comparators as the parser.
///
/// # Examples
///
/// ```
/// use anyver::{Comparator, Operator, Version};
///
/// # fn main() -> Result<(), anyver::ParseError> {
/// let at_least_3 = Comparator::new(Operator::GreaterEq, Version::parse("3")?);
/// assert!(at_least_3.matches(&Version::parse("3.0-rc.1")?));
/// assert_eq!(at_least_3.to_string(), ">=3");
///
/// let not_line_3: Comparator = "!=3.*".parse()?;
/// assert_eq!(not_line_3.op(), Operator::NotEqual);
/// assert!(not_line_3.is_wildcard());
/// assert!(!not_line_3.matches(&Version::parse("3.2")?));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparator {
    op: Operator,
    version: Version,
    wildcard: bool,
}

impl Comparator {
    /// A comparator comparing with `version`, such as `>=3` or `<4-rc.2`.
    #[must_use]
    pub fn new(op: Operator, version: Version) -> Self {
        Self {
            op,
            version,
            wildcard: false,
        }
    }

    /// A wildcard comparator for the line of `version`, such as `3.*` or `!=3.*`.
    ///
    /// Returns `None` unless `op` is [`Operator::Equal`] or [`Operator::NotEqual`]
    /// and `version` has neither a prerelease nor build metadata.
    ///
    /// ```
    /// use anyver::{Comparator, Operator, Version};
    ///
    /// # fn main() -> Result<(), anyver::ParseError> {
    /// let line = Comparator::wildcard(Operator::Equal, Version::parse("2026.6")?);
    /// assert_eq!(line.map(|c| c.to_string()).as_deref(), Some("==2026.6.*"));
    /// assert!(Comparator::wildcard(Operator::Greater, Version::parse("3")?).is_none());
    /// assert!(Comparator::wildcard(Operator::Equal, Version::parse("3-rc.1")?).is_none());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn wildcard(op: Operator, version: Version) -> Option<Self> {
        let valid = matches!(op, Operator::Equal | Operator::NotEqual)
            && !version.is_prerelease()
            && version.build_metadata().is_none();
        valid.then_some(Self {
            op,
            version,
            wildcard: true,
        })
    }

    /// The operator, such as [`Operator::GreaterEq`] for `>=3`.
    #[must_use]
    pub fn op(&self) -> Operator {
        self.op
    }

    /// The version to compare with, such as `3` for `>=3` or `3.*`.
    #[must_use]
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Whether this comparator refers to the whole line of its version, as in `3.*`.
    #[must_use]
    pub fn is_wildcard(&self) -> bool {
        self.wildcard
    }

    /// Whether this comparator matches `version`, following the
    /// [line rules](crate#lines).
    #[must_use]
    pub fn matches(&self, version: &Version) -> bool {
        if self.wildcard {
            let in_line = self.compare_line(version).is_eq();
            return in_line == (self.op == Operator::Equal);
        }
        let precedence = || version.cmp_precedence(&self.version);
        match (self.op, self.version.is_prerelease()) {
            (Operator::Equal, _) => precedence().is_eq(),
            (Operator::NotEqual, _) => precedence().is_ne(),
            (Operator::Greater, true) => precedence().is_gt(),
            (Operator::GreaterEq, true) => precedence().is_ge(),
            (Operator::Less, true) => precedence().is_lt(),
            (Operator::LessEq, true) => precedence().is_le(),
            (Operator::Greater, false) => self.compare_line(version).is_gt(),
            (Operator::GreaterEq, false) => self.compare_start(version).is_ge(),
            (Operator::Less, false) => self.compare_start(version).is_lt(),
            (Operator::LessEq, false) => self.compare_line(version).is_le(),
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
        padded(&version.release)
            .take(self.version.release.len())
            .cmp(self.version.release.iter().copied())
    }
}

impl FromStr for Comparator {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        const OPERATORS: [(&str, Operator); 7] = [
            (">=", Operator::GreaterEq),
            ("<=", Operator::LessEq),
            ("==", Operator::Equal),
            ("!=", Operator::NotEqual),
            (">", Operator::Greater),
            ("<", Operator::Less),
            ("=", Operator::Equal),
        ];
        let input = input.trim();
        let (op, version) = OPERATORS
            .iter()
            .find_map(|(symbol, op)| input.strip_prefix(symbol).map(|rest| (*op, rest.trim())))
            .unwrap_or((Operator::Equal, input));
        if version.is_empty() {
            return Err(ParseError::new(input, "comparator is missing a version"));
        }
        let Some(version) = version.strip_suffix(".*") else {
            return Ok(Self::new(op, version.parse()?));
        };
        if !matches!(op, Operator::Equal | Operator::NotEqual) {
            return Err(ParseError::new(
                input,
                "wildcards are only allowed with `==` and `!=`",
            ));
        }
        Self::wildcard(op, version.parse()?).ok_or_else(|| {
            ParseError::new(
                input,
                "wildcard versions must not have a prerelease or build metadata",
            )
        })
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
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Comparator {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_from_str(deserializer)
    }
}

/// The operator of a [`Comparator`].
///
/// Each variant describes how it matches for a comparator version without a
/// prerelease. Without a wildcard and with a prerelease version, all operators
/// compare by precedence. The [crate documentation](crate#lines) explains lines.
///
/// New operators may be added in minor releases, so matches on this type need a
/// wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
        let input = input.trim();
        match input {
            "==" | "=" => Ok(Self::Equal),
            "!=" => Ok(Self::NotEqual),
            ">" => Ok(Self::Greater),
            ">=" => Ok(Self::GreaterEq),
            "<" => Ok(Self::Less),
            "<=" => Ok(Self::LessEq),
            _ => Err(ParseError::new(input, "unknown operator")),
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
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Operator {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_from_str(deserializer)
    }
}

/// An error parsing a [`Version`], [`VersionReq`], [`Comparator`], or [`Operator`].
///
/// [`Display`](std::fmt::Display) combines the input and the problem, as in
/// `error parsing "1.x": release components must be numeric`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    input: String,
    message: &'static str,
}

impl ParseError {
    fn new(input: &str, message: &'static str) -> Self {
        Self {
            input: input.to_owned(),
            message,
        }
    }

    /// The input that failed to parse, without surrounding whitespace.
    ///
    /// For a requirement, this is the part that failed where possible: parsing
    /// `>=3,==x` reports the version `x`.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// What is wrong with the input, such as `release components must be numeric`.
    #[must_use]
    pub fn message(&self) -> &str {
        self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error parsing {:?}: {}", self.input, self.message)
    }
}

impl std::error::Error for ParseError {}

/// One prerelease identifier.
///
/// The variant order makes numeric identifiers precede alphanumeric ones.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum PrereleaseIdentifier {
    Numeric(u64),
    Alphanumeric(String),
}

impl PrereleaseIdentifier {
    /// Parse an identifier that has already been checked for valid characters.
    fn parse(input: &str, identifier: &str) -> Result<Self, ParseError> {
        if !identifier.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(Self::Alphanumeric(identifier.to_owned()));
        }
        identifier.parse().map(Self::Numeric).map_err(|_| {
            ParseError::new(input, "numeric prerelease identifiers must fit into a u64")
        })
    }
}

/// Split a version into its release, prerelease, and build metadata.
///
/// The release cannot contain `-` or `+`, and the prerelease cannot contain `+`, so the
/// first `+` starts the build metadata and the first `-` before it starts the prerelease.
fn split_version(version: &str) -> (&str, Option<&str>, Option<&str>) {
    let (version, build_metadata) = match version.split_once('+') {
        Some((version, build_metadata)) => (version, Some(build_metadata)),
        None => (version, None),
    };
    match version.split_once('-') {
        Some((release, prerelease)) => (release, Some(prerelease), build_metadata),
        None => (version, None, build_metadata),
    }
}

/// Parse one release component.
fn parse_release_component(input: &str, component: &str) -> Result<u64, ParseError> {
    if component.is_empty() {
        return Err(ParseError::new(
            input,
            "release components must not be empty",
        ));
    }
    if !component.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseError::new(input, "release components must be numeric"));
    }
    component
        .parse()
        .map_err(|_| ParseError::new(input, "release components must fit into a u64"))
}

/// A part of a version that consists of dot-separated identifiers.
#[derive(Clone, Copy)]
enum Part {
    Prerelease,
    BuildMetadata,
}

/// Check that a prerelease or build metadata consists of valid identifiers.
fn validate_identifiers(input: &str, identifiers: &str, part: Part) -> Result<(), ParseError> {
    for identifier in identifiers.split('.') {
        if identifier.is_empty() {
            return Err(ParseError::new(
                input,
                match part {
                    Part::Prerelease => "prerelease identifiers must not be empty",
                    Part::BuildMetadata => "build metadata identifiers must not be empty",
                },
            ));
        }
        if !identifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(ParseError::new(
                input,
                match part {
                    Part::Prerelease => {
                        "prerelease identifiers must consist of ASCII letters, digits, and hyphens"
                    }
                    Part::BuildMetadata => {
                        "build metadata identifiers must consist of ASCII letters, digits, and \
                         hyphens"
                    }
                },
            ));
        }
    }
    Ok(())
}

/// Release components followed by infinitely many zeros.
fn padded(release: &[u64]) -> impl Iterator<Item = u64> + '_ {
    release.iter().copied().chain(iter::repeat(0))
}

/// Compare release components, padding the shorter one with zeros.
fn compare_release(left: &[u64], right: &[u64]) -> Ordering {
    let len = left.len().max(right.len());
    padded(left).take(len).cmp(padded(right).take(len))
}

/// Deserialize a value from a string through its [`FromStr`] implementation.
#[cfg(feature = "serde")]
fn deserialize_from_str<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr<Err = ParseError>,
{
    let input = String::deserialize(deserializer)?;
    input.parse().map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::collections::{BTreeSet, HashSet};

    use super::*;

    /// Versions of the schemes the grammar covers parse and keep their spelling.
    #[test]
    fn parses_versions_of_common_schemes() {
        for input in [
            "1.4.2",
            "2.0.0-rc.1",
            "1.4.2+build.7",
            "2026.6",
            "2026.06.25",
            "24.04",
            "42",
            "20260625071935",
            "2026.6.25.2",
            "1.0--",
            "1.0-alpha-1+build-2.x",
            "18446744073709551615",
            "1-18446744073709551615",
            "1+18446744073709551616",
        ] {
            assert_eq!(version(input).to_string(), input);
        }
        assert_eq!(version(" 5.2 ").to_string(), "5.2");
    }

    /// Parsing rejects inputs outside the grammar with a message naming the problem.
    #[test]
    fn rejects_invalid_versions() {
        for (input, message) in [
            ("", "version must not be empty"),
            ("-rc.1", "release must not be empty"),
            ("1..2", "release components must not be empty"),
            ("1.", "release components must not be empty"),
            ("1.x", "release components must be numeric"),
            ("v1.2", "release components must be numeric"),
            (
                "18446744073709551616",
                "release components must fit into a u64",
            ),
            ("1-", "prerelease identifiers must not be empty"),
            ("1.0-alpha..1", "prerelease identifiers must not be empty"),
            (
                "1.0-alpha_1",
                "prerelease identifiers must consist of ASCII letters, digits, and hyphens",
            ),
            (
                "1-18446744073709551616",
                "numeric prerelease identifiers must fit into a u64",
            ),
            ("1.0+", "build metadata identifiers must not be empty"),
            (
                "1.0+build..1",
                "build metadata identifiers must not be empty",
            ),
            (
                "1.0+build+1",
                "build metadata identifiers must consist of ASCII letters, digits, and hyphens",
            ),
            ("1:2.27~rc1-3", "release components must be numeric"),
        ] {
            let error = Version::parse(input).err();
            assert_eq!(
                error.as_ref().map(ParseError::message),
                Some(message),
                "{input:?}"
            );
        }
    }

    /// The accessors return the parts of a version as spelled.
    #[test]
    fn returns_version_parts() {
        let parsed = version("05.2.0-rc-1.01+build-2.x");
        assert_eq!(parsed.release(), [5, 2, 0]);
        assert_eq!(parsed.prerelease(), Some("rc-1.01"));
        assert!(parsed.is_prerelease());
        assert_eq!(parsed.build_metadata(), Some("build-2.x"));

        let release = version("5+b");
        assert_eq!(release.prerelease(), None);
        assert!(!release.is_prerelease());
        assert_eq!(release.build_metadata(), Some("b"));
        assert_eq!(version("5").build_metadata(), None);
    }

    /// Precedence follows the four rules of the crate documentation.
    #[test]
    fn orders_versions_by_precedence() {
        let ordered = [
            "0.9",
            "1.0-0",
            "1.0-1",
            "1.0-2",
            "1.0-10",
            "1.0-RC",
            "1.0-alpha",
            "1.0-alpha.1",
            "1.0-alpha.beta",
            "1.0-alpha10",
            "1.0-alpha9",
            "1.0-beta",
            "1.0-beta.2",
            "1.0-beta.11",
            "1.0-rc",
            "1.0-rc.1",
            "1.0",
            "1.0.1",
            "1.1",
            "1.9",
            "1.10",
            "2",
        ];
        for pair in ordered.windows(2) {
            let ordering = version(pair[0]).cmp_precedence(&version(pair[1]));
            assert_eq!(ordering, Ordering::Less, "{} < {}", pair[0], pair[1]);
        }
    }

    /// Trailing zeros, leading zeros, and build metadata do not affect precedence.
    #[test]
    fn ignores_zeros_and_build_metadata_in_precedence() {
        for (left, right) in [
            ("5", "5.0"),
            ("5", "5.0.0"),
            ("05.002", "5.2"),
            ("1.0-01", "1.0-1"),
            ("1.0-alpha.01", "1.0-alpha.1"),
            ("1.0+build.7", "1.0"),
            ("1.0+build.7", "1.0+build.8"),
        ] {
            let ordering = version(left).cmp_precedence(&version(right));
            assert_eq!(ordering, Ordering::Equal, "{left} = {right}");
        }
        // Leading zeros of alphanumeric identifiers are significant.
        assert!(version("1-01a").cmp_precedence(&version("1-1a")).is_lt());
    }

    /// Equality and hashing compare spelling, and `Ord` breaks precedence ties by it.
    #[test]
    fn compares_identity_by_spelling() {
        assert_eq!(version("5.0"), version(" 5.0 "));
        for (left, right) in [("5", "5.0"), ("05", "5"), ("1+b.7", "1+b.8")] {
            assert_ne!(version(left), version(right), "{left} != {right}");
        }

        let spellings = ["1.0+build.7", "1.0", "1", "1.0-rc.1", "0.9", "1.0+build.7"];
        let hashed: HashSet<Version> = spellings.into_iter().map(version).collect();
        assert_eq!(hashed.len(), 5);
        let sorted: BTreeSet<Version> = spellings.into_iter().map(version).collect();
        let sorted: Vec<String> = sorted.iter().map(ToString::to_string).collect();
        assert_eq!(sorted, ["0.9", "1.0-rc.1", "1", "1.0", "1.0+build.7"]);
    }

    /// Requirements parse with whitespace and display in normalized form.
    #[test]
    fn normalizes_requirements() {
        for (input, display) in [
            (">=2.20,<3", ">=2.20,<3"),
            (" >= 2.20 , < 3 ", ">=2.20,<3"),
            ("= 5", "==5"),
            ("5", "==5"),
            ("3.*", "==3.*"),
            ("!=2026.6.*", "!=2026.6.*"),
            ("*", "*"),
            (" * ", "*"),
        ] {
            assert_eq!(req(input).to_string(), display, "{input:?}");
        }
        assert!(req("*").is_any());
        assert!(!req(">=0").is_any());
    }

    /// Requirement parsing reports the part that failed.
    #[test]
    fn rejects_invalid_requirements() {
        for (input, failing, message) in [
            ("", "", "requirement must not be empty"),
            (">=1,", ">=1,", "comparators must not be empty"),
            (">=1,,<2", ">=1,,<2", "comparators must not be empty"),
            (">=", ">=", "comparator is missing a version"),
            ("*,<2", "*", "release components must be numeric"),
            ("^1.2", "^1.2", "release components must be numeric"),
            ("~1", "~1", "release components must be numeric"),
            (">=3,==x", "x", "release components must be numeric"),
            (
                ">=3.*",
                ">=3.*",
                "wildcards are only allowed with `==` and `!=`",
            ),
            (
                "<3.*",
                "<3.*",
                "wildcards are only allowed with `==` and `!=`",
            ),
            (
                "3-rc.1.*",
                "3-rc.1.*",
                "wildcard versions must not have a prerelease or build metadata",
            ),
            (
                "3+b.*",
                "3+b.*",
                "wildcard versions must not have a prerelease or build metadata",
            ),
            ("3.*.*", "3.*", "release components must be numeric"),
            (".*", "", "version must not be empty"),
        ] {
            let error = VersionReq::parse(input).err();
            assert_eq!(
                error.as_ref().map(|e| (e.input(), e.message())),
                Some((failing, message)),
                "{input:?}"
            );
        }
    }

    /// Every row of the comparator table in the crate documentation holds.
    #[test]
    fn matches_the_documented_comparator_table() {
        assert_matches(&[
            (">=3", "3.0-rc.1", true),
            (">=3", "3.9", true),
            (">=3", "2.9", false),
            ("<4", "3.9", true),
            ("<4", "4.0-rc.1", false),
            (">3", "4.0-rc.1", true),
            (">3", "3.1", false),
            ("<=3", "3.9", true),
            ("<=3", "4.0-rc.1", false),
            ("3.*", "3.0-rc.1", true),
            ("3.*", "3.5.1", true),
            ("3.*", "4.0-rc.1", false),
            ("!=3.*", "4.0", true),
            ("!=3.*", "3.2", false),
            ("==3", "3.0", true),
            ("==3", "3+build.7", true),
            ("==3", "3.0-rc.1", false),
            ("==3", "3.0.1", false),
            ("!=3", "3.0.1", true),
            ("!=3", "3.0", false),
        ]);
    }

    /// A range matches its whole line, including prereleases, but not the next line.
    #[test]
    fn matches_lines_including_prereleases() {
        assert_matches(&[
            (">=3,<4", "3.0-rc.1", true),
            (">=3,<4", "3.5-beta", true),
            (">=3,<4", "3.9.2", true),
            (">=3,<4", "4.0-rc.1", false),
            (">=3,<4", "4-0", false),
            (">=3,<4", "2.9", false),
            (">=2.20,<3", "2.27.0", true),
            (">=2.20,<3", "2.20.0-rc.1", true),
            (">=2.20,<3", "2.19.9", false),
        ]);
    }

    /// Appending zeros narrows the end of a line, but not its start.
    #[test]
    fn respects_the_precision_of_line_ends() {
        assert_matches(&[
            (">3", "3.1", false),
            (">3.0", "3.1", true),
            (">3.0", "3.0.5", false),
            (">3.0.0", "3.0.5", true),
            (">3.0", "3", false),
            ("<=3", "3.0.5", true),
            ("<=3.0", "3.0.5", true),
            ("<=3.0", "3.1", false),
            (">1.2.3", "1.2.3.4", false),
            ("<=1.2.3", "1.2.3.4", true),
        ]);
        for bound in ["3", "3.0", "3.0.0"] {
            assert_matches(&[
                (&format!(">={bound}"), "3-rc.1", true),
                (&format!(">={bound}"), "3.0.0-rc.1", true),
                (&format!("<{bound}"), "3.0-rc.1", false),
                (&format!("<{bound}"), "2.9", true),
            ]);
        }
    }

    /// Comparators whose version has a prerelease compare by precedence.
    #[test]
    fn compares_prerelease_bounds_by_precedence() {
        assert_matches(&[
            (">=3-rc.2", "3-rc.1", false),
            (">=3-rc.2", "3-rc.2", true),
            (">=3-rc.2", "3", true),
            (">=3-rc.2", "4", true),
            (">3-rc.1", "3-rc.2", true),
            ("<4-rc.2", "4-rc.1", true),
            ("<4-rc.2", "4-rc.2", false),
            ("<4-rc.2", "4", false),
            ("<=4-rc.1", "4-rc.2", false),
            ("==1.0-rc.1", "1-rc.01", true),
            ("!=1.0-rc.1", "1.0-rc.2", true),
        ]);
    }

    /// Wildcards match a line of any precision and combine with other comparators.
    #[test]
    fn matches_wildcards() {
        assert_matches(&[
            ("3.*", "3", true),
            ("3.*", "2.9", false),
            ("3.0.*", "3", true),
            ("3.0.*", "3.0.9-rc.1", true),
            ("3.0.*", "3.1", false),
            ("2026.6.*", "2026.06.30", true),
            ("2026.6.*", "2026.7.1", false),
            ("3.*,>=3.2", "3.2-rc.1", true),
            ("3.*,>=3.2", "3.1", false),
            (">=5,!=5.1,<6", "5.0", true),
            (">=5,!=5.1,<6", "5.1.0", false),
            (">=5,!=5.1,<6", "5.2", true),
        ]);
    }

    /// Requirements work the same for calendar versions, serials, and timestamps.
    #[test]
    fn matches_other_schemes() {
        assert_matches(&[
            (">=2024.6,<2025", "2024.6.25", true),
            (">=2024.6,<2025", "2024.12", true),
            (">=2024.6,<2025", "2023.12.31", false),
            (">=2024.6,<2025", "2025.1-rc.1", false),
            (">=2024.6.25,<2024.7", "2024.6.25.3", true),
            (">=42", "43", true),
            (">=20260625000000,<20260626000000", "20260625071935", true),
            (">=20260625000000,<20260626000000", "20260626000000", false),
        ]);
    }

    /// `*` matches every version, including prereleases.
    #[test]
    fn any_matches_everything() {
        for input in ["0", "0.1-alpha", "999.999+b"] {
            assert!(VersionReq::any().matches(&version(input)), "{input}");
        }
    }

    /// Comparators built directly behave like parsed ones.
    #[test]
    fn builds_comparators_directly() {
        let at_least_3 = Comparator::new(Operator::GreaterEq, version("3"));
        assert_eq!(at_least_3, "  >= 3".parse().expect("comparator must parse"));
        assert_eq!(at_least_3.op(), Operator::GreaterEq);
        assert_eq!(at_least_3.version(), &version("3"));
        assert!(!at_least_3.is_wildcard());

        let line = Comparator::wildcard(Operator::NotEqual, version("3"));
        assert_eq!(line, "!=3.*".parse().ok());
        assert!(Comparator::wildcard(Operator::Equal, version("3")).is_some());
        for (op, input) in [
            (Operator::Greater, "3"),
            (Operator::LessEq, "3"),
            (Operator::Equal, "3-rc.1"),
            (Operator::Equal, "3+b"),
        ] {
            assert!(
                Comparator::wildcard(op, version(input)).is_none(),
                "{op}{input}.*"
            );
        }
    }

    /// Requirements can be collected from comparators.
    #[test]
    fn builds_requirements_from_comparators() {
        let requirement: VersionReq = [
            Comparator::new(Operator::GreaterEq, version("2.20")),
            Comparator::new(Operator::Less, version("3")),
        ]
        .into_iter()
        .collect();
        assert_eq!(requirement, req(">=2.20,<3"));
        assert_eq!(VersionReq::from_iter([]), VersionReq::any());
        let single = VersionReq::from(Comparator::new(Operator::Equal, version("5")));
        assert_eq!(single, req("5"));
    }

    /// Operators parse with surrounding whitespace and display canonically.
    #[test]
    fn parses_and_displays_operators() {
        for (input, op, display) in [
            ("==", Operator::Equal, "=="),
            (" = ", Operator::Equal, "=="),
            ("!=", Operator::NotEqual, "!="),
            (">", Operator::Greater, ">"),
            (">=", Operator::GreaterEq, ">="),
            ("<", Operator::Less, "<"),
            ("<=", Operator::LessEq, "<="),
        ] {
            assert_eq!(input.parse(), Ok(op), "{input:?}");
            assert_eq!(op.to_string(), display);
        }
        let error = " => ".parse::<Operator>().err();
        assert_eq!(
            error.as_ref().map(|e| (e.input(), e.message())),
            Some(("=>", "unknown operator"))
        );
    }

    /// Parse errors display their input and message.
    #[test]
    fn displays_parse_errors() {
        let error = Version::parse("1.x").err().map(|e| e.to_string());
        assert_eq!(
            error.as_deref(),
            Some(r#"error parsing "1.x": release components must be numeric"#)
        );
    }

    /// Serialization uses the display form, and deserialization validates it.
    #[cfg(feature = "serde")]
    #[test]
    fn serializes_as_strings() {
        let requirement = serde_json::to_string(&req(" >= 3 , 3.* ")).ok();
        assert_eq!(requirement.as_deref(), Some(r#"">=3,==3.*""#));
        let parsed: Option<Version> = serde_json::from_str(r#""1.0-rc.1""#).ok();
        assert_eq!(parsed, Some(version("1.0-rc.1")));
        let operator: Option<Operator> = serde_json::from_str(r#""<=""#).ok();
        assert_eq!(operator, Some(Operator::LessEq));
        assert!(serde_json::from_str::<Version>(r#""1..2""#).is_err());
        assert!(serde_json::from_str::<VersionReq>(r#""^1""#).is_err());
    }

    /// Assert whether each requirement matches each version.
    fn assert_matches(cases: &[(&str, &str, bool)]) {
        for &(requirement, candidate, expected) in cases {
            let actual = req(requirement).matches(&version(candidate));
            assert_eq!(actual, expected, "{requirement} against {candidate}");
        }
    }

    fn version(input: &str) -> Version {
        Version::parse(input).expect("input must be a valid version")
    }

    fn req(input: &str) -> VersionReq {
        VersionReq::parse(input).expect("input must be a valid version requirement")
    }
}
