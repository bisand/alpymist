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
#                  by an earlier job, with the public keys they were signed
#                  with in keys/. Nothing is built: the keys are trusted and
#                  the packages indexed, which is all mkimage needs.
#
# apk checks each package's own signature, not only the index's, and every
# build signs with a key of its own. So the public half of each key the
# repository's packages are signed with is kept in ~/packages/keys, installed
# where apk looks, and handed on beside the packages in $1/$ARCH/keys.
set -euo pipefail

OUT="${1:-}"
ARCH="$(apk --print-arch)"
REPO=~/packages/ap/"$ARCH"
KEYS=~/packages/keys
CACHE="${PACKAGE_CACHE:-}"
CACHED=" ghostty squint "

export CARGO_HOME=/tmp/cargo

if [ ! -e ~/.abuild/abuild.conf ]; then
	abuild-keygen -a -i -n >/dev/null 2>&1
fi
mkdir -p "$KEYS"
cp ~/.abuild/*.rsa.pub "$KEYS"/

# Trust the public keys in a directory: for apk here, and for the image, which
# mkimage gives this container's keys.
trust() {
	cp "$1"/*.rsa.pub "$KEYS"/
	doas cp "$1"/*.rsa.pub /etc/apk/keys/
}

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
	trust "$PREBUILT"/keys
	mkdir -p "$REPO"
	cp "$PREBUILT"/*.apk "$REPO"/
	reindex
else
	copied=false
	# ghostty is a backport from Alpine edge/testing, where it is packaged but
	# not yet in a stable branch; `cargo xtask ghostty-check` reports when that
	# changes.
	for pkg in alpymist-keys alpymistctl alpymist-install alpymist-menu alpymist-wifi alpymist-greeter alpymist-splash alpymist-desktop ghostty squint; do
		mkdir -p ~/ap/"$pkg"
		cp -r /src/aports/"$pkg"/. ~/ap/"$pkg"/
		key=""
		if [ -n "$CACHE" ] && [[ "$CACHED" == *" $pkg "* ]]; then
			key=$(cd /src && sha256sum aports/"$pkg"/* builder/Dockerfile | sha256sum | cut -c1-16)
			if [ -f "$CACHE/$pkg/$key" ] && compgen -G "$CACHE/$pkg/*.rsa.pub" >/dev/null; then
				echo "    $pkg (kept from an earlier build)"
				trust "$CACHE/$pkg"
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
			cp ~/.abuild/*.rsa.pub "$CACHE/$pkg"/
			touch "$CACHE/$pkg/$key"
		fi
	done
	if $copied; then
		reindex
	fi
fi

if [ -n "$OUT" ]; then
	mkdir -p "$OUT/$ARCH/keys"
	cp "$REPO"/*.apk "$OUT/$ARCH/"
	cp "$KEYS"/*.rsa.pub "$OUT/$ARCH/keys/"
	ls -la "$OUT/$ARCH"
fi
