# ADR 0030 — A game for a screen with no desktop, and for systems that are not ours

**Status:** proposed · **Date:** 2026-10-07

## Context

Solitaire (ADR 0029's addenda) is a window on the desktop. Its rules, its
table and the hand that plays it are a library with no window in it, and
Denise draws to a display through DRM/KMS and reads a mouse and a keyboard
from the kernel without a compositor: the login screen, the installer and
the splash are made that way.

So the same game can be shown on a text console, on a machine with no
desktop installed, on a Raspberry Pi. That is a program somebody may want
who does not run Alpymist, which nothing of ours has been until now:
everything else is a package for our own repository.

## Decision

**`alpymist-solitaire-console` is the game on a screen with no desktop.** A
crate of its own in the console build group, over `alpymist-solitaire`
built without its window: the display opened as the login screen opens it,
the mouse, a touch screen and the keyboard from `denise-evdev`, and the
pointer painted by the program. Everything a hand does and everything a
game shows is the library's `play`, the same as in the window. For that,
`alpymist-widget`'s Wayland host is a feature, on by default; without it
that crate is the fonts, the colours and the controls.

**It is given away as one file, and not as a package.** Each release
attaches `alpymist-solitaire-console-ARCH`, with its SHA-256, for x86_64,
aarch64 and armv7: linked statically against musl, so it asks the system
it runs on for nothing. `ci/build-standalone.sh` builds all three on one
machine with the linker and the C runtime that come with each Rust target,
and no cross compiler. The packages stay dynamic, as Alpine's are
(ADR 0004); this is built apart from them, in a directory of its own.

**It does not fail a release.** The job that builds it may fail without
failing the run stable is published from. Fewer files on the release than
there should be is how that shows.

**Alpymist does not install it.** It needs the display and the input
devices, which is root, or an account in `video` and `input`. An account
in `input` can read every keystroke typed on the machine, and ADR 0011
keeps accounts out of it; a package that only worked for someone who had
undone that would be an invitation to. Where there is a desktop there is
the window. Someone on Alpymist's own text console can download the file
like anyone else, and run it as root.

**It leaves by Ctrl+Q, Ctrl+C or a Quit button on the bar**, since there is
no window to close, and asks first when a game is under way.

**The table is painted once and kept.** A frame is that memory copied
where the pointer and the cards in the hand were and are; a button lit, a
clock's second and a letter typed into a panel are painted again only
inside what they cover. On a Raspberry Pi 3A+ a table painted whole is
48 ms, a card carried 3 ms and a button lit under one. It is what Denise's
own examples do with a tree's damage, done by hand for a game with no
tree.

**It carries no font.** Where the theme's is not installed it writes with
the first plain sans-serif the system has, DejaVu, Liberation or Noto, and
with Denise's built-in bitmap where there is none.

## Consequences

- The first thing of ours that is for other systems. It is a game, takes
  nothing from the machine but a screen and a hand, and keeps one file,
  `~/.local/state/alpymist/solitaire`: the back, the way of turning and
  the best games. Nothing is sent anywhere.
- While it runs, another console cannot be switched to: it holds the
  display and has muted the console's keyboard. The login screen lets go
  for another console and this does not yet.
- It looks at the mouse every 3 ms and does not sleep until the mouse
  moves, as Denise's examples do: that takes a wait on the devices' own
  descriptors, which the input crate does not hand out without `unsafe`.
- `--nearly-out`, a game five cards from its end, and `--bench`, what a
  frame costs on the machine, are there and not in the usage.
- 2.4 MiB a file, with the cards and the backs in it.
- Tried: played on a virtual machine's text console and on a Raspberry Pi
  3A+ running Alpine, both aarch64, with a mouse and a keyboard; the
  aarch64 file started on Debian 12 in a container. Not tried: the armv7
  and x86_64 files run at all, a touch screen, a second display, Raspberry
  Pi OS, and a system with none of the fonts looked for.
