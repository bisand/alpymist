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

---

## Addendum, 2026-09-27 — one desktop: Hyprland

**Status:** accepted · reverses the decision above.

Alpymist is one desktop, Hyprland on Wayland. The Lite and Potato tiers
(labwc) and the Legacy tier (i3 on X11) are gone, with their packages, their
greetd configurations, their skeleton files and every code path that asked
which desktop was running (#40).

**Why.** Three desktops were three sets of configuration, three paths through
the session, the wallpaper, the keyboard, idle and the lock screen, and three
things to test for every change. The "translation layer" the consequences
above foresaw was never written; each tier was configured by hand, and each
new setting had to say which of them it did not reach. One desktop keeps the
work, and the attention, on the desktop Alpymist actually is.

**What the evidence said.** The probe was more cautious than Hyprland is. The
dev VM runs Hyprland on llvmpipe although the probe called it Potato, and the
Asus E200HA, an Atom with 2 GiB, runs it although the probe's memory threshold
said Lite. Much of what Lite and Potato were for runs Hyprland, only slower.

**What it gives up.** A machine with no usable KMS at all, which is what
Legacy was for, cannot run Alpymist any more. Nor, probably, can one whose
only display is a firmware framebuffer, or whose GPU driver stops short of
GL ES 3.2. Those were the machines the first requirement above named, and this
drops them.

**The probe** stays, with a different question: not which desktop suits the
machine, but how Hyprland will do on it. `alpymist_core::hyprland::check`
answers *Runs*, *Slow* (a software renderer, or under 3 GiB) or *Unlikely* (no
KMS, a firmware framebuffer only, no EGL, or GL ES below 3.2), with its
reasons, as `select_tier` did. Nothing refuses on it: the installer shows the
answer and warns on *Unlikely*, and the person may install anyway. It stays
pessimistic, for the reason given above, but a doubt is now said rather than
acted on.

**Packages.** `alpymist-desktop` is the whole desktop. It provides and replaces
`alpymist-desktop-wayland` and `alpymist-desktop-full`, so an upgrade of a
Hyprland install moves to it with its world as it was. It deliberately does
not provide `-lite` or `-legacy`: those installs are left where they are,
held at their last release by their own exact-version dependencies, while
everything else on them still upgrades. Tried with apk 3.0.8 against local
repositories: a Full install purged `-full` and `-wayland` and upgraded; a
Lite install kept its desktop at the old version and upgraded the rest, and
both exited 0. greetd's configuration is `alpymist.toml`, and the package's
post-upgrade script moves `/etc/conf.d/greetd` on from `alpymist-full.toml`.

The other ADRs mention tiers where they were written when there were tiers,
and are left as the record of what was decided then.
