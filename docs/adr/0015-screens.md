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

## Addendum — 2026-09-30: each screen its own workspaces

Hyprland has one set of workspaces for every screen, and Super+4 goes to
workspace 4 wherever it is, taking the pointer to another screen with it.
On the X1's dock that made workspaces wander: closing the lid moved the
laptop's workspaces to a Samsung but left it showing its own, and Super+4
jumped the pointer across the desk. Hyprland's answer, workspace rules
binding numbers to monitors, is known not to hold when monitors come back
(hyprwm/Hyprland#9580, closed as not planned). Hyprland's users, and
Omarchy's plugins, have settled on what macOS does: each screen its own set.

So each screen has its own 1 to 9, on by default and a switch in Settings ›
Displays. The laptop's own screen keeps Hyprland's workspaces 1 to 9, so a
laptop on its own is unchanged; each other screen has a block of ten, 11 to
19, 21 to 29, named 1 to 9 so the bar shows those, and the block belongs to
the screen by make, model and serial, kept in `displays.toml`. Super+1 to
Super+9 run `alpymist displays workspace N`, which asks Hyprland's socket
which screen has the focus; Super+Shift takes the window. The first five of
each screen are kept by workspace rules in `displays.conf`, so the bar,
which now shows each screen's own, always has them.

Nothing is remembered about where a workspace was: its number says whose it
is. When a screen goes — unplugged, or the laptop's behind a closed lid —
Hyprland moves its workspaces to the screens left, and the one the laptop
showed is brought into view. When it comes back, each workspace is sent
home, and a screen left showing one Hyprland made up shows its own first.

Hyprland plugins stay denied, and the Lua packages that do the same need
Hyprland 0.55, which Alpine does not have; this uses only Hyprland's own
dispatchers and rules. An account made before has Hyprland's own Super+number
binds; changing either Displays setting takes them over, keeping the file as
it was beside it (ADR 0007).

## Addendum — 2026-09-30: workspaces keep their numbers; the login screen follows the lid

Naming every screen's workspaces 1 to 9 was a mistake: the bar tells
workspaces apart by name, and on the X1 each Samsung's bar showed both
Samsungs' five, 1234512345, with the highlight on every "2" at once. The
workspaces keep their numbers as names now, 11 to 15 and 21 to 25, and the
bar shows them by their last digit; each bar marks the workspace its own
screen shows, and underlines the one with the focus. Workspace rules name
screens by connector, which is how the bar knows them, and are written again
whenever the screens change. A session that has the earlier names gets the
numbers back the next time the layout is applied.

The splash, the installer and the login screen draw straight to the display,
before Hyprland, and chose the laptop's own screen whenever one was
connected — which a closed lid still is. On a dock with the lid shut the
login box was behind the lid and the dock's screens were dark. They now
prefer a connected external screen while the lid is closed
(`alpymist_ui::lid`).

## Addendum — 2026-09-30: the screensaver and the lock screen when screens come and go

The screensaver on every screen started one copy per screen it found at the
start, and took them all away when any one ended: unplugging a screen, or
closing the lid, ended the screensaver everywhere, and a screen plugged in
while it was up showed the desktop. The widget host now says when the
compositor took its surface away with its output (`host::LOST`); a copy
whose screen went ends quietly and the others carry on, and the launcher
follows Hyprland's event socket and starts a copy for each screen plugged in
while it is up. On one screen, the picture moves to the screen that is then
the main one when its own goes, the covers on the others already dark.

The lock screen draws the same screen on every output from one shared state
sized for one output. Outputs of different sizes composed the scenery again
for each on every keystroke, and a click was tested against the layout of
whichever was drawn last. Sceneries are kept for each size now, and a click
is tested with the layout of the screen it was on.

## Addendum — 2026-09-30: mirroring

A screen can show the same as another instead of a place of its own — a
projector showing the laptop's screen. It is part of the layout for that set
of screens: `mirror` names the screen it shows, by make, model and serial,
and the rule says `mirror, <connector>`. A mirror has no place in the
arrangement and no workspaces of its own. Mirroring a screen that is off —
the laptop's, behind a closed lid — or one that is itself a mirror, it shows
a picture of its own instead, so a screen is never left showing nothing.
Hyprland reports what a screen mirrors by the other's monitor number, not
its connector, and the check that a layout took reads it that way.

## Addendum — 2026-10-02: an overview of the workspaces, and the one plugin let in

With nine workspaces on each of three screens there was no way to see where
a window had been left but to go through them. Hyprland has no overview of
its own; the one its authors maintain is a plugin, hyprexpo, which draws a
screen's workspaces side by side on it and goes to the one clicked. Alpine
packages it, built against the Hyprland beside it in the same repository.

The addendum of 2026-09-30 said plugins stay denied. That is reversed for
this one, and only this one: `hyprland-security.conf` allows
`/usr/lib/libhyprexpo.so` by name, above the rule that denies every other,
and loads it. What that gives up is said plainly. A plugin is code inside
the compositor, able to see every window and every key, and hyprexpo is now
trusted as Hyprland itself is, on the grounds that it has the same authors
and comes signed from the same repository; a fault in it is a fault in the
compositor. What is kept: a program still cannot have Hyprland load a
plugin of its choosing, since the rule names one file and only root can
write it. A plugin must also match its Hyprland exactly, so an upgrade of
one without the other would leave the overview not loading; both come from
Alpine's community repository, which rebuilds them together.

hyprexpo knows nothing of each screen having its own workspaces: its grid
is a run of numbers from wherever it is told to start. So Super+Tab runs
`alpymist displays overview`, which asks which screen has the focus, tells
hyprexpo to start at that screen's first — 1, 11, 21 — and opens it, three
by three: that screen's nine. The key is a line in the account's own
`hyprland.conf`, given to an account made before it when either Displays
setting is changed, unless Super+Tab is already bound there. It goes
through `alpymist` rather than naming the plugin's dispatcher, so a
configuration read by a session that has not loaded the plugin has no
error in it.

Hyprland reads permissions once, when it starts. A session that began
before the upgrade does not load the plugin, and says nothing; the key does
nothing until the next login.

## Addendum — 2026-10-07: the overview needs `nm`, and an x86_64 processor

The overview was tried on a machine that builds Alpymist, which has
binutils, and on no other. On the aarch64 development machine, the first
login after the upgrade that brought the plugin had two red lines across
the desktop: hyprexpo could not load, "no fns for hook renderWorkspace".

Two things are behind that, both read from Hyprland 0.54.3's source:

- Hyprland finds the function a plugin wants to hook by running `nm -D`
  over its own binary. `nm` is binutils', which neither Hyprland's package
  nor the plugin's depends on, and the desktop did not either. Without it
  no function is found, on any processor. `alpymist-desktop` now depends on
  `binutils`, 14 MiB.
- Hooking a function is written for x86_64 and returns at once on anything
  else. On aarch64 hyprexpo cannot load whatever is installed.

So the plugin is loaded only where it can load. `alpymist session` sets
`ALPYMIST_OVERVIEW` for Hyprland on x86_64 when `nm` is there, and
`hyprland-security.conf` loads hyprexpo inside `# hyprlang if
ALPYMIST_OVERVIEW`. Elsewhere nothing is loaded and nothing is said at
login; Super+Tab says, in a notification, that the overview is not for
this processor. The rule that allows the one plugin's file is as it was.

What this leaves: no overview on aarch64, and binutils installed there for
nothing, since the desktop's package is the same for every processor. An
overview that works everywhere is one drawn by a program of ours and not
by a plugin, which would also take the plugin back out of the compositor;
that is not decided here.

Tried on the aarch64 machine: Hyprland reads the new file with the variable
and without, and an `if` on a variable of the environment skips what is
under it. Not tried: a login with the new packages on either processor, and
x86_64 without binutils, where the first cause is read from the source and
not seen.

## Addendum — 2026-10-02: the watch moves to the watchdog

`alpymist displays watch` was started by the packaged Hyprland configuration
on its own. It is now one of the watches `alpymist watchdog` keeps
([ADR 0019](0019-watchdog.md)), which is what that configuration starts
instead; the command is still there, and does what it did.

The watch took Hyprland to have ended when the event socket closed, and
stopped following the screens. Hyprland also closes that socket to a listener
that falls sixty-four events behind, so the watch now asks to listen again,
looks at the screens afresh, and ends only when there is nothing to connect
to.
