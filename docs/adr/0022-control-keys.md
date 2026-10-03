# ADR 0022 — The keyboard's control keys

**Status:** accepted · **Date:** 2026-10-03

## Context

No volume, brightness, media or radio key did anything on any machine.
Hyprland binds nothing by itself, and there is no desktop daemon underneath
it to catch those keys: no elogind, no settings daemon. The kernel is built
without `RFKILL_INPUT`, so it leaves the radio keys to the session as well.
The manual said to bind them by hand.

The 2009 MacBook Pro made it plain: its top row is these keys, and with none
of them working the screen stays at whatever brightness it started at.

## Decision

**The package binds them, for every machine alike.**
`/usr/share/alpymist/hyprland-keys.conf`, sourced by `hyprland-security.conf`
as `hyprland-windows.conf` is, so accounts made before it have the keys at
their next login and an upgrade keeps them current. There is no table of
machines: every key is bound everywhere, and a key a keyboard lacks is a bind
never pressed. Which key a keyboard sends is the kernel's and xkb's to say.

**Each key runs `alpymist key NAME`.** Not `wpctl` and `brightnessctl` lines
in the configuration: a program can say what it did, in one notification
that each press replaces, which is the only place a brightness is shown at
all; it can keep the volume at 100%, keep the screen from going dark, take
finer steps where the light is low, pick the right backlight where a machine
has two, and be tested. A test holds the binds and the program's names to
each other.

**Nothing runs as root, and nothing new listens.** Sound is the session's.
The screen's backlight and the keyboard's are given to the `video` group by
a udev rule, and the account at the screen is in it. brightnessctl's rules
give every LED to `input`; the account is deliberately not in `input`, which
reads every keyboard, so the keyboard's light, and no other LED, goes to
`video` instead.

**A radio is switched by its daemon, not by rfkill.** `/dev/rfkill` is
root's. Wi-Fi is iwd's to power, for the `netdev` group, exactly as the
Wi-Fi popup does it, so the key and the popup cannot disagree; Bluetooth is
bluetoothd's. Airplane mode is both off, and the ones it switched off back
on, remembered in the runtime directory.

**Bluetooth off stays off.** The Bluetooth key does nothing but say so while
bluetoothd is not running, and airplane mode coming off does not start it:
turning Bluetooth on is a system setting behind a password (ADR 0011), not a
key.

## Consequences

- A radio switched off by its key is powered down, not blocked: `rfkill
  list` shows it unblocked. A machine whose firmware blocks the radio itself
  on that key does both, and the two stay in step.
- Most laptops light their keyboard in firmware and send no key for it; the
  keyboard-light binds are for those that do, a Mac's above all.
- Do not disturb hides the notification too. The key still works.
- The keys work on the lock screen, the ones that open something apart.
  Turning the volume down on a locked machine is wanted; so, arguably, is
  not letting a passer-by switch Wi-Fi off. It is the same passer-by who
  can close the lid, and the key is left working.
- A key whose scancode the kernel has no name for still does nothing. That
  is a `hwdb` entry for that machine, and `alpymist report` is how it is
  heard of.
- Tested on the ThinkPad X1 this was written on: volume, mute, microphone
  and screen brightness. The Mac's keys, its keyboard light and the radio
  keys were not pressed by anyone before this was written down.
