# ADR 0015 — Screens: a layout for each set of screens, applied through Hyprland

**Status:** accepted · **Date:** 2026-09-29

## Context

Every screen was placed by one line in `hyprland.conf`, `monitor = , preferred,
auto, 1`: left to right, in whatever order Hyprland found them. Nothing
remembered a layout. A laptop moved between a dock at the desk, a projector and
nothing at all came up however Hyprland happened to find its screens, and a
closed lid on the dock left the built-in panel on behind it, with workspaces
and windows on a screen nobody could see (#17).

Alpine packages `kanshi`, which applies a profile for each set of screens, and
`wlr-randr`, which changes one screen once. Hyprland also has monitor rules of
its own, and can match a screen by its EDID's make, model and serial
(`monitor = desc:…`), not only by connector.

## Decision

**Hyprland arranges the screens; Alpymist only decides what to ask of it.**
kanshi changes screens through `wlr-output-management`, beside Hyprland's own
configuration rather than in it, so every `hyprctl reload` puts Hyprland's
rules back over kanshi's — and Settings reloads Hyprland often. So the layout
is written as Hyprland's own monitor rules, into
`~/.config/alpymist/hypr/displays.conf`, which the packaged configuration
sources after `hyprland.conf`'s catch-all line, and handed to the running
Hyprland with `hyprctl keyword monitor`. A reload re-reads the same layout.

**A layout belongs to a set of screens.** The key is the set of screens
connected, each named by its description: make, model and serial. `DP-3`
today can be `DP-5` after a reboot or on another port of the dock, and its
layout follows it. Two screens the description cannot tell apart — the same
model with no serial — are named by description and connector, and that one
case follows the port. A description a monitor rule cannot hold, with a comma
or a semicolon, is too.

**A set never seen together is extended, then remembered.** The laptop's
panel on the left, the others after it along the top in the order Hyprland
found them, each in the mode it came up in. That is kept for the set at once,
so it is what the set gets next time, until it is changed.

**A closed lid turns the laptop's panel off, but only with another screen
on.** `alpymist-power lid` runs `alpymist displays apply` when docked, and
Hyprland's lid-open switch runs it too; the lid's state is read from ACPI. The
panel is off in the rules, not in the layout, so opening the lid puts it back
where it was. A closed laptop alone keeps its panel: it is the only screen,
and whatever the lid is set to do — lock, sleep — does the rest.

**`alpymist displays` is the one way in**, for scripts now and for the
Displays page of Settings later: `list`, `set`, `save`, `apply`, `forget`.
`alpymist displays watch`, started by the packaged Hyprland configuration,
follows Hyprland's event socket, waits for a dock's screens to finish
arriving, and applies the layout only when the set of screens has changed —
turning the panel off is itself a screen going, and must not start it all
again.

Layouts are the account's, in `~/.config/alpymist/displays.toml`. A file that
does not parse is treated as empty and never written over.

## Consequences

- Docking, undocking and plugging into a projector put the screens back as
  they were for that set, with nothing to do.
- A hand edit of `displays.conf` lasts until the screens next change; the
  layouts file, or `alpymist displays set`, is where a change stays.
- A monitor rule in the user's own `hyprland.conf` that names a screen still
  applies where ours does not, but where both name the same screen, the
  sourced file's comes later and wins.
- Not yet (the rest of #17): the Displays page, with a "keep these settings?"
  revert; windows and workspaces following a screen as it goes and comes back;
  mirroring; and every Alpymist surface — the bar, the wallpaper, the lock and
  the screensaver, the greeter, the popups — checked on more than one screen.

## Addendum — 2026-09-29: the Displays page

Settings has a Displays page, the second part of #17. The screens are drawn
to scale and dragged, or moved with the arrow keys, and are put against the
nearest edge of another as they land (`alpymist_displays::arrange`), so a
layout made there has no gaps and no overlaps. Each screen's resolution,
scale, rotation and whether it is on are chosen below it, and Identify puts
each screen's number on it, from a small window of Settings' own that takes
no keyboard.

Nothing changes until Apply. Then the layout is kept for the set of screens
and put in place, and the page asks whether to keep it; with no answer in 15
seconds it puts the one before back. A resolution a screen cannot show must
not leave anyone without a picture to undo it with.

What a closed lid does is a setting on that page, `displays.lid`, kept in the
same file as the layouts (`lid-turns-panel-off`), and on by default: while
another screen is on, closing the lid turns the laptop's panel off.

## Addendum — 2026-09-29: screens stay where they are dropped

The first Displays page lined a dropped screen up with its neighbour's edge
whenever it could, which on the X1's desk made a grid of it: a screen that
sits a little lower than the laptop, or off to one side, could not be shown
that way. A dropped screen now keeps the place it was dropped in along the
edge it lands against, and is lined up only when it was dropped within 32
layout pixels of lined up. Shift and the arrow keys slide the chosen screen
along that edge ten pixels at a time. It still always touches another and
never covers one.
