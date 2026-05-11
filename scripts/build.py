#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Build the workspace.

Invocation:
  just build [CARGO_ARGS...]
  uv run --script infra/main/scripts/build.py -- [CARGO_ARGS...]

Thin wrapper over ``cargo build``. All extra arguments are forwarded.
"""
from __future__ import annotations

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
    """Run ``cargo build`` with any forwarded arguments."""
    result = _common.run(["cargo", "build", *ctx.args], check=False)
    raise typer.Exit(result.returncode)


if __name__ == "__main__":
    app()
