#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Generate a coverage report via ``cargo llvm-cov nextest``.

Invocation:
  just coverage                       # HTML report, opens in browser
  just coverage [LLVM_COV_ARGS...]    # forward custom flags
  uv run --script infra/main/scripts/coverage.py -- [ARGS...]

Forces ``RUSTFLAGS=-Zcodegen-backend=llvm`` so the cranelift backend (default
in our nightly devshell) is bypassed for the instrumented build.
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
    """Run ``cargo llvm-cov nextest`` with the LLVM codegen backend."""
    args = ctx.args if ctx.args else ["--html", "--open"]
    cmd = ["cargo", "llvm-cov", "nextest", *args]
    result = _common.run(
        cmd,
        env_overrides={"RUSTFLAGS": "-Zcodegen-backend=llvm"},
        check=False,
    )
    raise typer.Exit(result.returncode)


if __name__ == "__main__":
    app()
