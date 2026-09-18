#!/bin/bash
# Builds Alpymist's packages into ~/packages/ap/$ARCH, and copies them to
# $1 when given. Runs inside the builder container, with the workspace at /src.
#
# Used by build-iso.sh for the image, and by the Release workflow for the
# repository. The key abuild signs with here is ephemeral and trusted by
# nobody: what installed systems trust is the repository index, which
# `cargo xtask publish` signs off CI with the release key (ADR 0002).
#
# Four variables change what it does:
#
#   BUILD          the number of the CI run building these packages, which
#                  becomes every first-party package's pkgrel: 0.0.4 is built
#                  as 0.0.4-r8 by run 8 and 0.0.4-r9 by run 9. Nobody bumps a
#                  pkgrel by hand, and no two builds of one version collide
#                  (ADR 0008). Unset, the committed pkgrel of 0 stands, which
#                  is what a local build wants.
#   CHANNEL        stable (the default) or dev. For dev, each first-party
#                  package's pkgver gets _git and the UTC time of the last
#                  commit that could change it: 0.0.4 becomes
#                  0.0.4_git20260915120301. apk sorts that above 0.0.4 and
#                  below 0.0.5, so dev stays ahead of the release it follows
#                  and meets the next one when it arrives (ADR 0006). Needs the
#                  git history at /src.
#   PACKAGE_CACHE  a directory kept between builds. squint is built from a
#                  pinned upstream release, so what it builds to depends only on
#                  its aport and the builder: it is kept there and not built
#                  again until one of those changes.
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
CACHED=" squint "
CHANNEL="${CHANNEL:-stable}"
BUILD="${BUILD:-}"
# Built from a pinned upstream release or a key file, not from this workspace:
# versioned by hand, on every channel, and given neither the dev stamp nor the
# build number. xtask's version command holds the same list.
INDEPENDENT=" squint alpymist-keys "

export CARGO_HOME=/tmp/cargo

case "$CHANNEL" in
stable | dev) ;;
*)
	echo "CHANNEL is stable or dev, not $CHANNEL" >&2
	exit 1
	;;
esac

if [ -n "$BUILD" ]; then
	case "$BUILD" in
	*[!0-9]*)
		echo "BUILD is the number of the run building these, not $BUILD" >&2
		exit 1
		;;
	esac
fi

# The UTC time of the last commit touching what a first-party package is built
# from: its aport, and the workspace every one of them copies in.
stamp() {
	TZ=UTC git -C /src -c safe.directory=/src log -1 \
		--format=%cd --date=format-local:%Y%m%d%H%M%S -- \
		"aports/$1" crates desktop Cargo.toml Cargo.lock rust-toolchain.toml
}

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
	for pkg in alpymist-keys alpymist alpymistctl alpymist-install alpymist-menu alpymist-about alpymist-wifi alpymist-auth alpymist-power alpymist-settings alpymist-screensaver alpymist-saver-mountains alpymist-store alpymist-greeter alpymist-splash alpymist-desktop squint; do
		mkdir -p ~/ap/"$pkg"
		cp -r /src/aports/"$pkg"/. ~/ap/"$pkg"/
		if [[ "$INDEPENDENT" != *" $pkg "* ]]; then
			if [ "$CHANNEL" = dev ]; then
				when=$(stamp "$pkg")
				[ -n "$when" ] || { echo "no commit touches $pkg; is the history there?" >&2; exit 1; }
				sed -i "s/^pkgver=\([^_]*\)$/pkgver=\1_git$when/" ~/ap/"$pkg"/APKBUILD
				grep -q "^pkgver=.*_git$when$" ~/ap/"$pkg"/APKBUILD \
					|| { echo "could not stamp $pkg's pkgver" >&2; exit 1; }
			fi
			# The build number, so a rebuild of an unchanged version is still a
			# new package to apk and reaches systems that have the old one.
			if [ -n "$BUILD" ]; then
				sed -i "s/^pkgrel=.*$/pkgrel=$BUILD/" ~/ap/"$pkg"/APKBUILD
				grep -q "^pkgrel=$BUILD$" ~/ap/"$pkg"/APKBUILD \
					|| { echo "could not number $pkg's build" >&2; exit 1; }
			fi
		fi
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
		# Our own packages build from the workspace copied in above, so their
		# checksums describe files that were just written and are computed here
		# rather than committed. squint is fetched from upstream, and its
		# committed sha512 is the pin: `abuild checksum` would delete that block
		# and write whatever was downloaded, so it is not run over it. abuild
		# checks the sums itself while fetching, and stops if they disagree.
		( cd ~/ap/"$pkg" \
			&& { [ "$pkg" = squint ] || abuild checksum >/dev/null; } \
			&& abuild -r >/dev/null )
		if [ -n "$key" ]; then
			# Whoever restored the cache may not be who builds here.
			rm -rf "${CACHE:?}/$pkg" || {
				echo "cannot replace $pkg in $CACHE: it belongs to someone else" >&2
				exit 1
			}
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
