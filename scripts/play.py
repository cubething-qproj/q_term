#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "typer>=0.12",
# ]
# ///
"""Run a freshly-built quell binary, with ergonomic local + remote (psync) modes.

Invocation:
  just play                              # builds + runs target/debug/quell
  just play -p PACKAGE                   # builds + runs target/debug/PACKAGE
  just play -x EXAMPLE                   # builds + runs target/debug/examples/EXAMPLE
  just play PATH                         # runs the given binary
  uv run --script infra/main/scripts/play.py -- [FLAGS] [PATH]

Local mode (no ``$SSH_CLIENT``):
  Re-adds ``target/debug/deps`` to ``LD_LIBRARY_PATH`` so dylib-feature builds
  resolve ``libbevy_dylib-<hash>.so`` under nix-shell, and routes through a
  nixGL wrapper when it can detect (or be told) the right one.

Remote mode (``$SSH_CLIENT`` set):
  patchelf-rewrites the binary so it loads through the host's standard
  loader + a /home/psync/lib RPATH, rsyncs libstd / libbevy_dylib to the
  psync server, and hands off to ``cubething_psync`` via uvx.
"""

from __future__ import annotations

import os
import re
import shlex
import shutil
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


# ---------------------------------------------------------------------------
# Asset path resolution (metarepo vs standalone layout)
# ---------------------------------------------------------------------------


def _default_assets() -> Path:
    """Pick the right ``assets/`` dir for metarepo vs standalone checkouts.

    In metarepo mode, follow the ``quell/active`` symlink so the assets
    tracked match the source tree the workspace is currently building
    against (i.e. switching worktrees with ``just ws wt switch quell ...``
    repoints both code and assets in lockstep).
    """
    repo_root = _common.script_dir().parent
    metarepo = repo_root.parent.parent / "infra"
    if metarepo.is_dir():
        return repo_root.parent.parent / "quell" / "active" / "assets"
    return repo_root / "assets"


# ---------------------------------------------------------------------------
# Remote (SSH / psync) mode helpers
# ---------------------------------------------------------------------------


_LIBSTD_RE = re.compile(r"libstd-[0-9A-Za-z]+\.so")
_LIBBEVY_RE = re.compile(r"libbevy_dylib-[0-9A-Za-z]+\.so")


def _patch_elf_for_psync(target: Path) -> None:
    """patchelf the binary + rsync its dynamic deps to the psync server."""
    elfdata = subprocess.run(
        ["readelf", "-d", str(target)], capture_output=True, text=True, check=True
    ).stdout
    libstd_match = _LIBSTD_RE.search(elfdata)
    libbevy_match = _LIBBEVY_RE.search(elfdata)
    if not libstd_match or not libbevy_match:
        raise typer.BadParameter(
            "could not find libstd / libbevy_dylib NEEDED entries in the binary"
        )
    libstd = libstd_match.group(0)
    libbevy = libbevy_match.group(0)

    dylib_path = Path("./target/debug/libbevy_dylib.so")
    sysroot = Path(_common.rustc_sysroot())
    stdlib_candidates = list(sysroot.rglob(libstd))
    if not stdlib_candidates:
        raise typer.BadParameter(f"could not locate {libstd} under {sysroot}")
    stdlib_path = stdlib_candidates[0]

    psync_ip = os.environ.get("PSYNC_SERVER_IP")
    if not psync_ip:
        raise typer.BadParameter("PSYNC_SERVER_IP not set; required in SSH mode")

    _common.run(
        [
            "rsync",
            "-avzr",
            "-e",
            "/usr/bin/ssh -l psync -p 5022",
            "-L",
            "--progress",
            "--mkpath",
            str(dylib_path),
            str(stdlib_path),
            f"{psync_ip}:/home/psync/lib",
        ]
    )

    _common.run(
        ["patchelf", "--set-interpreter", "/lib64/ld-linux-x86-64.so.2", str(target)]
    )
    _common.run(
        ["patchelf", "--replace-needed", libbevy, "libbevy_dylib.so", str(target)]
    )
    _common.run(["patchelf", "--set-rpath", "/home/psync/lib", str(target)])


# ---------------------------------------------------------------------------
# nixGL detection (local mode)
# ---------------------------------------------------------------------------


def _autodetect_nixgl() -> str:
    """Pick a nixVulkan* wrapper based on the host GPU vendor."""
    if Path("/proc/driver/nvidia/version").exists():
        return "nixVulkanNvidia"
    pick = ""
    for vendor_file in Path("/sys/class/drm").glob("card*/device/vendor"):
        try:
            vid = vendor_file.read_text().strip()
        except OSError:
            continue
        # 0x10de = NVIDIA, 0x1002 = AMD, 0x8086 = Intel.
        if vid == "0x10de":
            return "nixVulkanNvidia"
        if vid in ("0x1002", "0x8086"):
            pick = "nixVulkanIntel"
    return pick


def _resolve_nixgl_with_suffix(name: str) -> str:
    """nixGL ships versioned wrappers (e.g. ``nixGLNvidia-580.159.03``).

    If the bare name isn't on PATH, scan PATH for any ``<name>-*`` and use it.
    """
    for d in os.environ.get("PATH", "").split(os.pathsep):
        if not d:
            continue
        try:
            for entry in Path(d).iterdir():
                if entry.name.startswith(f"{name}-") and os.access(entry, os.X_OK):
                    return entry.name
        except (FileNotFoundError, NotADirectoryError, PermissionError):
            continue
    return ""


