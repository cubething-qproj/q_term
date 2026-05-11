"""Shared stdlib-only helpers for ``infra/main/scripts/*.py`` uv scripts.

Imported directly (no PEP 723 header) by the wrapper scripts in this
directory. Provides a ``run()`` subprocess wrapper that echoes the command
(``set -x`` style) and a ``script_dir()`` / ``repo_root()`` pair so each
script can locate its siblings without depending on cwd.
"""
from __future__ import annotations

import os
import shlex
import subprocess
import sys
from pathlib import Path


def script_dir() -> Path:
    """Directory containing this ``_common.py`` (i.e. ``infra/main/scripts``)."""
    return Path(__file__).resolve().parent


def echo(cmd: list[str], *, env_overrides: dict[str, str] | None = None) -> None:
    """Print a shell-quoted preview of ``cmd`` to stderr (``set -x`` style)."""
    prefix = ""
    if env_overrides:
        prefix = " ".join(f"{k}={shlex.quote(v)}" for k, v in env_overrides.items()) + " "
    print(f"+ {prefix}{shlex.join(cmd)}", file=sys.stderr, flush=True)


def run(
    cmd: list[str],
    *,
    env_overrides: dict[str, str] | None = None,
    check: bool = True,
    **kwargs: object,
) -> subprocess.CompletedProcess[str]:
    """Run ``cmd`` after echoing it. ``env_overrides`` is layered on ``os.environ``."""
    echo(cmd, env_overrides=env_overrides)
    env = None
    if env_overrides is not None:
        env = {**os.environ, **env_overrides}
    return subprocess.run(cmd, check=check, env=env, **kwargs)  # type: ignore[arg-type]


def rustc_sysroot() -> str:
    """Return ``rustc --print sysroot`` (stripped)."""
    out = subprocess.run(
        ["rustc", "--print", "sysroot"], capture_output=True, text=True, check=True
    )
    return out.stdout.strip()
