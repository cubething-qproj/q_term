# ------------------------------------------
# SPDX-License-Identifier: MIT OR Apache-2.0
# -------------------------------- 𝒒𝒑𝒓𝒐𝒋 --

QPROJ_REF := env("QPROJ_SCRIPTS_REF", "main")
QPROJ_GIT_URL := "git+https://github.com/cubething-qproj/infra.git@" + QPROJ_REF + "#subdirectory=scripts"
SCRIPTS_SRC := env("QPROJ_SCRIPTS_SRC", QPROJ_GIT_URL)
qproj := "uvx --refresh --from " + quote(SCRIPTS_SRC) + " qproj-scripts"

# nixGL wrapper attribute used by `play`. Override per host in .env.local
# (e.g. `export NIXGL=nixVulkanIntel`). The flake itself no longer ships
# nixGL -- we pull it ad-hoc via `nix run` so the devshell stays pure.
NIXGL := env("NIXGL", "nixVulkanNvidia")

_default:
    just --list

# Build the workspace.
build *args:
    {{ qproj }} build {{ args }}

# Run the application.
play *args:
    nix run --impure github:nix-community/nixGL#{{ NIXGL }} -- \
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

# Set up and synchronize proj repos.
sync *args:
    {{ qproj }} sync {{ args }}

# Change where 'active' points to.
target dir:
    {{ qproj }} target {{ dir }}

# Create a new worktree.
add name:
    {{ qproj }} add {{ name }}

# Prune merged branches.
prune:
    {{ qproj }} prune
