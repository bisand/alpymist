# ADR 0009 — The screensaver is a picture, and the lock is somewhere else

**Status:** accepted · **Date:** 2026-09-18

## Context

Alpymist had no idle behaviour at all. No tier's configuration started
`swayidle`, though the Wayland tiers installed it; nothing turned a screen off;
nothing locked one. A laptop left alone on the login desk kept its backlight on
until the battery was flat. `Super+L` ran `swaylock`, and that was the whole of
it.

Wayland has no screensaver framework to reach for. The X server had one — the
screen saver extension, `xset s`, a fullscreen override-redirect window — and
nothing replaced it, because the job turned out to be three unrelated ones:

1. **Noticing that a seat has gone still.** `ext-idle-notify-v1`, which
   Hyprland and labwc both implement and `swayidle` speaks.
2. **Covering the screen.** Either an ordinary client on the `overlay` layer of
   `wlr-layer-shell` — which is what the menu and every Alpymist popup already
   are — or an `ext-session-lock-v1` client, which the compositor keeps on
   screen even if the client dies.
3. **Turning the panel off.** `wlr-output-power-management`, spoken by `wlopm`.

Only the second of these has anything to do with a picture, and the third is
the one that actually saves a battery. The phosphor a screensaver was invented
to save has not existed on this hardware for twenty years.

## Decision

### 1. The screensaver is an overlay surface, and it is not a lock

`alpymist-screensaver` is a layer surface on the overlay layer, drawn with
Denise like every other Alpymist window, that goes away at the first key or
movement. It is the same `alpymist-widget` host the Wi-Fi and power popups use,
with a `Placement::FullScreen` added to it.

It is deliberately **not** an `ext-session-lock-v1` client, and it deliberately
**never asks for anything**. Locking stays `swaylock`, run separately at its own
timeout and by `Super+L`, as before.

That division is worth more than the code it saves. A full-screen surface that
holds the keyboard and asks for a password is exactly what a lookalike prompt
would be, and ADR 0003's answer to lookalikes — Ctrl+Alt+Delete, which asks
Hyprland who drew what is on screen — works because Alpymist's real password
prompts are few and known. A screensaver that sometimes asked for a password
would make "the screensaver is asking for your password" a normal thing to see.
It is not one: if the mountains ask you for anything, something is wrong.

The cost is honest and should be written down: an overlay surface is not a
guard. A compositor crash, or the process being killed, uncovers the screen. It
is a picture in front of an unlocked session, which is what `screensaver.lock`
and `swaylock` behind it are for.

### 2. `swayidle` decides when, and `alpymist-screensaver idle` decides what

Rather than a timeout written into each tier's compositor configuration, the
watch is one command in both, and it reads the account's
`~/.config/alpymist/screensaver.toml`:

```
timeout 300 alpymist-screensaver resume 'alpymist-screensaver stop'
timeout 600 "wlopm --off '*'" resume "wlopm --on '*'"
```

`alpymist-screensaver idle` builds that list and keeps a `swayidle` running with
it. Running it a second time — which is what Settings does after a change —
does not start a second watch: it reaches the first over a socket in
`XDG_RUNTIME_DIR` and has it start over with the new file. So every screensaver
setting is `Applies::Now` (ADR 0007) rather than `next-login`, which for a
timeout is the difference between a setting and a note to self.

`swayidle` rather than speaking `ext-idle-notify-v1` ourselves: it is packaged,
it is already installed on both Wayland tiers, and what it adds over the
protocol — `before-sleep`, and a process per command — is what we would have
written. If it ever needs replacing, `idle::arguments` is the only thing that
knows its spelling.

`wlopm` rather than `hyprctl dispatch dpms`: it speaks the wlroots protocol, so
one command covers Hyprland and labwc instead of one each.

### 3. The picture is the wallpaper's, drawn small

The scene is `alpymist_ui::backdrop` at the same seed as the wallpaper, the
splash and the installer, composed at a fraction of the screen's resolution and
blown up in square blocks. The pixelation is not an effect applied to a picture;
it is the picture, drawn at 1/6 the size. A 1920×1080 screen is 320×180 pixels
of scene, and the expansion is one memory copy per row.

What moves is the mist, and only the mist: sky and ridges are painted once per
screen size and never again, and each frame copies that and lays patchy,
sideways-drifting bands over it. At twelve frames a second this is a few hundred
thousand pixels of work for a screen of two million.

That mattered enough to design around. The machines Alpymist exists for are the
reason the Potato tier exists, and a screensaver is by definition the only thing
running: one that keeps a core busy drains the battery it was there to idle
through. The setting that turns the *screen* off after ten minutes saves far
more power than any amount of care here, which is why it is on by default and
why the picture is an overture to it rather than an alternative.

## Consequences

- The Wayland tiers gain an idle policy they did not have: mountains at five
  minutes, screen off at ten, and a lock only if asked for. The Legacy tier
  gains nothing — X11 has `xautolock` and `i3lock` and none of this — and is
  left as it was.
- `alpymist-screensaver` brings `swayidle` and `wlopm`; `swayidle` comes off
  `alpymist-desktop-wayland`'s list, where it was installed and unused.
- `alpymist-widget`'s `Widget` gains `frame_interval`, so an animation can ask
  for fewer frames than a spinner wants, and `Placement::FullScreen`.
- Locking by default was considered and declined. A machine that starts asking
  for a password because it was upgraded is a machine people turn the feature
  off on, and the switch is one line in Settings for anyone who wants it.
- If a lock screen of Alpymist's own is ever wanted, the drawing here is what it
  would show, and `ext-session-lock-v1` is where it would go. Nothing here
  forecloses that; it is a separate decision, and this one does not pretend to
  have made it.
