#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Emit Clippy + bevy_lint diagnostics in JSON form for rust-analyzer.

Invocation:
  uv run --script infra/main/scripts/ra_check.py -- [CARGO_ARGS...]

Configured in the editor as the ``check.overrideCommand`` so RA shows
both Clippy lints and bevy_lint lints as inline diagnostics. Each linter
gets an isolated target dir to avoid step-on-toes incremental rebuilds.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import _common  # noqa: E402

import typer  # noqa: E402

app = typer.Typer(
    add_completion=False,
    context_settings={
        "allow_extra_args": True,
        "ignore_unknown_options": True,
        "help_option_names": ["-h", "--help"],
    },
)


@app.command()
def main(ctx: typer.Context) -> None:
    """Run Clippy then bevy_lint, both with JSON-rendered-ANSI diagnostics."""
    extra = list(ctx.args)

    clippy_rc = subprocess.run(
        [
            "cargo", "clippy", "--all-features",
            "--target-dir=target/ra-clippy",
            "--message-format=json-diagnostic-rendered-ansi",
            *extra,
        ],
        stderr=subprocess.DEVNULL,
    ).returncode

    bevy_rc = _common.run(
        [
            "bevy_lint", "--all-features",
            "--target-dir=target/ra-bevy-lint",
            "--message-format=json-diagnostic-rendered-ansi",
            *extra,
        ],
        env_overrides={"RUSTC_WRAPPER": ""},
        check=False,
        stderr=subprocess.DEVNULL,
    ).returncode

    raise typer.Exit(clippy_rc or bevy_rc)


if __name__ == "__main__":
    app()
