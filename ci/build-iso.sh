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

# With PREBUILT naming a directory of this architecture's packages, as the
# Release workflow does, they are used as they are and nothing is built again.
echo ">>> [1/3] Alpymist packages"
bash /src/ci/build-packages.sh

echo ">>> [2/3] fetching Alpine image scripts ($ALPINE_BRANCH)"
# GitHub's mirror first: Alpine's GitLab challenges clients it takes for bots,
# which includes CI runners, and every CI build failed here until this changed.
# Errors are left visible — this step failed silently for weeks.
if [ ! -d /tmp/aports ]; then
	for url in https://github.com/alpinelinux/aports.git \
		https://gitlab.alpinelinux.org/alpine/aports.git; do
		echo "    cloning $url"
		if git clone --quiet --filter=blob:none --no-checkout --depth 1 \
			-b "$ALPINE_BRANCH" "$url" /tmp/aports; then
			break
		fi
		rm -rf /tmp/aports
	done
	[ -d /tmp/aports ] || { echo "could not fetch aports from any mirror" >&2; exit 1; }
	git -C /tmp/aports sparse-checkout init --cone
	git -C /tmp/aports sparse-checkout set scripts
	git -C /tmp/aports checkout --quiet
fi
cp /src/profiles/mkimg.alpymist.sh /src/profiles/genapkovl-alpymist.sh /tmp/aports/scripts/
chmod +x /tmp/aports/scripts/genapkovl-alpymist.sh

echo ">>> [3/3] building the image for $ARCH"
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

# A checksum beside each image, named as sha256sum -c expects to find it.
(
	cd /out
	for iso in *.iso; do
		sha256sum "$iso" > "$iso.sha256"
	done
)

echo ">>> images:"
ls -la /out
