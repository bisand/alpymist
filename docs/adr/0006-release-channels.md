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
