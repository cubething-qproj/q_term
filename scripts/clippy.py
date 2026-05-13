#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Run Clippy with the workspace's standard flags.

Invocation:
  just clippy [CARGO_ARGS...]
  just fix    [CARGO_ARGS...]                  # adds --fix
  uv run --script infra/main/scripts/clippy.py -- [CARGO_ARGS...]

Pins ``--target-dir=target/clippy`` so Clippy's incremental cache does not
collide with plain ``cargo build`` or ``bevy_lint``.
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
    """Run ``cargo clippy --all-features --target-dir=target/clippy``."""
    result = _common.run(
        ["cargo", "clippy", "--all-features", "--target-dir=target/clippy", *ctx.args],
        check=False,
    )
    raise typer.Exit(result.returncode)


if __name__ == "__main__":
    app()
