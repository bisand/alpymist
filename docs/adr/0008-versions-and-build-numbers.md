# ADR 0008 — One version for everything, and the run number as the build number

**Status:** accepted · **Date:** 2026-09-16 · **Amends:** [ADR 0006](0006-release-channels.md)

## Context

Every first-party package carried `pkgver=0.0.1` and a `pkgrel` of its own,
bumped by hand whenever that package changed. Three releases later the
repository said 0.0.1 and GitHub said v0.0.3, and the packages ranged from
`0.0.1-r0` to `0.0.1-r15`. Nothing in the version told you which release a
package belonged to, and `alpymist-about`, which reads those versions off the
installed system, could only report what it was given.

The hand-bumped `pkgrel` was also a footgun. `cargo xtask publish` keeps the
already-published file for any version it has published before (ADR 0002 —
builds are not reproducible, so the same version rebuilt is a different file
and replacing it would break every cached index). A forgotten bump therefore
did not fail loudly at build time; it silently shipped nothing, until publish
caught the clash and refused the whole release.

## Decision

1. **One version, written down once.** `Cargo.toml`'s `[workspace.package]
   version` is it. Every `aports/*/APKBUILD` repeats it as `pkgver` because
   abuild sources the APKBUILD as shell and has nowhere else to read it from,
   and `cargo xtask version` keeps the copies honest:

   | | |
   |---|---|
   | `cargo xtask version` | fails unless every `pkgver` is the workspace version. CI runs this. |
   | `cargo xtask version 0.0.5` | writes that version everywhere and starts `pkgrel` again at 0. |
   | `cargo xtask version --expect v0.0.5` | also fails unless that is the tag being released. The Release workflow runs this before it builds anything. |

   The list of packages is `aports/` itself, less the two versioned by hand, so
   a new aport is covered the day it is added.

2. **`pkgrel` is the build number, and nobody types it.**
   `ci/build-packages.sh` with `BUILD=<n>` sets every first-party `pkgrel` to
   `n`, and the workflows pass `github.run_number`. Run 8 builds `0.0.4-r8`;
   run 9 of the same commit builds `0.0.4-r9`. Reusable workflows share the
   caller's run context, so stable counts Release's runs and dev counts Dev's,
   each on its own. Unset — a local `make iso` — the committed `pkgrel=0`
   stands.

3. **The tag is the version.** A release published as `v0.0.4` is built from a
   commit that says 0.0.4, or the Release workflow stops before building.

4. **squint and `alpymist-keys` keep their own versions.** They are built from
   a pinned upstream release and from a key file, not from this workspace, so
   neither the dev stamp nor the build number touches them — squint because its
   build is cached across runs on exactly its own inputs, `alpymist-keys`
   because its version is the key year. `ci/build-packages.sh` and
   `cargo xtask version` hold the same two names. squint's `pkgver` is the
   version of the squint release it is built from, and its committed `sha512`
   is the pin: `build-packages.sh` does not run `abuild checksum` over it,
   which would replace the pin with whatever the download happened to be.

## Consequences

- A package's version now says which release it belongs to, on both channels:
  stable `0.0.4-r8`, dev `0.0.4_git20260916144307-r10`. Both still order the
  way ADR 0006 needs, because only dev carries `_git` and every comparison
  between the channels is decided before `pkgrel` is reached.
- No first-party `pkgrel` is bumped by hand any more, and no build can land on
  a version already published. `cargo xtask publish`'s rebuilt-in-place check
  stays as the backstop it was, but it should now never fire.
- Every run produces a new version for every package, so every publish ships
  every package and every system downloads them all. That was nearly true
  before — each package's dev stamp covers `crates/`, so any change to the
  workspace restamped all of them — and the packages are small.
- Releasing is: `cargo xtask version <next>`, `cargo update --workspace`,
  commit, tag, publish the release.
- The run number is not stable across a workflow being renamed or recreated,
  which would reset the counter. If that happens, bump `pkgver` in the same
  change and the reset cannot be observed.
