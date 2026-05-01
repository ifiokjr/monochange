#!/usr/bin/env bash
# without-nix-env.sh — run a command with Nix-specific environment
# variables stripped so host binaries (e.g. /usr/bin/env invoked
# via shebangs) don't load incompatible Nix glibc libraries.
set -euo pipefail

exec env -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT \
	-u NIX_LD -u NIX_LD_LIBRARY_PATH \
	-u NIX_CFLAGS_COMPILE -u NIX_LDFLAGS \
	"$@"
