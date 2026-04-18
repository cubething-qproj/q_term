#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PKG=""
if [[ "${1:-}" != "" ]]; then PKG="-p $1"; fi
"$SCRIPT_DIR/clippy.sh" $PKG &
"$SCRIPT_DIR/bevy-lint.sh" $PKG &
wait
