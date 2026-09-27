# ADR 0012 — Thunderbolt and USB4 devices are asked about, and the IOMMU is on

**Status:** accepted · **Date:** 2026-09-26

## Context

A Thunderbolt or USB4 device is connected to the computer as if it were
inside it. Its PCIe tunnel lets it, or anything behind it, read and write
memory by DMA. So most firmware does not let such a device in by itself. It
sets the host controller's security level to `user` or `secure` and leaves
the decision to the operating system, which says yes by writing to the
device's `authorized` file in sysfs.

Nothing in Alpymist did that. On the dev machine (a ThinkPad X1 Carbon on a
ThinkPad Thunderbolt 3 Dock) the dock's screens worked, because DisplayPort
tunnels need no approval, but its keyboard, mouse, network and sound never
appeared. The dock sat at `authorized=0` behind a domain at level `user`.
Every Thunderbolt 3, Thunderbolt 4 and USB4 dock behaves this way on firmware
that asks, from any vendor, on Intel and AMD alike.

Two further facts from that machine:

- **The IOMMU was off.** The hardware has one (VT-d, listed in the ACPI DMAR
  table), but Alpine's `linux-lts` is built without
  `CONFIG_INTEL_IOMMU_DEFAULT_ON`, and neither the live image nor the
  installer asked for it. So `iommu_dma_protection` read `0`: a device let
  in could reach all of memory. AMD's IOMMU is on by default wherever it
  exists.
- **At level `user` a device proves nothing.** It is known by the UUID it
  reports, and another device can report the same one. Only at level
  `secure`, and only with a device that has a `key` file, can a device be
  given a random key the first time and made to answer a challenge against
  it afterwards.

The usual tool is bolt (`boltd` and `boltctl`), and it is packaged for
Alpine. It does not fit Alpymist. It is started by systemd, through its udev
rule and D-Bus activation, so on OpenRC nothing would run it at boot to let
a known dock back in. Its polkit rule trusts "an active, local wheel user",
and without elogind polkit sees no sessions, so that rule never matches.
Alpymist already has the pattern bolt would replace: a small root helper run
through `pkexec`, rules keyed on the `seat` group, and `alpymist-auth` to ask
for a password (as `alpymist-power` does).

[ADR 0011](0011-secure-by-default.md) decides the rest: nothing is let in that
nobody chose to let in.

## Decision

### 1. `alpymist-thunderbolt`, not bolt

One crate, three parts, nothing vendor-specific:

- **The helper**, `alpymist-thunderbolt-helper`, run as root through pkexec,
  udev and OpenRC. A closed set of verbs: `allow UUID once|always`,
  `reconnect UUID`, `forget UUID` and `boot`. A device is named by the UUID it
  reports, checked, and found by reading the bus; no path is taken from the
  caller.
- **The store**, `/var/lib/alpymist/thunderbolt/`. `allowed.toml` lists the
  devices allowed always, readable by everyone so the session and Settings
  can show it. `keys/<uuid>` holds a key, readable by root alone.
- **The session's watch**, `alpymist-thunderbolt`, started with the desktop.
  It listens for the kernel's uevents, which any user may read, asks about a
  new device, and lets a remembered one back in.

It reads only what the kernel documents for every Thunderbolt and USB4 host
(`Documentation/admin-guide/thunderbolt.rst`), and does nothing at the levels
where the firmware decides (`none`, `dponly`, `usbonly`, `nopcie`).

### 2. What happens when a device arrives

| The device is… | Unlocked desktop | Locked, or nobody logged in |
|---|---|---|
| already in, or at a level that needs no approval | nothing | nothing |
| unknown | asked about | waits |
| allowed always, known only by its UUID | let in again, without asking | waits |
| allowed always, proves who it is with its key | let in again | let in again |

- **Asking** is a dialog of ours in the middle of the screen, the rest dimmed.
  It names the device, says what letting it in means, and offers *Don't
  allow*, *Allow once* and *Always allow*. *Don't allow* is the highlighted
  button, and Enter with nothing focused does nothing. Allowing then asks for
  an administrator's password (`org.alpymist.thunderbolt.allow`, always
  `auth_admin`), because this gives the device the run of the machine.
- ***Always allow* says what it means for this device.** Where the device can
  only give a UUID, the dialog says in one sentence that another device could
  pretend to be this one, and that *Always* would let that one in too
  whenever the screen is unlocked. Where it can prove itself, the dialog says
  that instead.
- **Letting a remembered device in again** needs no password for the `seat`
  group (`org.alpymist.thunderbolt.reconnect`), as suspending does not. The
  administrator already decided. The helper refuses any device that is not
  in the store.
- **Never while locked, never at the login screen**, unless the device proves
  who it is. A lock that let in any device reporting the right UUID would
  guard nothing. The cost: a laptop that boots docked with its lid shut, at
  level `user`, cannot use the dock's keyboard to log in. Open the lid, or
  use a device that can take a key.
- **At level `secure`** the first *Allow* writes a fresh 32-byte key into a
  device that has a `key` file. *Always* keeps that key, and from then on
  the device must answer a challenge (`authorized=2`). udev and the OpenRC
  service do this at boot and on hotplug, before anyone logs in.

### 3. The IOMMU is on by default

The installer passes `KERNELOPTS="quiet intel_iommu=on"` to `setup-disk`, and
the live image's kernel command line carries `intel_iommu=on`, on every tier.
With the IOMMU on, a device can reach only what its driver mapped for it, and
on firmware that marks its Thunderbolt ports (`iommu_dma_protection=1`) the
kernel also bounces untrusted devices' DMA.

It is a small risk on old or buggy chipsets, the Potato tier's in particular,
where VT-d firmware tables are sometimes wrong. The remedy is the usual one:
`intel_iommu=off` on the command line, from GRUB's editor once and in
`/etc/default/grub` for good. That is a choice to make the machine less safe,
and per ADR 0011 it is the owner's to make, not ours to make for everyone.

Installed systems are not changed on upgrade. Rewriting a machine's boot
configuration from a package script would be a surprise even when it is the
safe direction.

## Consequences

- A Thunderbolt or USB4 dock works on Alpymist after one question and one
  password, and comes back without either while the screen is unlocked.
- On a `user`-level laptop, a remembered dock waits for an unlocked session,
  so docked boots with the lid shut need the lid opened to log in.
- The Legacy tier (i3, X11) has no layer shell for the dialog and does not
  install the package. There, `alpymist-thunderbolt-helper allow UUID once`
  through `doas` is the way until it has an answer of its own.
- The live image and the installer do not ask yet: someone installing from a
  docked laptop at level `user` has to use the built-in keyboard. Asking on
  the console there is tracked in #18.
- `iommu_dma_protection` depends on the firmware as well as the IOMMU. The dev
  machine's 2018 firmware may never report `1`. The IOMMU still confines
  devices, but that bit is not ours to set.
