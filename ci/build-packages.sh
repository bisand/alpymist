#!/bin/bash
# Builds Alpymist's packages into ~/packages/ap/$ARCH, and copies them to
# $1 when given. Runs inside the builder container, with the workspace at /src.
#
# Used by build-iso.sh for the image, and by the Packages workflow for the
# repository. The key abuild signs with here is ephemeral and trusted by
# nobody: what installed systems trust is the repository index, which
# `cargo xtask publish` signs off CI with the release key (ADR 0002).
set -euo pipefail

OUT="${1:-}"
ARCH="$(apk --print-arch)"

export CARGO_HOME=/tmp/cargo

if [ ! -e ~/.abuild/abuild.conf ]; then
	abuild-keygen -a -i -n >/dev/null 2>&1
fi

# ghostty is a backport from Alpine edge/testing, where it is packaged but not
# yet in a stable branch; `cargo xtask ghostty-check` reports when that changes.
for pkg in alpymist-keys alpymistctl alpymist-install alpymist-menu alpymist-desktop ghostty squint; do
	mkdir -p ~/ap/"$pkg"
	cp -r /src/aports/"$pkg"/. ~/ap/"$pkg"/
	echo "    $pkg"
	# Local sources, so the checksum is computed here rather than committed.
	( cd ~/ap/"$pkg" && abuild checksum >/dev/null && abuild -r >/dev/null )
done

if [ -n "$OUT" ]; then
	mkdir -p "$OUT/$ARCH"
	cp ~/packages/ap/"$ARCH"/*.apk "$OUT/$ARCH/"
	ls -la "$OUT/$ARCH"
fi
