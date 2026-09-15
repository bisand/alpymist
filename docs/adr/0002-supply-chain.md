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
