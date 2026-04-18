#!/usr/bin/env bash
set -euo pipefail
docker run \
	--name artifact-server \
	-d -p 8080:8080 \
	--add-host artifacts.docker.internal:host-gateway \
	-e AUTH_KEY=foo \
	ghcr.io/jefuller/artifact-server:latest || true

act -P ubuntu-24.04=ghcr.io/catthehacker/ubuntu:act-24.04 \
	--env ACTIONS_RUNTIME_URL=http://artifacts.docker.internal:8080/ \
	--env ACTIONS_RUNTIME_TOKEN=foo \
	--env ACTIONS_CACHE_URL=http://artifacts.docker.internal:8080/ \
	--artifact-server-path .artifacts \
	--env RUSTC_WRAPPER="" \
	"$@"
