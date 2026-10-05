# ADR 0027 — Development for glibc: a box of another distribution, documented and not shipped

**Status:** accepted · **Date:** 2026-10-05

## Context

ADR 0003 answered software built for glibc with Flatpak, and rejected
`gcompat`. That holds for applications. It does not hold as well for the
tools of someone who writes software, and Zed is what showed it.

Zed's Flatpak starts its editor outside the sandbox, on the host, where there
is no glibc; it did not start on either machine it was installed on. Kept
inside (`ZED_FLATPAK_NO_ESCAPE=1`, which the Store now sets, PR #135) it
starts, and three things are then true of it, and of Visual Studio Code's
Flatpak, which never leaves:

- Its terminal is the sandbox's, without `apk`, `doas` or anything installed
  on the system. `host-spawn` gives it the account's own shell outside, and
  the Store writes that setting for Zed.
- Its language servers and builds run inside, and cannot run the `cargo` or
  `go` from Alpine, which are built for musl. They need a second toolchain,
  a Flatpak SDK extension, beside the one already installed.
- Whatever the editor downloads for itself, a language server or a Node, is
  a glibc build, and runs only inside.

So an editor from Flathub edits, and is a poor place to build.

What else there is was looked at:

| | On musl | What it is for | Why not |
|---|---|---|---|
| `gcompat` | in part | | Rejected in ADR 0003. Best effort, and what runs on it has no sandbox. |
| AppImage | no | | Wants the host's glibc, and FUSE. |
| Snap | no | | Wants systemd. |
| Nix | yes | Command-line tools | A second package manager and a `/nix` to explain; its graphical programs need more for the graphics card. |
| Distrobox | yes | A whole glibc distribution, with the account's home | Below. |

Distrobox with rootless podman was tried on the X1 (Alpymist Full, Alpine
3.24, OpenRC, Intel UHD 620) on 2026-10-05. It took the two packages and two
things podman's own post-install message names:

- `/etc/subuid` and `/etc/subgid`, which did not exist, with one line each
  for the account. Without them an image cannot be unpacked.
- The `cgroups` service, which was not started. Without it podman falls back
  to cgroups v1 and says that is deprecated.

Nothing else: the kernel allows user namespaces, and `tun` and `fuse` were
loaded. In a box of Debian 13, made in 12 seconds and entered the first time
in 63:

- glibc 2.41 and `/lib64/ld-linux-x86-64.so.2`; the same user and home.
- A window from Debian's `foot` in Hyprland, and Debian's Mesa drawing on the
  Intel card, not on the processor.
- Zed's own build for Linux, unpacked into the box's `/opt`: it started,
  drew with Vulkan, downloaded its usual rust-analyzer and ran it against the
  `cargo` installed in the box. Its terminal was the box's `zsh`, in the
  project's directory.
- Visual Studio Code from Microsoft's `.deb`: a window on Wayland, and a
  task in its terminal that ran the box's `zsh` and built a project with the
  box's `cargo`. `distrobox-export` put it in the menu and took it out.
- The host from inside, through `distrobox-host-exec`.

It cost 15 packages, podman's 50 MiB among them, and 1.6 GB under
`~/.local/share/containers` for that one box.

## Decision

**Alpymist documents this and ships none of it.** The manual says how to set
a box up, what it is and is not, and how to take it away again. There is no
package that depends on podman or distrobox, no `alpymist` subcommand, no
switch in Settings, nothing in the installer and no entry in the Store's
configuration. Flatpak stays the one answer to glibc that is on every
system, as ADR 0003 has it, and `gcompat` stays rejected.

Why not a switch, when ssh and the guest's graphics have one:

- **It is not one thing to turn on.** A switch could write the two files,
  start the service and install the packages, and the user would then still
  have everything to decide: which distribution, which release, what to
  install in it, whether it shares the home directory. What can be automated
  is four commands; what cannot is the rest, and that is what needs
  explaining. A switch would look like more than it is.
- **It would be ours to keep working.** Podman, its network helper, the
  image registries and each distribution's images change on their own
  calendars. A page in the manual that goes stale says so by failing in front
  of the reader; a command that goes stale is a bug in Alpymist.
- **It is for few of the machines.** 1.6 GB and a container runtime are
  nothing on a developer's laptop and out of the question on the machines
  ADR 0001 calls Potato. Those who want it can follow a page.
- **It is not a sandbox, and should not be offered beside one.** A box
  shares the home directory, the session's Wayland socket and the graphics
  card. Anything in it can read and write what the account can. Flatpak's
  entry in the Store says "sandbox" and means something by it; a switch next
  to it would borrow the word.

What the manual has to say, so that the page is the feature:

- The four steps, with what each is for, and that the `cgroups` service is
  put in a runlevel or it is gone at the next boot.
- That a box is not a sandbox, in those words.
- That `sudo` in a box is root in the box and the account outside it, and
  why the sub-ids make that so.
- That a vendor's install script run in a box writes into the shared home:
  Zed's puts a `dev.zed.Zed.desktop` in `~/.local/share/applications`, which
  hides the Flatpak's entry behind one that cannot run on the host. Install
  into the box's own filesystem, and put it in the menu with
  `distrobox-export`.
- Which editor comes from where: the ones Alpine packages, which need none
  of this, and Visual Studio Code, VSCodium, Zed, JetBrains' IDEs and
  Sublime Text, from Flathub or in a box. Zed is where this started and is
  not what most people use.
- How to remove a box, and all of it.

## How this sits with the earlier decisions

**ADR 0003, Flatpak and no `gcompat`.** Unchanged, and this is not an
addendum to it: nothing it decided is reversed. The base system stays musl
with nothing of glibc installed into it. A box's glibc is in an image under
the account's home, as a Flatpak runtime's is.

**ADR 0011, secure by default.** Nothing is on by default, and nothing is
shipped that could be. What a reader sets up by hand is theirs: rootless
podman has no daemon and listens on nothing, and the sub-ids give an
account a range of user ids that are nobody's, which is not a privilege on
the host. The manual does not suggest podman as root, the `docker` group or
a socket.

**ADR 0002, the supply chain.** A box's image comes from a registry this
project has no part in, and the page says so. The packages are Alpine's.

**ADR 0023, dependencies.** None added.

## Consequences

- A developer on Alpymist has a second place for software, and the manual is
  the only thing that tells them. Someone who does not read it finds an
  editor from Flathub that cannot see their compilers, and no hint of why
  beyond the section the Store's Zed entry led to.
- The Store goes on installing Zed's Flatpak set up for its sandbox. That is
  the right default for looking at files and the wrong one for building, and
  the manual sends the second kind of reader on to the box.
- The page describes other people's software and will fall behind it. It
  names what was tried and when, so that a reader can tell.
- Tried on one machine, one architecture and one distribution's image, with
  two editors, Zed and Visual Studio Code. Not tried: the other editors the
  manual names in a box, a language server in Visual Studio Code, a reboot, a machine with an NVIDIA card, aarch64, a box with a home
  directory of its own, sound, or a build of anything large.
- Nothing stops a later decision to ship a switch. It would be an addendum
  here, and it would have to answer the four points above, the fourth most
  of all.
