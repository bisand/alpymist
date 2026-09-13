#!/bin/bash
# Builds the Alpymist ISO. Runs inside the builder container; see `make iso`.
#
# Shell rather than Rust because every step is a direct invocation of Alpine's
# own build tools, and wrapping `abuild` and `mkimage.sh` in a process spawner
# would add indirection without adding logic. The orchestration that does carry
# logic — booting the result and asserting on it — is Rust, in xtask/.
set -euo pipefail

ARCH="${1:-$(uname -m)}"
ALPINE_BRANCH="${ALPINE_BRANCH:-3.24-stable}"
ALPINE_VERSION="${ALPINE_VERSION:-v3.24}"
TAG="${TAG:-0.0.1}"
MIRROR="${MIRROR:-https://dl-cdn.alpinelinux.org/alpine}"

export CARGO_HOME=/tmp/cargo

echo ">>> [1/4] generating a local package signing key"
# Ephemeral, for local builds only. Release signing uses an offline key that
# never touches a build container — see ADR 0002.
abuild-keygen -a -i -n >/dev/null 2>&1

echo ">>> [2/4] building Alpymist packages"
for pkg in alpymistctl alpymist-install; do
	mkdir -p ~/ap/"$pkg"
	cp -r /src/aports/"$pkg"/. ~/ap/"$pkg"/
	echo "    $pkg"
	# Local sources, so the checksum is computed here rather than committed.
	( cd ~/ap/"$pkg" && abuild checksum >/dev/null && abuild -r >/dev/null )
done

echo ">>> [3/4] fetching Alpine image scripts ($ALPINE_BRANCH)"
if [ ! -d /tmp/aports ]; then
	git clone --filter=blob:none --no-checkout --depth 1 -b "$ALPINE_BRANCH" \
		https://gitlab.alpinelinux.org/alpine/aports.git /tmp/aports >/dev/null 2>&1
	git -C /tmp/aports sparse-checkout init --cone >/dev/null
	git -C /tmp/aports sparse-checkout set scripts >/dev/null
	git -C /tmp/aports checkout >/dev/null 2>&1
fi
cp /src/profiles/mkimg.alpymist.sh /src/profiles/genapkovl-alpymist.sh /tmp/aports/scripts/
chmod +x /tmp/aports/scripts/genapkovl-alpymist.sh

echo ">>> [4/4] building the image for $ARCH"
cd /tmp/aports/scripts
sh mkimage.sh \
	--tag "$TAG" \
	--outdir /out \
	--workdir /tmp/work \
	--arch "$ARCH" \
	--profile alpymist \
	--hostkeys \
	--repository "$MIRROR/$ALPINE_VERSION/main" \
	--repository "$MIRROR/$ALPINE_VERSION/community" \
	--repository "$HOME/packages/ap"

echo ">>> images:"
ls -la /out
