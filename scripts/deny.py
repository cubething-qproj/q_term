#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Audit dependencies via ``cargo deny``.

Invocation:
  just deny
  uv run --script infra/main/scripts/deny.py

Runs the advisories, bans, and sources checks across the whole workspace at
log-level error, with the dependency-inclusion graph hidden for terser output.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import _common  # noqa: E402

import typer  # noqa: E402

app = typer.Typer(add_completion=False, context_settings={"help_option_names": ["-h", "--help"]})


@app.command()
def main() -> None:
    """Run ``cargo deny check advisories bans sources``."""
    result = _common.run(
        [
            "cargo", "deny",
            "--workspace",
            "-L", "error",
            "check", "advisories", "bans", "sources",
            "--hide-inclusion-graph",
        ],
        check=False,
    )
    raise typer.Exit(result.returncode)


if __name__ == "__main__":
    app()
