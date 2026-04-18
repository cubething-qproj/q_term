# ------------------------------------------
# SPDX-License-Identifier: MIT OR Apache-2.0
# -------------------------------- 𝒒𝒑𝒓𝒐𝒋 --

scripts := parent_directory(canonicalize(justfile())) / "scripts"

# Build the workspace.
build *args:
    {{scripts}}/build.sh {{args}}

# Run the application.
play *args:
    {{scripts}}/play.sh {{args}}

# Lint with Clippy and bevy_lint.
check *args:
    {{scripts}}/check.sh {{args}}

# Run clippy.
clippy *args:
    {{scripts}}/clippy.sh {{args}}

# Run bevy_lint.
bevy-lint *args:
    {{scripts}}/bevy-lint.sh {{args}}

# Check dependencies with cargo-deny.
deny:
    {{scripts}}/deny.sh

# Run tests via cargo-nextest.
test *args:
    {{scripts}}/test.sh {{args}}

# Generate test coverage report.
coverage *args:
    {{scripts}}/coverage.sh {{args}}

# Fix all fixable issues.
fix *args:
    {{scripts}}/clippy.sh --fix {{args}}

# Test CI locally with act.
ci *args:
    {{scripts}}/ci.sh {{args}}
