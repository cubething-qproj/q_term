#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Run ``bevy lint`` with the right sysroot and isolated target dir.

Invocation:
  just bevy-lint [CARGO_ARGS...]
  uv run --script infra/main/scripts/bevy_lint.py -- [CARGO_ARGS...]

Invokes the ``lint`` subcommand of the ``bevy`` CLI (provided by the upstream
bevy_cli flake), which dispatches to ``bevy_lint_driver``.

Sets ``RUSTC_WRAPPER=`` (disables sccache, which conflicts with bevy_lint's
custom driver) and ``BEVY_LINT_SYSROOT`` to the active toolchain's sysroot.
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
    """Run ``bevy lint --all-features --target-dir=target/bevy_lint``."""
    result = _common.run(
        [
            "bevy",
            "lint",
            "--config",
            'profile.dev.codegen-backend="llvm"',
            "--all-features",
            "--target-dir=target/bevy_lint",
            *ctx.args,
        ],
        env_overrides={
            "RUSTC_WRAPPER": "",
            "BEVY_LINT_SYSROOT": _common.rustc_sysroot(),
        },
        check=False,
    )
    raise typer.Exit(result.returncode)


if __name__ == "__main__":
    app()
