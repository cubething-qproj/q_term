#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Run the workspace test suite via ``cargo nextest``.

Invocation:
  just test [NEXTEST_ARGS...]
  uv run --script infra/main/scripts/test.py -- [NEXTEST_ARGS...]

With no arguments, runs ``r --workspace`` (run all tests). With arguments,
forwards them verbatim. Always pins ``--config-file=./.config/nextest.toml``.
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
    """Run ``cargo nextest`` with the workspace nextest config."""
    base = ["cargo", "nextest", "--config-file=./.config/nextest.toml"]
    cmd = base + (ctx.args if ctx.args else ["r", "--workspace"])
    result = _common.run(cmd, check=False, env_overrides={"RUSTC_WRAPPER": "sccache"})
    raise typer.Exit(result.returncode)


if __name__ == "__main__":
    app()
