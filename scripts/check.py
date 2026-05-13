#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Run Clippy and ``bevy_lint`` concurrently.

Invocation:
  just check [PACKAGE]
  uv run --script infra/main/scripts/check.py [PACKAGE]

Both linters use isolated target dirs (``target/clippy`` and
``target/bevy_lint``) so they can build in parallel without contention.
Exits non-zero if either linter fails.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import _common  # noqa: E402
import typer  # noqa: E402

app = typer.Typer(
    add_completion=False, context_settings={"help_option_names": ["-h", "--help"]}
)


@app.command()
def main(
    package: str | None = typer.Argument(
        None, help="Optional cargo package to scope both linters to (-p PACKAGE)."
    ),
) -> None:
    """Run Clippy and bevy_lint in parallel; non-zero if either fails."""
    here = _common.script_dir()
    pkg_args = ["-p", package] if package else []

    clippy_cmd = [str(here / "clippy.py"), *pkg_args]
    bevy_cmd = [str(here / "bevy_lint.py"), *pkg_args]

    _common.echo(clippy_cmd)
    _common.echo(bevy_cmd)
    procs = [
        subprocess.Popen(clippy_cmd),
        subprocess.Popen(bevy_cmd),
    ]
    rcs = [p.wait() for p in procs]
    raise typer.Exit(max(rcs) if any(rcs) else 0)


if __name__ == "__main__":
    app()
