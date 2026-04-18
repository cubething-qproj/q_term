#!/usr/bin/env bash
set -euo pipefail
cargo clippy --all-features \
	--target-dir=target/clippy "$@"
