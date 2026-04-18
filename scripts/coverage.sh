#!/usr/bin/env bash
set -euo pipefail
if [ $# -eq 0 ]; then
	RUSTFLAGS=-Zcodegen-backend=llvm cargo llvm-cov nextest \
		--html --open
else
	RUSTFLAGS=-Zcodegen-backend=llvm cargo llvm-cov nextest \
		"$@"
fi
