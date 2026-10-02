# ADR 0019 — A watchdog for the session

**Status:** accepted · **Date:** 2026-10-02

## Context

The bar on the X1 stopped following the workspaces: the numbers stayed as
they were, the underline did not move, and a click on a number changed the
screen and not the bar. Nothing had crashed. Each bar shows its screen's
workspaces from what Hyprland says on its event socket; Hyprland takes a
listener off that socket when it falls sixty-four events behind
(`EventManager.cpp`, 0.54.3), and waybar 0.15.0, taken off, reads the end of
the socket for ever and never connects again. Its development branch stops
reading instead, which is no better. What held the bar up that long was not
found — twenty-five configuration reloads in a row did not do it — but
stopping the bar while events arrive does it every time.

Mending that needs something running for the length of the session that
looks at the bar now and then. There was one such program already, `alpymist
displays watch`, and the first mend put the bar into it. That was the wrong
home: the bar is not a screen, and the next thing to keep an eye on would
have gone into whichever program happened to be running too.

## Decision

**One watchdog, `alpymist watchdog`, started once by the packaged Hyprland
configuration.** What it watches is a list in `alpymist-watchdog`, `WATCHES`:
a name, a line saying what it is for, and a function that keeps watch until
Hyprland ends. `alpymist watchdog --list` prints it. A new watch is a new
entry there, not a new `exec-once`.

**It is the account's, and has nothing the account does not.** It runs in the
session, as the user, started by Hyprland. It listens on no socket, takes no
commands, reads no configuration and keeps no file. It is not a service: a
session that is not Hyprland's has none, and it ends with Hyprland.

**A watch is a process.** Alpymist's programs are built to end on a panic
rather than unwind, so a thread would not keep one watch's fault from the
others. The watchdog starts each watch as itself again with the watch's name,
waits, and starts again one that ended badly: after two seconds, then four,
doubling to five minutes, and from two again once a watch has kept going a
minute. It never gives one up. A watch that ends well has seen Hyprland go,
and the watchdog ends; a watch asks the kernel to be ended with the watchdog,
so none is left behind.

**A watch acts only on what it has seen, and does one thing.** The two there
are:

- *screens* — `alpymist displays watch` as it was (ADR 0015): the layout for
  the screens connected now, put in place when the set of them changes.
- *bar* — every twenty seconds it asks the kernel, over a `sock_diag` netlink
  socket, for the unix sockets connected to Hyprland's event socket, and
  compares them with the sockets each waybar of the account holds. A bar that
  was listening and is not, two looks running, is ended and started again by
  Hyprland with the arguments it had. A bar that never listened — one of the
  account's own, with no Hyprland modules — is never touched, and neither is
  any when the kernel does not answer or its answer is not whole. Toggle ›
  Top bar still turns the bar off: there is then no bar to look at.

**What stays out.** The watchdog is for looking after the desktop the account
is sitting at. It is not where everything that runs for a long time goes:

- `alpymist firmware watch` installs packages, as root, from boot. Putting it
  with watches that run as the account would mean either the watchdog as
  root, or a way for the account to ask root for things; it stays a service
  of its own.
- The clipboard history holds what was copied, passwords among it, and runs
  only when asked for (ADR 0014). It stays a program of its own, so that
  nothing else is in the process that holds those.
- `alpymist-thunderbolt` asks before a device is let in (ADR 0012), and the
  idle watch decides when the screen locks (ADR 0009). Each is the thing
  itself, not a watch over something else, and a fault in a watch should not
  be able to take either down.

## Consequences

- Three small processes where there was one: the watchdog, which only waits,
  and one for each watch.
- A bar cut off from Hyprland is back within a minute — forty seconds at
  most to be sure of it, then the restart, which shows as the bar blinking.
- Building waybar here with a change to connect again was the other way, and
  was not taken: it would put a patched bar in place of Alpine's on every
  machine, to be kept current by hand. If waybar comes to connect again
  itself, the bar watch finds nothing to do and can go.
- A session already running when the package is upgraded keeps the watch it
  started with until the next login.
- `sock_diag` is read-only and needs no privilege for the account's own
  sockets, but it is one more thing the kernel must provide; where it does
  not, the bar watch does nothing, and says nothing.
