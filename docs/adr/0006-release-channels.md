# ADR 0006 — Upgrades come from the repository, on a stable or a dev channel

**Status:** accepted · **Date:** 2026-09-15 · **Amends:** [ADR 0002](0002-supply-chain.md)

## Context

Trying a change on a real machine meant building an ISO and installing it
again. Installed systems already had everything an upgrade needs, a signed
repository and `apk upgrade`. But packages were built only with a release,
alongside the ISOs, and every change had to wait for a hand-signed publish
behind a `pkgrel` bump.

We want two things: to upgrade an installed system to whatever is on main
without a new image, and to keep a channel that only moves at a release.

## Decision

1. **An ISO is for installing, not for upgrading.** It is built for a release.
   Every installed system upgrades with `apk upgrade`, from whichever channel
   it follows.
2. **Two channels, two repositories.**

   | | stable | dev |
   |---|---|---|
   | Repository | `https://pkgs.alpymist.org/v3.24/alpymist` | `https://dev.pkgs.alpymist.org/v3.24/alpymist` |
   | Built by | the Release workflow | the Dev workflow, on every push to main |
   | Index signed by | CI, key `alpymist-2026` | CI, key `alpymist-dev-2026` |
   | Site | `bisand/alpymist-packages` | `bisand/alpymist-packages-dev` |

3. **A channel is one line in `/etc/apk/repositories`.**
   `alpymistctl channel dev|stable` rewrites that line and upgrades. It is
   also in the menu under Update → Release channel. New systems follow stable.
4. **Dev versions itself.** `ci/build-packages.sh` with `CHANNEL=dev` appends
   `_git<UTC time of the last commit touching the package's sources>` to each
   first-party pkgver. apk orders `0.0.4 < 0.0.4_git20260915120301 < 0.0.5`,
   so dev moves ahead with each push and no bump, and meets the next release
   when it arrives. squint and `alpymist-keys` are versioned by hand on both
   channels. ([ADR 0008](0008-versions-and-build-numbers.md) later made the
   pkgver itself the release's, and the pkgrel the build's run number.)
5. **Leaving dev downgrades.** Dev's versions sort above stable's, so
   `alpymistctl channel stable` upgrades with `--available`, which takes the
   repository's versions even when they are older.

## Dev's key, and what CI can reach

apk trusts every key in `/etc/apk/keys` for every repository. If the dev key
were there on every system, anyone who could sign with it and serve an index
at the stable address would be trusted by all of them. So:

- `alpymist-keys` ships the dev public key to `/usr/share/alpymist/keys`, where
  apk does not look. `alpymistctl channel dev` copies it into `/etc/apk/keys`;
  `alpymistctl channel stable` removes it.
- The dev signing key and the deploy key for `bisand/alpymist-packages-dev`
  are secrets of the `dev-channel` GitHub environment, which only main may
  deploy to. That deploy key cannot push to the stable site.
- `cargo xtask publish` refuses to put stable in the hands of anything but a
  Release run, and refuses dev-versioned packages on stable.

This kept dev's blast radius to systems that chose dev. It no longer does:
since 2026-09-16 the release key is a secret of the `stable-channel`
environment and CI signs stable too, so a compromised workflow reaches every
Alpymist system, not only those following dev. The separation above still
holds within itself — dev's key and deploy key cannot touch the stable site,
and `cargo xtask publish` still refuses dev-versioned packages on stable — but
it is no longer the boundary it was written to be. See ADR 0002's addendum of
that date. Anyone following either channel is trusting GitHub Actions, and
should know that.

## Consequences

- A change reaches a dev machine within one build, with `doas apk upgrade -U`
  or the menu's Update.
- Stable needs a new version per build, which
  [ADR 0008](0008-versions-and-build-numbers.md) now gives it automatically;
  `cargo xtask publish` still enforces that it got one.
- Dev rebuilds and re-downloads every Rust package whenever the workspace
  changes, because each one copies the whole workspace. They are small.
- The `v3.24` in both addresses ties each channel to an Alpine release. Moving
  Alpine releases is a new address for both, as it was before.
- Rotating the dev key follows the stable key's procedure: ship the new public
  key in `alpymist-keys`, with `alpymistctl` copying it on the next switch, before
  signing with it.

## Addendum — 2026-10-01: dev is built without link-time optimisation

Dev was built exactly as stable is, with the release profile's full LTO. That
profile has every program optimise all of its dependencies again as it links,
and with twenty programs it was most of the twelve minutes between a merge and
a package. Measured on one machine, twelve of the packages took 508 s with it
and 233 s without.

So the dev channel now builds with LTO off and sixteen codegen units, and
stable is unchanged. What that gives up: a dev package is no longer byte for
byte what the next release ships. Its programs are about two thirds bigger,
and a fault that only full LTO brings out would first be seen in a Release
build, not on dev. Release still builds the full profile, and its smoke test
boots an image made of those packages, which is where such a fault would show
before anything is published to stable.

## Addendum — 2026-10-04: dev is built as stable is again, a group of programs at a time

The addendum above traded what dev tests for time: its programs were not the
ones the next release ships. The time was the links. Full LTO has each program
optimise everything it links, on about one core, and the packages were built
one after another, so nineteen cargo runs linked twenty-three programs one at
a time on a runner with four cores.

Now `ci/build-programs.sh` builds the programs in two cargo runs, and each
package's `build()` calls it: the first package of a group builds every
program in it, linking as many at once as there are cores, and the others find
theirs built. Measured on one machine with four jobs, stable's profile
throughout:

| | time | the programs |
| --- | --- | --- |
| one package at a time, as it was | 635 s | 28.86 MB |
| two runs, a group each | 296 s | 28.80 MB |
| one at a time without LTO, as dev was | 246 s | 43.1 MB |

Thin LTO was measured too and is not the answer: 459 s one at a time, and
programs 24% bigger.

So both channels build the release profile as it is, and the first addendum's
decision is undone: a dev package is again the program the release ships, and
a fault that only full LTO brings out is seen on dev first.

What that gives up is what `ci/README.md` used to promise, that a package
builds only its own crates with their own features. Cargo gives a crate the
features of every package it is built beside, so within a group they are
shared: `alpymist` and Settings link the Wi-Fi, power, AI usage and
screensaver crates with their popups compiled in, where alone they would not.
The optimiser drops what a program does not call — twenty of the programs came
out the same size to the byte, and none more than 4 kB bigger — but they are
not the same bytes, and a crate that only compiles because a neighbour turns a
feature on is no longer caught by the package build. The groups are by what
must not be shared: the programs that run with no desktop (the greeter, the
installer, the splash) are built without default features and never beside
the rest, so the window backend cannot reach them this way.
