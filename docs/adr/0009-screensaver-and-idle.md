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

---

## Addendum, 2026-09-18 — a screensaver is a program, not a picture in ours

**Status:** accepted · amends §3 above and the shape of §1.

### What changed

The decision above described one screensaver, built into
`alpymist-screensaver`, with the account's chunkiness in the same file as the
idle policy. Asked for more screensavers, the obvious next step was a second
module and an enum variant. That was declined, and deliberately:

> I think of screensavers as individual programs, so then they should also
> provide a settings page that individually can be configured. The settings
> page could be definitions that each screensaver defines that makes the
> settings page render it from that definition.

So a screensaver is now a **program plus a file that describes it**:

```
/usr/bin/alpymist-saver-mountains
/usr/share/alpymist/screensavers/mountains.toml
```

The file says what it is called, what to run, and every setting it takes —
each with a kind, a range, a default and a sentence of its own. Settings reads
that directory, renders a page per screensaver from the declarations, and
writes the values to `~/.config/alpymist/screensavers/<id>.toml`. The program
reads that same pair. **Nothing in Alpymist knows what a "mountain" is.**
Installing a package that ships those two files adds a page and an entry in the
`Screensaver` list; removing it takes both away.

`alpymist-screensaver` keeps the idle watch and becomes a launcher: it resolves
`show` — a name, or `random` — against what is installed and `exec`s it, so
what swayidle started and what `alpymist-screensaver stop` takes away are one
process.

### What this costs, and what was kept

The obvious cost of separate programs is that each opens its own Wayland
surface and does its own scaling, and the care in §3 above would have to be
repeated in every one. That is not how this is built. `alpymist-screensaver` is
a **library** as well as a launcher: a screensaver implements `Painting` — draw
a small picture, say how small — and `paint::start` provides the layer surface,
the block magnification, the frame pacing, the input that dismisses it and the
socket `stop` reaches. A whole screensaver is a `main` of a dozen lines and its
picture. The 8% of a core measured above is a property of the shared host, so
every screensaver gets it.

What it buys is that a third party can add one without Alpymist being rebuilt,
and that a screensaver's settings live with the screensaver rather than in a
registry somebody has to remember to extend.

### Two things that follow

- **`Kind::Action`.** Settings gained a control that is a thing to do rather
  than a thing to be: a button, whose `set` runs it. Every screensaver's page
  gets one — *See it now* — whether or not it declares anything else, because
  looking at a screensaver is the one thing everybody wants from its page and
  a timeout of five minutes is a poor way to do it. `alpymist set
  screensaver-mountains.preview` does the same from a terminal.
- **Settings' registry is no longer entirely static.** An `Area` and a
  `Setting` hold `&'static str` because every one of them used to be written in
  the source. The screensavers' are read from files, once per process, and kept
  for as long as it runs. That is a deliberate small leak, bounded by the number
  of screensavers installed, and it is the reason `Settings::areas()` can still
  hand back `&'static [Area]` to everything that already expected one.

### What this does not reach

The menu's settings fragment is generated when `alpymist-settings` is packaged
(`alpymist menu-fragment`, ADR 0007 §6), in a container where no screensaver is
installed — so the menu offers the `Screensaver` page and its four policy
settings, and not the per-screensaver pages. Nothing wrong is written; the list
is simply the static half. Settings itself reads the directory every time it
starts and is complete. Closing the gap would mean either generating the
fragment on install, which a screensaver added later would still miss, or
having every screensaver ship a menu fragment as well as a definition — a
second file saying what the first already says. Neither is worth it for a
search result, and this is written down so the next person does not assume it
was overlooked.

### What 0.0.7 shipped, and what happens to it

0.0.7 wrote `block` into `screensaver.toml`. That key now belongs to the
mountains, in their own file. The policy file still *accepts* `block` and
ignores it: `deny_unknown_fields` would otherwise refuse the whole file, and a
refused file means a laptop whose screen stops turning off — over a key that no
longer matters. The default is the same either way, so nobody who never changed
it can tell.
