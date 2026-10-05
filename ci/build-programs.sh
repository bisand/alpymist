#!/bin/sh
# Builds every program of one group in a single cargo run, from the workspace
# this is run in. Each first-party APKBUILD's build() calls it for the group
# its programs are in; with ci/build-packages.sh's one workspace and one
# target directory, the first package of a group builds them all and the rest
# find it done.
#
# Why together: the release profile's full link-time optimisation has each
# program optimise everything it links, on about one core, and one cargo run
# for all of them links as many at once as there are cores. Nineteen runs one
# after another took 635 s on four cores; these two took 296 s (ADR 0006's
# addendum of 2026-10-04).
#
# What that costs: cargo gives a crate the features of every package it is
# built with here, not of one alone. So the groups are by what must not be
# mixed. The console programs run before there is a desktop and must not link
# its window backend, and are built without default features; the rest are
# built as they come.
#
# A package whose program is not named here has nothing to install and fails
# at package(): add it to its group.
set -eu

case "${1:-}" in
desktop)
	set -- \
		-p alpymist \
		-p alpymist-about \
		-p alpymist-ai-usage \
		-p alpymist-auth \
		-p alpymist-fingerprint \
		-p alpymist-lock \
		-p alpymist-menu \
		-p alpymist-power \
		-p alpymist-saver-mountains \
		-p alpymist-saver-starfield \
		-p alpymist-screensaver \
		-p alpymist-settings-app \
		-p alpymist-store \
		-p alpymist-thunderbolt \
		-p alpymist-wallpaper \
		-p alpymist-wifi
	;;
console)
	set -- --no-default-features --features drm \
		-p alpymist-greeter \
		-p alpymist-install \
		-p alpymist-splash
	;;
*)
	echo "usage: build-programs.sh desktop|console" >&2
	exit 2
	;;
esac

# Without the network first: an image built where there is none has the
# crates already. What that try says is kept back and shown only if it built,
# so a log is not left with an error from a build that then went well.
said=$(mktemp)
if cargo build --release --locked --offline "$@" 2>"$said"; then
	cat "$said" >&2
	rm -f "$said"
else
	rm -f "$said"
	cargo build --release --locked "$@"
fi
