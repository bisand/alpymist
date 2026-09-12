# ADR 0003 — Foundational choices

**Status:** accepted · **Date:** 2026-09-12

Four decisions taken at project start. Recorded together because each was a
genuine fork, and because the reasoning matters more than the outcome when we
revisit them.

## Binary compatibility: Flatpak, no `gcompat`

Alpine is musl. Steam, Chrome, VS Code, Discord and the proprietary NVIDIA
driver are glibc binaries and will not run natively.

We ship **Flatpak and `xdg-desktop-portal` by default** and nothing else. The
glibc runtime lives inside the sandbox, so proprietary software gets *better*
isolation than a native install would give it, and the base system stays purely
musl and auditable.

We explicitly reject `gcompat`: it is best-effort, it fails in ways that are
hard to diagnose, and a natively-installed proprietary binary has no sandbox.

Cost: the Flatpak runtime is a few hundred MiB, which is real on `Potato`-tier
machines. Those machines should use native Alpine packages; the tier system
already knows which machine it is on, so the installer can say so.

## Root filesystem: mutable

Normal, apk-managed, writable root. Familiar, simplest, and it gets us to a
bootable desktop fastest.

Immutable-root and Alpine diskless/`lbu` are both attractive — atomic upgrades,
rollback, tamper resistance — but each reshapes the installer, the upgrade path
and every package we write. Revisit after Phase 2, once the desktop layer is
proven and we know what an upgrade actually has to move.

## Architecture: `aarch64` locally, `x86_64` in CI

Development happens on Apple Silicon, so local builds and VM boots are native
`aarch64` and fast. CI builds and boot-tests `x86_64` on every PR.

`x86_64` is the *real* target — "really bad hardware" is overwhelmingly old
x86 laptops — so it must never be the arch that rots. Making it a required CI
gate from the first PR, rather than a later port, is what keeps that honest.

Known risk: `x86_64`-only bugs surface in CI rather than under our fingers. If
that becomes the dominant failure mode, move to emulated local `x86_64` boots
for the affected component.

## Licence: `MIT OR Apache-2.0`

The Rust ecosystem default. Maximum reuse, no contributor friction, and no
barrier to another distro adopting the tier system. Copyleft would buy a
community guarantee we do not currently need at the cost of that reuse.
