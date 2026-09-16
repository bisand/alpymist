# ADR 0002 — Everything is a signed package

**Status:** accepted · **Date:** 2026-09-12

## Context

The most substantive technical criticism aimed at Omarchy-style projects is
about *delivery*, not taste:

- Installation by `curl … | bash`, so what you audit is not necessarily what
  you run, and there is no signature over the thing being executed.
- Long shell scripts run as root that mutate a system in place, with no record
  of what changed and no way to roll back.
- Third-party repositories and AUR packages pulled in without pinning or
  review, silently widening the trusted set.
- No reproducibility, so a compromised build host is undetectable.

None of that is inherent to a curated desktop. It is a choice about packaging.

## Decision

1. **No installer script. Ever.** There is no `curl | sh` path to an Alpymist
   system. You either boot a signed ISO, or you `apk add alpymist-desktop` from a
   signed repository on an existing Alpine box.
2. **Every artefact we ship is an apk**, signed with an offline key. Config
   files, themes, and wallpapers are packages, not `cp` commands.
3. **System mutation goes through `alpymistctl`**, in Rust, which writes to
   `/etc/alpymist` and generates backend config from it. Hand-edited files are
   detected, never silently overwritten.
4. **Reproducible builds are a release gate**, not an aspiration. CI builds
   each package twice, on different hosts, and diffs them.
5. **No third-party repositories by default.** Only Alpine `main`/`community`
   and our own. Adding anything else is an explicit, logged user action.
6. **Sandboxed by default** for third-party GUI apps: Flatpak with portals,
   not native installs.
7. **Rust code is `#![forbid(unsafe_code)]`**, `-D warnings` in CI, with
   `cargo-deny` and `cargo-audit` gating merges and an SBOM per release.

## Consequences

- Higher friction for us: adding a tool means writing an `APKBUILD`, not a line
  in a setup script. This is the point.
- Alpine's own hardening comes along for free: musl, PIE by default, stack
  protector, `RELRO`/`BIND_NOW`, `doas` instead of `sudo`.
- musl also means no glibc binaries. Steam, Chrome, and proprietary NVIDIA
  drivers do not run natively. Flatpak covers most of this; the rest is a
  deliberate audience choice, not an oversight.

## Addendum, 2026-09-14: how the offline key meets CI

CI builds packages; it cannot sign what systems trust. The Alpymist
repository's index is signed on a maintainer's machine by `cargo xtask
publish`, and apk verifies each package against the hash in that index, so a
compromised workflow can produce a bad artifact but not an update anyone
installs. Tested both ways before relying on it: a package signed by an
unknown build key installs from a trusted index, and a swapped or modified
package does not. See `ci/README.md`.

## Addendum, 2026-09-15: the dev channel

The dev channel is the one exception to the offline key: CI signs its index
with a key of its own, which only systems that opt in to dev trust. See
[ADR 0006](0006-release-channels.md).

## Addendum, 2026-09-15: `alpymist`

Decision 3 is carried out by `alpymist`, formerly `alpymistctl`, and the
`alpymist-settings` library behind it. See
[ADR 0007](0007-settings.md).

## Addendum, 2026-09-16: CI signs stable, and the offline key is retired

The offline key is gone from this decision. `alpymist-2026` is a secret of the
`stable-channel` GitHub environment, and `.github/workflows/publish-stable.yml`
signs and publishes the index on every successful Release run of a published
release. Nothing between a release and `pkgs.alpymist.org` is a person.

**This gives up what the 2026-09-14 addendum above promised.** A compromised
workflow can now sign an update that every Alpymist system accepts, and so can
anything that can reach into a workflow: a dependency of the build, an action
we call, or anyone who can push to main. The property that survives is
narrower — a bad package still has to pass through a Release run of a tagged
commit on main, so what is signed is what was on main and built in the open.
What no longer holds is that signing needs a person and a machine.

Why, anyway: releases had come to depend on one laptop being to hand, and a
release that cannot be cut is its own kind of failure. The trade was made
deliberately, with the alternative — an environment gated on a reviewer, which
keeps a person in the loop without keeping the machine — considered and turned
down.

What still stands from the original decision: packages are signed apks, systems
verify each one against a signed index, there is no `curl | sh`, and the dev
key still reaches only systems that ask for it. If the key is ever to go back
offline, the shape is here: remove the environment's secrets and publish with
`cargo xtask publish --run <id> --push` again, which is unchanged and still
works.
