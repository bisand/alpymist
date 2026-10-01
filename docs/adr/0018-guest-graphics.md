# ADR 0018 — Graphics in a virtual machine: a second Mesa, asked for

**Status:** accepted · **Date:** 2026-10-01

## Context

Alpymist in a virtual machine was slow in a way Ubuntu in the same virtual
machine was not. The reason is one word in a build recipe. QEMU and UTM offer
a guest the host's graphics card through virtio-gpu, and the kernel takes the
offer (`[drm] features: +virgl`), but the Mesa driver for it, `virgl`, is not
in Alpine 3.24's Mesa: `_gallium_drivers` in `main/mesa` leaves it out on
every architecture. Mesa says `virtio_gpu: driver missing` and falls back to
llvmpipe, and Hyprland, which draws everything through GL ES shaders, then
draws every frame on the processor.

Measured on the dev VM (UTM on an M5 Pro, 6 virtual processors, 1280x800,
Hyprland 0.54.3), with Alpine's aport rebuilt with `,virgl` and nothing else
changed:

| Load | llvmpipe | virgl |
|---|---|---|
| Workspace switching, four a second | 389% of a core | 5% |
| A full-screen screensaver at 20 frames a second | 199–349% | 5% |
| Repainting every frame | 223%, 59 fps, 14.1 ms a frame | 13%, 68 fps, 2.9 ms |

Alpine has since put the driver back, in aports !107536, merged to master on
2026-08-29. It is in edge and will be in 3.25. It is not in 3.24-stable, which
is what Alpymist is built from (ADR 0005).

## Decision

**A second apk repository carries Alpine's Mesa with `virgl` added, and a
system follows it only when asked.** It is one more line in
`/etc/apk/repositories`, `https://guest.pkgs.alpymist.org/v3.24/guest`, and
one more key, exactly as the dev channel is (ADR 0006) and independent of
which channel the system follows.

**The Mesa is Alpine's, built elsewhere.** `bisand/alpymist-mesa` holds no
copy of Mesa or of the aport. Each night it reads Alpine's `main/mesa` on
`3.24-stable`; when that has moved, it takes the aport at that commit, adds
the one word, raises `pkgrel` by a hundred so apk prefers it to Alpine's build
of the same version, builds for x86_64 and aarch64, signs the index and
publishes. Nothing of it is built in this repository, and nothing in
`ci/build-packages.sh` knows of it.

**It is not on every machine**, which is why it is a repository and not a
package in Alpymist's own. A Mesa of a higher version in the repository every
system follows would be installed by every `apk upgrade`, on hardware it does
nothing for, built by someone other than Alpine. In a repository of its own
it reaches the machines that asked.

**The installer offers it where it is for.** On a machine whose display is
virtio-gpu, the Desktop screen has one switch, on: *Draw with the host's
graphics card in this virtual machine*. Left on, the installed system follows
the repository and takes its Mesa before the first boot. On any other machine
the switch is not there. Afterwards:

```sh
doas alpymist guest on     # follow it, trust its key, upgrade
doas alpymist guest off    # stop, distrust it, back to Alpine's Mesa
```

and it is `updates.guest-graphics` in Settings › Updates.

**Its key is its own, and trusted only where it is on.** apk trusts every key
in `/etc/apk/keys` for every repository. `alpymist-keys` ships
`alpymist-guest-2026.rsa.pub` to `/usr/share/alpymist/keys`, where apk does
not look, and turning the switch on copies it; turning it off removes it. The
private half is a secret of the `guest-channel` environment of
`bisand/alpymist-mesa`, which only its `main` may deploy to, and is read by a
job that runs `apk index` and `abuild-sign` and nothing else. The jobs that
run Alpine's APKBUILD and Mesa's build never see it.

## How this sits with the earlier decisions

**ADR 0011, secure by default.** The switch is on in the installer where it
is offered, so this is a default, and it has to be argued as one. What it
turns on is not a weakening of anything ADR 0011 lists: no privilege, login,
service, device or secret. It is a second source of packages, and that is
ADR 0002's ground. It is shown and can be declined before anything is
written, it changes nothing on a machine that is not a virtual machine, and
it is undone by one command.

**ADR 0002, the supply chain.** "No third-party repositories by default"
stands: this one is first-party, its index signed by a key this project
holds, its contents Alpine's source at a named commit with Alpine's
checksums. What it adds is a third key held by GitHub Actions, in a second
repository, and anyone who turns the switch on is trusting that workflow for
their Mesa, which is code that runs in every graphical program. A system that
does not turn it on trusts nothing new.

**ADR 0001, the probe.** The probe said *Unlikely* of the machine this was
measured on once it had the driver: virgl over ANGLE on a Mac offers GL ES
3.0, and the check required 3.2. Hyprland 0.54.3 started and ran on it. The
floor in `alpymist_core::hyprland` is now 3.0, and ADR 0001 has an addendum
saying so.

## Consequences

- When Alpine ships a newer Mesa in 3.24, apk prefers Alpine's until the next
  night's build is out, and a guest draws on the processor again for those
  hours. Nothing breaks. Stable and dev are alike in this: the repository is
  published on its own, not with a release.
- A system that follows it needs it to answer. apk stops an upgrade when any
  repository it follows cannot be reached, so with the guest repository's
  site down, `apk upgrade` on a guest fails until it is back or the switch is
  turned off. The same is already true of the channel's.
- Turning it off has to pass `--available` to apk, as leaving dev does: the
  guest Mesa is versioned above Alpine's and would otherwise stay.
- The installer's second step needs the network. Offline, the system still
  follows the repository, and the first `apk upgrade` brings the Mesa.
- Whether the host actually offers 3D cannot be told from the guest's sysfs.
  Where it does not, the guest Mesa draws on the processor as Alpine's does,
  and the switch has cost a repository line.
- Only aarch64 under UTM was measured. x86_64 is built and not yet tried, and
  nothing here has been tried with a browser or video.
- A system keeps following the repository after it stops being needed, until
  told otherwise.

## When it ends

With Alpine 3.25, or sooner if 3.24-stable takes the backport. `patch.sh` in
`bisand/alpymist-mesa` fails the nightly run, by design, on the day Alpine's
APKBUILD builds virgl itself. Then: stop the schedule, make
`alpymist guest` and the installer's switch say there is nothing to turn on,
and record it here in an addendum. Moving Alpymist to a new Alpine release
changes the `v3.24` in the repository's address with the channels' (ADR 0006).
