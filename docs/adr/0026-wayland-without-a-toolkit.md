# ADR 0026 — Wayland without a toolkit

**Status:** accepted · **Date:** 2026-10-04

## Context

Everything in the session that is not a terminal is put on screen by one of
four hosts: the menu's, the popups', the application window's and the lock
screen's. All four stood on smithay-client-toolkit, and through it on calloop,
a cursor-theme loader, client-side decoration types and a second memory-map
crate: eighteen packages that nothing else in what ships needed, under every
desktop program there is. The four hosts were also four copies of the same
plumbing — about a hundred and fifty lines each of handlers that did nothing,
written out because the toolkit's traits asked for them.

The toolkit is well known and would pass ADR 0023's test on its own. What it
was used for is small: bind a dozen globals, acknowledge configures, hand a
buffer over, turn key numbers into keys, and sleep until something happens.

## Decision

**`alpymist-wayland`: the four hosts' plumbing, once, over `wayland-client`.**
Layer surfaces, windows, the session lock and its surfaces, outputs, one
keyboard, one pointer, timers, and a loop on `poll`. A host asks it for
surfaces and reads a list of events; there are no handler traits to fill in.

**The protocol itself is not ours to write.** `wayland-client` and the
protocol crates beside it stay, and libxkbcommon — through the `xkbcommon`
crate — says what a key means. Both are what every Wayland client in Rust
stands on. A wire implementation of our own would be the D-Bus one again at
ten times the size, for a format that still grows.

**Pictures are written, not mapped.** A `wl_shm` buffer is a file both sides
hold. Mapping it is `unsafe` whichever crate is asked to, and ADR 0004 keeps
`unsafe` to two files. So a host paints in memory of its own and the picture
is handed over with one `pwrite`. Nothing here is `unsafe`, and the crate is
`#![forbid(unsafe_code)]` like the rest.

**The keymap is read, not mapped,** for the same reason: `read` on the
descriptor the compositor sends, and libxkbcommon given the text.

**One seat's keyboard and pointer, and keys repeated here.** `wl_seat` is
asked for at version 9 at most, so that a held key is repeated by the client
on every compositor rather than by some compositors and not others.

**No cursor themes.** The pointer's shape is asked for by name through
`wp-cursor-shape-v1`, as it already was. Where a compositor lacks that, the
pointer keeps what it wore.

## How it is held to that

- Unit tests for what can be tested without a compositor: the channel that
  wakes the loop, and the keyboard against libxkbcommon's own layouts.
- The rest needs a compositor. It was checked by running the programs built
  before and after this against a headless Sway and comparing what was on
  screen: the menu, a popup, a window, the lock on one output and on two with
  one unplugged and plugged in again, at scale 1 and 2, with a key held, text
  the layout does not have, and a pointer moved, clicked and scrolled. The
  pictures were the same to the pixel, apart from the clock and the wheel
  below, and the menu's first frame as soon. That is a check
  made once, by hand; nothing in CI runs it.

## Consequences

- Every desktop program links seventeen or eighteen fewer third-party crates: the menu and
  the popups 62 to 44, the lock screen 71 to 54, Settings 72 to 55. The
  lockfile goes from 267 to 262; it still holds the toolkit, because winit,
  under the desktop preview of the installer and login screen, wants it.
  Nothing that ships links it.
- `ALLOWED` goes from sixteen names to nineteen: one toolkit out, and the four
  crates it stood on in, now named for what they are.
- **A full-screen picture costs one more copy a frame.** Painting into a
  mapping was one pass over the screen; painting and then writing is two.
  Measured on a 1920×1080 output, the mountains screensaver went from about
  2% of a core to about 6%. The menu, popups, windows and the lock, which
  draw when something happens rather than twenty times a second, measure the
  same as before. The way back is a mapping, in one more file allowed
  `unsafe`: an addendum to ADR 0004 and to this, not an edit.
- Programs hold less memory: a mapped buffer counted against the program as
  well as the compositor. The screensaver went from 26 MB resident to 10, the
  lock from 44 to 19.
- A wheel scrolls one row a notch again. Since the move to the toolkit's 0.21
  a wheel's notches arrived in a form the hosts did not read, and its distance
  was used instead.
- What the toolkit knew and this must now know: the order surfaces and their
  roles are destroyed in, that every configure is owed an acknowledgement and
  a buffer, which version of each global brings what. A mistake there is a
  protocol error, and the compositor disconnects the program that made it.
- Not exercised by the check above, and to be tried on a machine: Hyprland
  itself, a real pointer and touchpad, a keyboard with dead keys, HiDPI with a
  fractional scale, and a screensaver across two screens.
