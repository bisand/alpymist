# ADR 0005 — Alpine 3.24 as the base, and why stable is now enough

**Status:** accepted · **Date:** 2026-09-12 · **Amends:** [ADR 0003](0003-foundational-choices.md)

## Context

Two constraints had to meet. DeniseUI 0.24 — the toolkit the splash and
installer are built on — requires **rustc 1.95**. And ADR 0003 chose to base
Alpymist on Alpine *stable* rather than edge.

On Alpine 3.22 those were in conflict:

| | Alpine 3.22 | Alpine 3.24 | edge |
|---|---|---|---|
| rust | 1.87 ❌ | **1.96** ✅ | 1.97 |
| hyprland | 0.49.0 | **0.54.3** | 0.54.3 |

3.22 could not build the installer at all, and its Hyprland was several
releases behind. That is what motivated the earlier plan to carry newer
desktop packages in our own repository.

## Decision

**Base on Alpine 3.24.** The build container and the image are the same Alpine
version, so everything we compile links against the same musl it will run on —
which is why the toolchain is not simply taken from edge instead.

Rust comes from `apk`, not `rustup`. A build container assembled entirely from
signed Alpine packages is one less thing to trust, which is the whole argument
of [ADR 0002](0002-supply-chain.md).

`rust-toolchain.toml` pins 1.96 to match, so a local build and a package build
use the same compiler.

## Consequences

- **The "stable is stale" worry does not apply to 3.24.** Its Hyprland is the
  same version edge has. Basing on stable currently costs nothing in currency
  while keeping two years of security support.
- **We still owe our own `APKBUILD`s** for `hyprlock`, `hypridle`,
  `hyprpicker`, `hyprpaper` and `xdg-desktop-portal-hyprland`. Those live in
  edge's `testing`, which has no stable-branch equivalent, so the packaging
  burden estimated earlier is unchanged.
- **This will recur.** Denise is ours and moves quickly; each time its minimum
  Rust rises past what Alpine stable ships, we face this choice again. The
  answer is not automatically "bump the base" — if it happens mid-release it
  may be cheaper to hold Denise's minimum down for one cycle.
