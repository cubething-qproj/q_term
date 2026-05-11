# ------------------------------------------
# SPDX-License-Identifier: MIT OR Apache-2.0
# -------------------------------- 𝒒𝒑𝒓𝒐𝒋 --

scripts := parent_directory(canonicalize(justfile())) / "scripts"

# Build the workspace.
build *args:
    {{scripts}}/build.py {{args}}

# Run the application.
play *args:
    {{scripts}}/play.py {{args}}

# Lint with Clippy and bevy_lint.
check *args:
    {{scripts}}/check.py {{args}}

# Run clippy.
clippy *args:
    {{scripts}}/clippy.py {{args}}

# Run bevy_lint.
bevy-lint *args:
    {{scripts}}/bevy_lint.py {{args}}

# Check dependencies with cargo-deny.
deny:
    {{scripts}}/deny.py

# Run tests via cargo-nextest.
test *args:
    {{scripts}}/test.py {{args}}

# Generate test coverage report.
coverage *args:
    {{scripts}}/coverage.py {{args}}

# Fix all fixable issues.
fix *args:
    {{scripts}}/clippy.py --fix {{args}}

# Test CI locally with act.
ci *args:
    {{scripts}}/ci.py {{args}}
