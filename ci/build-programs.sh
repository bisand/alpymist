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
#
# `build-programs.sh alone` builds nothing: it has cargo check each package by
# itself, with its own features and no neighbour's, which is what building
# them a package at a time used to prove. A crate that only compiles with a
# feature another package turns on passes a build of the group and fails
# there. CI runs it.
set -eu

DESKTOP="
	alpymist
	alpymist-about
	alpymist-ai-usage
	alpymist-auth
	alpymist-fingerprint
	alpymist-lock
	alpymist-menu
	alpymist-overview
	alpymist-power
	alpymist-saver-mountains
	alpymist-saver-starfield
	alpymist-screensaver
	alpymist-settings-app
	alpymist-solitaire
	alpymist-store
	alpymist-thunderbolt
	alpymist-wallpaper
	alpymist-wifi
"
CONSOLE="
	alpymist-greeter
	alpymist-install
	alpymist-solitaire-console
	alpymist-splash
"
# How the console programs are built: without the desktop's window backend.
BARE="--no-default-features --features drm"

if [ "${1:-}" = alone ]; then
	for p in $DESKTOP; do
		echo "    $p, alone"
		cargo check --locked -p "$p"
	done
	for p in $CONSOLE; do
		echo "    $p, alone"
		# shellcheck disable=SC2086 # two words, meant as two
		cargo check --locked $BARE -p "$p"
	done
	exit 0
fi

case "${1:-}" in
desktop)
	set --
	for p in $DESKTOP; do set -- "$@" -p "$p"; done
	;;
console)
	# shellcheck disable=SC2086 # two words, meant as two
	set -- $BARE
	for p in $CONSOLE; do set -- "$@" -p "$p"; done
	;;
*)
	echo "usage: build-programs.sh desktop|console|alone" >&2
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
