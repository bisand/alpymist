# ADR 0020 — Hardware reports, sent by the person and not by the system

**Status:** accepted · **Date:** 2026-10-02

## Context

Alpymist was installed on a 2009 MacBook Pro, and three things were wrong
that no machine it had been tried on had shown: the Wi-Fi card was not found,
the pointer was not drawn, and the image was not seen by the firmware at all.
Finding out why meant asking the person at the machine to type `lspci -nn`,
`dmesg | grep firmware` and the like, and to copy the answers back by hand,
from a machine with no network.

That is how every such machine will be found: by whoever has it. What they
are asked for is the same each time — which devices, which driver has each,
which firmware was asked for and not there — and none of it can be had any
other way. Nothing in Alpymist sent anything anywhere, and
[ADR 0011](0011-secure-by-default.md) is why.

## Decision

**`alpymist report` gathers it, and sends nothing.** Alone it prints the
report. `--save FILE` writes it to a file, for the machine with no network:
onto the stick it was installed from, say. `--issue` prints it, asks, and on
a yes opens the browser at a new issue in Alpymist's repository with the
report filled in. The issue is posted when the person presses the button on
that page, signed in as themselves, having seen it twice.

**Nothing runs it for anyone.** Not the installer, not a service, not the
first login, not on a failure. There is no setting that turns reporting on,
because there is no reporting to turn on: only a command, run by hand.

**It holds hardware, and nothing that names a person or tells one machine
from another of its kind.** In it: the make and model the firmware gives,
the kernel and Alpine and Alpymist versions, the processor, what `alpymist
probe` says of Hyprland, every PCI and USB device by its maker's numbers and
the driver bound to it, each network card as wired or wireless and its
driver, the sound cards and their codecs, and the firmware files drivers
asked for and did not find. Not in it: serial numbers and the machine's
UUID, the addresses of network cards, the host name, account names, networks
joined, the names of disks and what is on them, and the kernel log itself.

Two rules follow from that and are easy to break by accident:

- *Devices by number, not by the name they announce.* A phone on USB
  announces "André's iPhone". `05ac:12a8` says what it is and not whose.
- *A network card by its kind, not its name.* `enx0026b0aabbcc` is the
  card's address.

`alpymist-hwprobe`'s `report` module is where what goes in is decided, and
its test builds a machine with each of those in it and fails if any comes
out.

**There is no identifier.** Nothing is made up to tell one report from the
next, so the same machine reported twice is two reports.

**The firmware section needs root where the kernel log does.** The kernel
restricts its log to root on an Alpymist system. Run as the account, the
report says that section was not read and how to have it; it does not ask
for a password, and `--issue` is not for running as root.

## Consequences

- A report reaches Alpymist only through GitHub, so sending one needs an
  account there, and what is sent is public. Someone without an account, or
  who would not have it public, has the file and no way offered to send it.
  An address of Alpymist's own to post to was the other way: one keypress and
  no account, against a service to run, to secure and to answer for what it
  keeps. It was not taken. If the account turns out to be what stops people,
  that is an addendum here.
- The report goes in the address the browser is given, and GitHub takes
  about eight thousand characters of one. A longer report is left in a file
  and the issue asks for it to be pasted. The issue names the file and not
  its path, which has the account's name in it.
- Make and model are in it, and somebody with a rare machine can be known by
  that. It is what the report is for, it is shown before anything is opened,
  and it is why nothing sends it without being asked.
- Nothing counts how many machines run Alpymist or on what. That was never
  wanted from this.
- The sound card's name and the graphics renderer's are text a device or a
  driver gives, and are in the report as given. They were let in because
  sound and graphics cannot be told apart without them; a device that put a
  person's name there would have it reported.
