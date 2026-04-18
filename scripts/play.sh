#!/usr/bin/env bash
set -e

EXAMPLE=""
BUILD_ARGS="-F dylib"
FILE="target/debug/quell"
ENV_VARS=""
CMD_ARGS=""
# Detect metarepo vs standalone layout
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
if [ -d "$REPO_ROOT/../../infra" ]; then
	ASSETS="$REPO_ROOT/../../quell/main/assets"
else
	ASSETS="$REPO_ROOT/assets"
fi

while [[ $# -gt 0 ]]; do
	case "$1" in
	-x | --example)
		EXAMPLE="$2"
		shift 2
		;;
	-B | --build-args)
		BUILD_ARGS="$2"
		shift 2
		;;
	-e | --env)
		ENV_VARS="$2"
		shift 2
		;;
	-a | --args)
		CMD_ARGS="$2"
		shift 2
		;;
	-A | --assets)
		ASSETS="$2"
		shift 2
		;;
	*)
		FILE="$1"
		shift
		;;
	esac
done

if [ -n "$EXAMPLE" ]; then
	CARGO_EXAMPLE="--example $EXAMPLE"
fi

just build ${BUILD_ARGS:-} ${CARGO_EXAMPLE:-}

if [ -n "$EXAMPLE" ]; then
	TARGET_PATH="./target/debug/examples/$EXAMPLE"
else
	TARGET_PATH="$FILE"
fi

function patch_elf() {
	local elfdata=$(readelf -d $TARGET_PATH)
	local libstd=$(echo $elfdata | grep -Po "libstd-[\d\w]+.so")
	local libbevy_dylib=$(echo $elfdata | grep -Po "libbevy_dylib-[\d\w]+\.so")
	local deps_dir="./target/debug/deps"

	# rsync libstd and libbevy_dylib as assets to psync server
	# this will change every time we rebuild
	local dylib_path="./target/debug/libbevy_dylib.so"
	# but this only changes when the toolchain changes
	local stdlib_path="$(find "$(rustc --print sysroot)" -name "$libstd")"
	rsync -avzr -e "/usr/bin/ssh -l psync -p 5022" -L --progress --mkpath $dylib_path $stdlib_path "$PSYNC_SERVER_IP:/home/psync/lib"

	# patch elf file
	patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 "$TARGET_PATH"
	patchelf --replace-needed "$libbevy_dylib" "libbevy_dylib.so" "$TARGET_PATH"
	patchelf --set-rpath "/home/psync/lib" "$TARGET_PATH"
}

if [ -n "${SSH_CLIENT:-}" ]; then
	patch_elf
	set -x
	uvx --from cubething_psync psync \
		"$TARGET_PATH" -e "${ENV_VARS}" -a "${CMD_ARGS}" -A "${ASSETS}"
else
	set -x
	exec "$TARGET_PATH" ${CMD_ARGS}
fi
