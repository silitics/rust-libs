#!/usr/bin/env bash
set -euo pipefail

repository_root=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repository_root"

cargo fmt
taplo fmt
