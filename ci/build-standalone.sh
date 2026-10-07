#!/bin/sh
# Solitaire's console program as one file for a system that is not Alpymist:
# linked statically against musl, so it runs on whatever Linux the machine
# has, a Raspberry Pi's among them.
#
#   sh ci/build-standalone.sh OUT [TARGET...]
#
# Run in the rust:alpine container, at the workspace's root. With no TARGET
# it builds for the machine it runs on. Each file is written to OUT as
# alpymist-solitaire-console-ARCH, with its SHA-256 beside it.
#
# The packages are linked dynamically, as Alpine's are (ADR 0004); this is
# the one thing built the other way, and it is built apart from them, in a
# target directory of its own, so that neither finds the other's objects.
set -eu

OUT="${1:?usage: build-standalone.sh OUT [TARGET...]}"
shift
[ $# -gt 0 ] || set -- "$(rustc -vV | sed -n 's/^host: //p')"

mkdir -p "$OUT"
for target in "$@"; do
	arch="${target%%-*}"
	rustup target add "$target" >/dev/null 2>&1 || true
	# RUSTFLAGS from the environment replace .cargo/config.toml's, which ask
	# for the dynamic musl the packages use. rust-lld and the C runtime that
	# come with the target link it, so no cross compiler is needed.
	RUSTFLAGS="-C target-feature=+crt-static -C linker=rust-lld -C link-self-contained=yes" \
	CARGO_TARGET_DIR="${STANDALONE_TARGET_DIR:-target/standalone}" \
		cargo build --locked --release --target "$target" \
		--no-default-features --features drm -p alpymist-solitaire-console
	file="alpymist-solitaire-console-$arch"
	cp "${STANDALONE_TARGET_DIR:-target/standalone}/$target/release/alpymist-solitaire-console" "$OUT/$file"
	( cd "$OUT" && sha256sum "$file" > "$file.sha256" )
	echo "    $OUT/$file"
done
