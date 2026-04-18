#!/usr/bin/env bash
set -ex
if [ $# -eq 0 ]; then
	cargo nextest \
		--config-file=./.config/nextest.toml \
		r --workspace
else
	cargo nextest \
		--config-file=./.config/nextest.toml \
		"$@"
fi
