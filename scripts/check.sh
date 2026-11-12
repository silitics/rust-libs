#!/usr/bin/env bash
set -euo pipefail

repository_root=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repository_root"

shellcheck scripts/check.sh scripts/fmt.sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo deny check
taplo fmt --check
