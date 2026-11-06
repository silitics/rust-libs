# Silitics' Rust Libraries

This repository contains Rust crates for internal use within Silitics. While the crates are open-source and you are free to use them for whatever purpose you see, **we will not accept any contributions other than bug fixes**.

## Releasing

Each crate is released on its own. To release a crate, bump its version in its `Cargo.toml`, update `Cargo.lock`, and merge the change. Then tag the merged commit with `<crate>-v<version>` and push the tag, for example:

```sh
git tag anyver-v0.2.0
git push origin anyver-v0.2.0
```

The Release workflow checks that the tag matches the crate's version, runs the crate's tests, and publishes the crate to crates.io. It authenticates through crates.io trusted publishing and runs in the `crates-io` environment, so each crate must list this repository's `release.yml` workflow as a trusted publisher on crates.io.
