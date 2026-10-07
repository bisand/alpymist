# ADR 0028 — A file manager: Thunar, with drives opened by hand

**Status:** accepted · **Date:** 2026-10-07

## Context

Settings › Default applications said "Nothing installed" for folders, and a
keyboard's Files key ran `alpymist open files` into nothing, or into
whatever else had said it opens folders: on a machine with Visual Studio
Code from the Store, an editor.

What there is was installed, or its installation simulated, on the
development machine (Alpine 3.24.2, aarch64) on 2026-10-07. Sizes are what
each adds to a system that has the desktop:

| | Adds | Why not |
|---|---|---|
| Thunar, alone | 3.8 MiB | Below. |
| pcmanfm | about the same | Built for X11, and no longer developed. |
| xfe | 10 MiB | X11 only. |
| spacefm | 20 MiB | No longer maintained. |
| COSMIC Files | 42 MiB | Drawn by the graphics card, which the slowest machines and a guest without one pay for. |
| Nautilus | 44 MiB | Brings GNOME's indexer, and takes no theme. |
| Nemo | 47 MiB | Brings Cinnamon's libraries. |
| pcmanfm-qt | 108 MiB | Brings Qt, for one program. |
| nnn, lf, yazi | 0.2 to 13 MiB | A terminal's. Good ones, and not what Files means to someone with a mouse. |

One of our own, drawn with Denise, was the other way. It is weeks of work to
reach what Thunar does the first day: copying with progress and conflicts,
the trash, drives, dragging between windows. And the dialog other programs
open to choose a file would still be GTK's, since a portal is spoken to on
the session bus, which `alpymist-dbus` does not do (ADR 0025).

GTK 3 is already installed, for LibreWolf. A file manager with a trash and a
list of drives needs gvfs and udisks2 whichever it is.

## Decision

**Thunar is the file manager, and the desktop depends on it.** With it come
`gvfs`, for the trash and the list of drives, and `udisks2`, which mounts
them: 29 MiB and 46 packages together. `/etc/xdg/mimeapps.list` says Thunar
opens folders, so that is what Automatic comes to, and another can be chosen
in Settings as for any kind. <kbd>Super</kbd>+<kbd>E</kbd> runs
`alpymist open files` in a new account's `hyprland.conf`; an account that
exists keeps the file it has.

**Folders open as a list.** `/etc/xdg/xfce4/xfconf/xfce-perchannel-xml/thunar.xml`
sets Thunar's `default-view`. It is a default, which Thunar's own
preferences override for the account.

**A drive is mounted when it is opened, never when it is plugged in.**
`thunar-volman`, which mounts on arrival, is not installed. Nothing is
mounted at the login screen or behind a locked one, because nothing there
asks (ADR 0011, §1).

**Whoever is at the machine may open a drive they plugged in without a
password.** udisks2 asks polkit, and its own policy says yes to an active
session. Without elogind polkit knows no sessions, so here that came to an
administrator's password, asked of a process that `alpymist-auth` cannot
speak for: a stick would simply not open. `49-alpymist-files.rules` says yes
to the `seat` group, as the power rules do, for four actions: mounting a
filesystem, unlocking an encrypted one, ejecting, and powering a drive off.
udisks2 asks about those only for a drive that is not the system's. The
machine's own disks, formatting, partitioning and loop devices still ask for
an administrator's password. A drive is mounted `nosuid` and `nodev`, which
is udisks2's doing and not ours.

This is the one thing here that ADR 0011 has to be read against. It gives
the account nothing it could not do by other means with the stick in its
hand, and no root: what it adds is that a program running as the account can
have the kernel read a filesystem somebody plugged in. Every desktop that
mounts a stick at a click has that; ours does not have it at the plug.

**gvfs's root helper is not started without a password.** gvfs ships a rule
letting an administrator in an active session start `gvfsd-admin`, which is
root, unasked. No session is active here, so today the helper is refused
outright, which is safe and stays. The same file answers before gvfs's rule
does, by its name, and says to ask where that rule would say yes, so that
the day polkit does know a session here, nothing becomes passwordless
because of it.

**No thumbnails.** `tumbler` is not installed. It brings 137 MiB, most of it
WebKit for the covers of e-books, and it would have a parser read every
picture, film and document in a folder that was only looked at.

**No network places by default.** `gvfs-smb`, for Windows shares, is 25 MiB
of Samba, and `gvfs-mtp` is for phones. Both are packages someone adds.

## Consequences

- One more root process, `udisksd`, started by the system bus when a program
  asks for the drives, and not at boot. It listens on nothing but the bus.
- Thunar looks like GTK: Adwaita, dark as the rest of GTK is here, and not
  like Alpymist's own windows. A theme of ours for GTK is separate work.
- Thunar's sidebar offers "Browse Network", which shows nothing until
  `gvfs-smb` is installed.
- The Xfce libraries Thunar needs come with it: exo, libxfce4ui, xfconf.
- Tried on the development machine: Thunar as a Wayland window under
  Hyprland, the trash, the list of drives, the default read from
  `/etc/xdg`, and what polkit answers with the rules in place. Not tried: a
  real stick, an encrypted one, x86_64, and the override's own case, which
  needs a session polkit calls active.
- If a file manager of our own is ever written, this is what it replaces,
  and the rules for drives stay as they are.
