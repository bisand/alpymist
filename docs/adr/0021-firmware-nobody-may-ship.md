# ADR 0021 — Firmware nobody may ship is fetched by the machine that asks

**Status:** accepted · **Date:** 2026-10-02

## Context

Alpymist says it is for old machines. A 2009 MacBook Pro has a Broadcom
BCM4322 Wi-Fi card, as every Mac of those years and a great many other
laptops have one of its family. The kernel's `b43` driver has it, and does
nothing without firmware. That firmware is in no package of Alpine's, nor of
any other distribution's, because Broadcom never gave anyone leave to pass it
on. Debian, Arch and the rest ship a tool that downloads Broadcom's own
driver for routers, which has the firmware in it, and cuts it out on the
machine.

The machine was installed, its Wi-Fi card was not found, and it had no cable.
What worked was done by hand: the driver downloaded on another machine, the
firmware cut out there with `b43-fwcutter`, carried over on the install
stick, and copied into `/lib/firmware`. The card came up at once.

Three ways were open: fetch it on the machine that asks; carry it from a
machine that can fetch; or put the firmware on the image.

## Decision

**Alpymist does not ship the firmware, in the image or in any package.**
Putting it on the image is the only way that works with nothing asked of
anyone, and it is passing on what there is no leave to pass on, to everyone
who downloads the image, in the project's name.

**`alpymist firmware broadcom` fetches it, on the machine that asks.** As
root: it installs `b43-fwcutter` from Alpine if it is not there, downloads
`broadcom-wl-5.100.138.tar.bz2` from OpenWrt's source mirrors over TLS,
refuses it unless its SHA-256 is the one written in the program, cuts the
firmware into `/lib/firmware/b43`, and starts the driver again. That version
is the one the `b43` developers name for every kernel since 3.2; the place
Broadcom's files were first published, lwfinger.com, no longer serves them.

**`--to DIR` puts it on a stick instead, and `--from DIR` and the installer
take it from one.** The machine that needs it is the one with no network.
`alpymist firmware broadcom --to /media/usb`, on any machine with a network,
leaves it in `alpymist-firmware/b43` there and needs no root. The installer
copies what the stick it installs from carries into the new system, with
nothing asked; `doas alpymist firmware broadcom --from DIR` does the same on
a system already installed. Only `.fw` files, only ordinary ones, only into
the directories named in `alpymist-core`'s `CARRIED_DIRS`: it runs as root
on what a stick holds.

**Nothing runs the fetching for anyone.** The firmware watch, seeing `b43`
ask for a file no package has, says so in a notification with the command
to run; the installer says the same in its log. Neither downloads. It is an
opt-in in ADR 0011's sense: a file from outside Alpine's and Alpymist's
repositories, signed by nobody, held to a checksum and asked for by name.

**A pin, as squint's is.** One file, one checksum. A mirror that serves
something else is refused, and a new version of the driver is a change to
the program, reviewed like any other.

## Consequences

- A machine with one of these cards and no cable, no phone to tether and no
  second computer cannot have Wi-Fi. That is what shipping the firmware
  would have mended, and it is given up.
- A stick written with `dd` is an ISO 9660 filesystem and cannot be added
  to. The firmware goes on a stick made by copying the image's files to a
  FAT partition — which an old Mac needs anyway to see the stick at all — or
  on a second stick, taken with `--from` after installing.
- The live system does not use what is carried: its `/lib/firmware` is the
  kernel's modloop. A machine whose only network is such a card installs
  with no network, which the image is made to do, and has Wi-Fi from its
  first start.
- OpenWrt keeps that file for building its own images. If it goes, the
  command fails with a message and the pin needs a new home; the firmware on
  a stick someone already made still works.
- This is the one driver done. `b43legacy`, for cards older still, wants a
  different file from the same maker, and is a new entry when a machine
  that needs it turns up — which `alpymist report` (ADR 0020) is for.
- Firmware written from nothing, to be shipped freely, is the other answer.
  OpenFWWF is that for the oldest `b43` cards and not for this one; nobody
  has written it for the N-PHY cards. It is not ruled out here; it is a
  project of its own, to be measured before it is promised.
