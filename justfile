# ------------------------------------------
# SPDX-License-Identifier: MIT OR Apache-2.0
# -------------------------------- 𝒒𝒑𝒓𝒐𝒋 --
#
# Downstream `justfile` prelude. NOT a standalone justfile -- the sync
# workflow concatenates this file with `shared.just` and writes the
# combined output as `justfile` in each downstream repo. This keeps the
# recipe definitions (in shared.just) as the single source of truth
# while letting downstreams ship a single justfile rather than a
# justfile + shared.just pair.
#
# `qproj` resolves to a `uvx --refresh` invocation that pulls the
# qproj-scripts CLI directly from the infra repo's `main` branch. The
# `--refresh` flag makes uvx re-resolve `main` to the current commit on
# every invocation; if the commit hasn't moved, the cached install is
# reused (no re-download).
#
# Override the ref by exporting `QPROJ_SCRIPTS_REF` (e.g. a commit SHA or
# branch name) -- useful for bisecting or temporarily holding a downstream
# back from upstream changes.

QPROJ_REF := env_var_or_default("QPROJ_SCRIPTS_REF", "main")
QPROJ_GIT_URL := "git+https://github.com/cubething-qproj/infra.git@" + QPROJ_REF + "#subdirectory=scripts"
qproj := "uvx --refresh --from " + quote(QPROJ_GIT_URL) + " qproj-scripts"
# ------------------------------------------
# SPDX-License-Identifier: MIT OR Apache-2.0
# -------------------------------- 𝒒𝒑𝒓𝒐𝒋 --
#
# Recipes that wrap the qproj-scripts CLI. Consumers of this file
# (metarepo.just, downstream.just) must define `qproj` themselves --
# typically as `uv run --project <local-scripts>` for the metarepo and
# `uvx --from git+...` for downstreams.

# Build the workspace.
build *args:
    {{ qproj }} build {{ args }}

# Run the application.
play *args:
    {{ qproj }} play {{ args }}

# Lint with Clippy and bevy_lint.
check *args:
    {{ qproj }} check {{ args }}

# Run clippy.
clippy *args:
    {{ qproj }} clippy {{ args }}

# Run bevy_lint.
bevy-lint *args:
    {{ qproj }} bevy-lint {{ args }}

# Check dependencies with cargo-deny.
deny:
    {{ qproj }} deny

# Run tests via cargo-nextest.
test *args:
    {{ qproj }} test {{ args }}

# Generate test coverage report.
coverage *args:
    {{ qproj }} coverage {{ args }}

# Fix all fixable issues.
fix *args:
    {{ qproj }} fix {{ args }}

# Test CI locally with act.
ci *args:
    {{ qproj }} ci {{ args }}

# Emit Clippy + bevy_lint diagnostics as JSON for rust-analyzer.
ra-check *args:
    {{ qproj }} ra-check {{ args }}
