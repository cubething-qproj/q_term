#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Run GitHub Actions workflows locally via ``act``.

Invocation:
  just ci [ACT_ARGS...]
  uv run --script infra/main/scripts/ci.py -- [ACT_ARGS...]

Boots a local artifact server in the background (idempotent — ignores
``docker run`` failure if the container already exists) and points act at
it via the ``ACTIONS_RUNTIME_*`` env vars.
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
    """Boot the artifact server (best-effort) and run ``act``."""
    docker_cmd = [
        "docker",
        "run",
        "--name",
        "artifact-server",
        "-d",
        "-p",
        "8080:8080",
        "--add-host",
        "artifacts.docker.internal:host-gateway",
        "-e",
        "AUTH_KEY=foo",
        "ghcr.io/jefuller/artifact-server:latest",
    ]
    _common.echo(docker_cmd)
    subprocess.run(docker_cmd, check=False)  # idempotent: tolerate "already exists"

    act_cmd = [
        "act",
        "-P",
        "ubuntu-24.04=ghcr.io/catthehacker/ubuntu:act-24.04",
        "--env",
        "ACTIONS_RUNTIME_URL=http://artifacts.docker.internal:8080/",
        "--env",
        "ACTIONS_RUNTIME_TOKEN=foo",
        "--env",
        "ACTIONS_CACHE_URL=http://artifacts.docker.internal:8080/",
        "--artifact-server-path",
        ".artifacts",
        *ctx.args,
    ]
    result = _common.run(act_cmd, check=False)
    raise typer.Exit(result.returncode)


if __name__ == "__main__":
    app()
