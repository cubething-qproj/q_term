#!/usr/bin/env bash
set -euo pipefail
cargo deny --workspace \
	-L error \
	check advisories bans sources \
	--hide-inclusion-graph
