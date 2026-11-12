# Silitics' Rust Libraries

This repository contains Rust crates for internal use within Silitics. While the crates are open-source and you are free to use them for whatever purpose you see, **we will not accept any contributions other than bug fixes**.

## Development

Install Nix with flakes enabled and [mise](https://mise.jdx.dev/), then run:

```sh
mise install
mise run check
mise run test
```

Mise supplies the pinned nightly Rust toolchain. Tasks enter the Nix environment
for native build tools, cargo-deny, Taplo, and ShellCheck. Use `mise run fmt` to
format Rust and Cargo manifests, and `mise run doc` to build documentation.
