# anyver

Version parsing and requirement matching across versioning schemes.

Software is versioned in many ways: libraries use [Semantic Versioning](https://semver.org),
operating system images often use [Calendar Versioning](https://calver.org), firmware counts
builds, and CI pipelines stamp timestamps. `anyver` parses all of these with one grammar,
orders them numerically, and matches them against requirements, without assuming a scheme up
front:

```rust
use anyver::{Version, VersionReq};

let requirement: VersionReq = ">=2.20,<3".parse()?;
assert!(requirement.matches(&Version::parse("2.27.0")?));

// Calendar versions and serial numbers use the same grammar.
assert!(VersionReq::parse("2026.6.*")?.matches(&Version::parse("2026.06.25")?));
assert!(Version::parse("20260625071935")? > Version::parse("20260624235959")?);
```

Requirements are designed for checking whether installed components fit together. `>=3,<4`
matches every version of the 3 line, including the release candidate `3.0-rc.1`, but not
`4.0-rc.1`, which already belongs to the next line.

The [documentation](https://docs.rs/anyver) defines the version grammar, the ordering, and
the requirement rules in detail. Enable the `serde` feature to serialize versions and
requirements as strings.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