def _resolve_nixgl(override: str | None) -> str:
    """Return the nixGL wrapper to prepend to the run command, or "" for none.

    Precedence: ``-G`` flag > ``$NIXGL`` > GPU autodetect. ``$NO_NIXGL=1``
    short-circuits everything to "".
    """
    if os.environ.get("NO_NIXGL"):
        return ""
    name = override or os.environ.get("NIXGL") or _autodetect_nixgl()
    if not name:
        return ""
    if shutil.which(name):
        return name
    resolved = _resolve_nixgl_with_suffix(name)
    if resolved:
        return resolved

    in_nix_shell = os.environ.get("IN_NIX_SHELL", "")
    print(
        f"play.py: detected GPU wants '{name}' but it isn't on PATH.",
        file=sys.stderr,
    )
    if not in_nix_shell:
        print(
            "  IN_NIX_SHELL is unset -- you are not inside a nix devshell.",
            file=sys.stderr,
        )
        print(
            "  direnv likely hasn't fired in this shell. Try `direnv reload`,",
            file=sys.stderr,
        )
        print(
            "  open a fresh terminal in the project root, or run via:",
            file=sys.stderr,
        )
        print(
            "    nix develop --impure ./infra/main#nvidia -c just play ...",
            file=sys.stderr,
        )
    elif name == "nixVulkanNvidia":
        print(
            "  Enter the NVIDIA devshell: nix develop --impure ./infra/main#nvidia",
            file=sys.stderr,
        )
        print(
            "  Or in .envrc:               use flake --impure ./infra/main#nvidia",
            file=sys.stderr,
        )
    elif name == "nixVulkanIntel":
        print(
            "  Enter the default devshell: nix develop --impure ./infra/main",
            file=sys.stderr,
        )
    print(
        "  Override with -G <wrapper> or NIXGL=<wrapper>; disable with NO_NIXGL=1.",
        file=sys.stderr,
    )
    print(
        "play.py: refusing to launch -- the binary would panic at vkCreateInstance.",
        file=sys.stderr,
    )
    raise typer.Exit(code=1)


# ---------------------------------------------------------------------------
# Main command
# ---------------------------------------------------------------------------


@app.command()
def main(
    ctx: typer.Context,
    example: str | None = typer.Option(
        None, "-x", "--example", help="Cargo --example name."
    ),
    package: str | None = typer.Option(
        None, "-p", "--package", help="Cargo -p package."
    ),
    build_args: str = typer.Option(
        "-F dylib", "-B", "--build-args", help="Extra args forwarded to `just build`."
    ),
    env_vars: str = typer.Option(
        "", "-e", "--env", help="Env-var string forwarded to psync (remote mode only)."
    ),
    cmd_args: str = typer.Option(
        "", "-a", "--args", help="Args appended to the binary invocation."
    ),
    assets: Path | None = typer.Option(
        None,
        "-A",
        "--assets",
        help="Override the assets directory (default: autodetected).",
    ),
    nixgl_override: str | None = typer.Option(
        None,
        "-G",
        "--nixgl",
        help="Force a specific nixGL wrapper (e.g. nixVulkanNvidia).",
    ),
) -> None:
    """Build, then exec/relay the binary locally or to the psync host."""
    file_set = False
    file_path = Path("target/debug/quell")
    extras = list(ctx.args)
    if extras:
        # First positional extra is treated as the explicit binary path.
        file_path = Path(extras[0])
        file_set = True

    if assets is None:
        assets = _default_assets()

    cargo_example = ["--example", example] if example else []
    cargo_package: list[str] = []
    if package:
        cargo_package = ["-p", package]
        if not file_set:
            file_path = Path(f"target/debug/{package}")

    # Build via `just build`.
    build_cmd = [
        "just",
        "build",
        *shlex.split(build_args),
        *cargo_package,
        *cargo_example,
    ]
    _common.run(build_cmd)

    target_path = Path(f"./target/debug/examples/{example}") if example else file_path

    if os.environ.get("SSH_CLIENT"):
        _patch_elf_for_psync(target_path)
        _common.run(
            [
                "uvx",
                "--from",
                "cubething_psync",
                "psync",
                str(target_path),
                "-e",
                env_vars,
                "-a",
                cmd_args,
                "-A",
                str(assets),
            ]
        )
        return

    # Local exec path.
    deps_dir = Path.cwd() / "target" / "debug" / "deps"
    existing_ld = os.environ.get("LD_LIBRARY_PATH", "")
    run_ld_path = f"{deps_dir}{':' + existing_ld if existing_ld else ''}"

    nixgl_cmd = _resolve_nixgl(nixgl_override)

    final = []
    if nixgl_cmd:
        final.append(nixgl_cmd)
    final.append(str(target_path))
    final.extend(shlex.split(cmd_args))

    _common.echo(final, env_overrides={"LD_LIBRARY_PATH": run_ld_path})
    os.environ["LD_LIBRARY_PATH"] = run_ld_path
    os.execvp(final[0], final)


if __name__ == "__main__":
    app()
