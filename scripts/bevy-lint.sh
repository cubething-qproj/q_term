#!/usr/bin/env bash
set -euo pipefail
RUSTC_WRAPPER="" BEVY_LINT_SYSROOT="$(rustc --print sysroot)" bevy_lint \
	--all-features \
	--target-dir=target/bevy_lint "$@"
