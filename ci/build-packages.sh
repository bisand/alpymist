#!/bin/bash
# Builds Alpymist's packages into ~/packages/ap/$ARCH, and copies them to
# $1 when given. Runs inside the builder container, with the workspace at /src.
#
# Used by build-iso.sh for the image, and by the Release workflow for the
# repository. The key abuild signs with here is ephemeral and trusted by
# nobody: what installed systems trust is the repository index, which
# `cargo xtask publish` signs off CI with the release key (ADR 0002).
#
# Two variables change what it does:
#
#   PACKAGE_CACHE  a directory kept between builds. ghostty and squint are
#                  built from a pinned upstream commit, so what they build to
#                  depends only on their aport and the builder: they are kept
#                  there and not built again until one of those changes.
#                  ghostty's Zig build alone is most of the time this takes.
#   PREBUILT       a directory of this architecture's packages, already built
#                  by an earlier job. Nothing is built: they are indexed and
#                  signed, which is all mkimage needs of a repository.
set -euo pipefail

OUT="${1:-}"
ARCH="$(apk --print-arch)"
REPO=~/packages/ap/"$ARCH"
CACHE="${PACKAGE_CACHE:-}"
CACHED=" ghostty squint "

export CARGO_HOME=/tmp/cargo

if [ ! -e ~/.abuild/abuild.conf ]; then
	abuild-keygen -a -i -n >/dev/null 2>&1
fi

# The index over everything in the repository, signed with this build's key.
# abuild -r writes one as it goes; packages that did not pass through it need
# this instead.
reindex() {
	(
		cd "$REPO"
		rm -f APKINDEX.tar.gz
		apk index --allow-untrusted --no-warnings --quiet \
			--rewrite-arch "$ARCH" --output APKINDEX.tar.gz *.apk
		abuild-sign -q APKINDEX.tar.gz
	)
}

if [ -n "${PREBUILT:-}" ]; then
	echo "    using the packages in $PREBUILT"
	mkdir -p "$REPO"
	cp "$PREBUILT"/*.apk "$REPO"/
	reindex
else
	copied=false
	# ghostty is a backport from Alpine edge/testing, where it is packaged but
	# not yet in a stable branch; `cargo xtask ghostty-check` reports when that
	# changes.
	for pkg in alpymist-keys alpymistctl alpymist-install alpymist-menu alpymist-desktop ghostty squint; do
		mkdir -p ~/ap/"$pkg"
		cp -r /src/aports/"$pkg"/. ~/ap/"$pkg"/
		key=""
		if [ -n "$CACHE" ] && [[ "$CACHED" == *" $pkg "* ]]; then
			key=$(cd /src && sha256sum aports/"$pkg"/* builder/Dockerfile | sha256sum | cut -c1-16)
			if [ -f "$CACHE/$pkg/$key" ]; then
				echo "    $pkg (kept from an earlier build)"
				mkdir -p "$REPO"
				cp "$CACHE/$pkg"/*.apk "$REPO"/
				copied=true
				continue
			fi
		fi
		echo "    $pkg"
		# Local sources, so the checksum is computed here rather than committed.
		( cd ~/ap/"$pkg" && abuild checksum >/dev/null && abuild -r >/dev/null )
		if [ -n "$key" ]; then
			rm -rf "${CACHE:?}/$pkg"
			mkdir -p "$CACHE/$pkg"
			( cd ~/ap/"$pkg" && abuild listpkg ) | while read -r apk; do
				cp "$REPO/$apk" "$CACHE/$pkg"/
			done
			touch "$CACHE/$pkg/$key"
		fi
	done
	if $copied; then
		reindex
	fi
fi

if [ -n "$OUT" ]; then
	mkdir -p "$OUT/$ARCH"
	cp "$REPO"/*.apk "$OUT/$ARCH/"
	ls -la "$OUT/$ARCH"
fi
