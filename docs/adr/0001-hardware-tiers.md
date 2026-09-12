# ADR 0001 — One desktop, three renderers

**Status:** accepted · **Date:** 2026-09-12

## Context

Two requirements pull in opposite directions:

1. Use Hyprland and Wayland "where applicable".
2. Run on *really* bad hardware.

Hyprland hard-requires OpenGL ES 3.2 and a working accelerated DRM driver. On a
2009 netbook with GMA 950, on a machine that only gets `simpledrm`, or in a VM
without virtio-gpu acceleration, Hyprland will not start. There is no
configuration that fixes this.

wlroots — which labwc is built on — ships a `pixman` renderer that composites
entirely on the CPU (`WLR_RENDERER=pixman`). labwc is also a *stacking*
compositor, which is the "lightweight, X11-like desktop" half of the brief.

## Decision

Alpymist defines the desktop **once** — keybindings, theme, panel, launcher, lock,
idle — and renders it onto whichever backend the machine can drive:

| Tier     | Backend                    | Chosen when                              |
|----------|----------------------------|------------------------------------------|
| `Full`   | Hyprland                   | accelerated DRM + GL ES ≥ 3.2 + ≥ 3 GiB  |
| `Lite`   | labwc (GLES2)              | accelerated DRM + GL ES ≥ 2.0 + ≥ 1.5 GiB|
| `Potato` | labwc (`WLR_RENDERER=pixman`) | KMS but no usable GPU rendering       |
| `Legacy` | X11 + i3                   | no DRM/KMS device at all                 |

Selection is a pure function, `alpymist_core::select_tier`, run at install time and
again on first boot. It is **pessimistic**: anything unknown downgrades. A
desktop that starts slowly is recoverable; one that does not start is not.

Every decision carries a `Rationale` explaining itself, shown to the user and
written to the install log.

## Consequences

- We own a config *translation layer*, not three separate desktop configs.
  That layer is the main body of Rust we write, and the main thing to test.
- Feature parity is not total: Hyprland's animations and blur have no labwc
  equivalent. Tier-specific features must degrade, never break.
- The X11 tier is a genuine fallback, not a co-equal desktop. It exists so that
  hardware wlroots cannot drive still boots to a usable graphical session.
- We must build a small EGL probe to fill in `gles_version`. Until it exists
  every machine reads as `Potato`, which is the safe direction to be wrong in.
