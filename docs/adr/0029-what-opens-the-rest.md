# ADR 0029 — What opens the rest: a viewer, a player, an archiver, and a terminal's programs

**Status:** proposed · **Date:** 2026-10-07

## Context

Settings › Default applications said "Nothing installed" for pictures,
films, music and archives. A picture double-clicked in the file manager of
ADR 0028 opened in the browser, a zip in nothing.

There is also what someone who lives in a terminal installs in the first
hour. The desktop already brings two programs of that kind, `impala` and
`bluetuith`, because the bar opens them.

What there is was installed, or its installation simulated, on the
development machine (Alpine 3.24.2, aarch64) on 2026-10-07. Sizes are what
each adds to a system that has the desktop:

| For | | Adds | |
|---|---|---|---|
| Pictures | **Ristretto** | 0.3 MiB | GTK 3, on the libraries Thunar brought. |
| | imv, swayimg | 1 to 4.5 MiB | No toolkit: keys only, no menu, and no theme to follow. |
| | Loupe | 7 MiB | GTK 4 and libadwaita, which take no theme of ours (ADR 0007's addendum). |
| | qimgv | 148 MiB | Brings Qt. |
| Archives | **xarchiver**, with Thunar's plugin | 0.9 MiB | GTK 3. |
| | file-roller | 0.8 MiB | GTK 4 and libadwaita. |
| | engrampa | 19 packages | Brings MATE's libraries. |
| Films, music | **mpv** | 29 MiB | Nearly all of it ffmpeg, which nothing brought. |
| | Celluloid | 29 MiB | mpv in a GTK 4 window. |
| PDF | nothing | | LibreWolf opens them. zathura is 4.4 MiB with poppler and 52 with mupdf. |

| In a terminal | Adds | |
|---|---|---|
| **lazygit** | 21 MiB | One Go binary. gitui is 7.4 MiB and tig 0.6. |
| **cliamp** | 37 MiB | One Go binary; below. cmus is 8.7 MiB and has no radio. |
| **btop** | 1.4 MiB | |
| **fastfetch** | 2.1 MiB | |
| **tmux** | 1 MiB | |
| **ncdu** | 0.1 MiB | |
| fzf | 45 MiB | Its package brings Perl. |
| zellij, gdu, yazi, lazydocker | 13 to 22 MiB each | Each a second one of something. |

cliamp is not in Alpine. It is a player in Winamp's manner whose
<kbd>R</kbd> lists the Radio Browser directory's stations, which is what it
was asked for and what cmus lacks. Its release 2.3.0 was read for what it
does unasked:

- It listens on a unix socket in `~/.config/cliamp`, for its own `cliamp
  next` and the like, and on no network address while it plays.
- Signing in to Spotify, Qobuz or YouTube Music opens a port for the
  browser to answer on, for as long as the sign-in takes. YouTube Music's is
  opened on every address and not only the machine's own.
- `cliamp upgrade` downloads a release from GitHub over the program's own
  file. Nothing runs it but someone typing it, and nothing looks for a new
  version otherwise.

## Decision

**The desktop depends on Ristretto, xarchiver with `thunar-archive-plugin`
and `7zip`, and mpv.** 32 MiB together. `/etc/xdg/mimeapps.list` gives each
the kinds it opens, so Settings shows them where it said "Nothing
installed", and another can be chosen as for any kind. The viewer and the
archiver are GTK 3, so the stylesheet and the icons the theme writes
(ADR 0007's addendum of 2026-10-07) dress them as they do Thunar.

**mpv plays music as well as films.** One program for both, and it has no
library to keep: it plays what it is given.

**Nothing new opens a PDF.** LibreWolf does.

**A file is read by these when someone opens it, and not before.** That is
ADR 0028's reason for no thumbnails, and it stands: Ristretto, mpv and
7-Zip are parsers of files from anywhere, and what is new is only that
opening one no longer takes a trip to the Store.

**A terminal's programs are a package of their own, `alpymist-tui`, which
the desktop depends on.** lazygit, cliamp, btop, ncdu, tmux and fastfetch,
with `pipewire-alsa` and `ffmpeg` for cliamp: about 64 MiB. A dependency,
as `alpymist-tools` is, so that an upgrade brings them to every system and
the image carries them with the desktop. Leaving it for the installer to
add by name was the other way, and would have let `apk del` take them away
again; it would also have left every system there is without them until
someone typed a command. They cannot be removed while the desktop is
installed.

**They are in the menu.** btop and cliamp bring desktop entries;
`alpymist-tui` ships one each for lazygit, ncdu and tmux, which have none,
under names of our own, and the desktop one each for impala and bluetuith,
which it already brought. fastfetch prints and ends, and a terminal opened
for it would close before it was read.

**Ristretto shows no strip of thumbnails.** It asks tumbler for them, and
ADR 0028 left tumbler out. `/etc/xdg/xfce4/xfconf/xfce-perchannel-xml/ristretto.xml`
turns the strip off, and the message about the missing thumbnailer that
came with every picture opened. Both are defaults the account's own
settings override.

**cliamp is an aport of ours, pinned to a release.** `aports/cliamp` builds
upstream's tarball, held to its SHA-512 as squint's is, with the Go that
Alpine packages (`GOTOOLCHAIN=local`) and the modules the release's
`go.sum` names. It keeps its own version, so it is in both `INDEPENDENT`
lists and in `FETCHED`. Nothing of its source is changed.

## Consequences

- 96 MiB more on every desktop at its next upgrade.
- A third aport of somebody else's program to move forward by hand, and
  this one's upstream releases often. A release is a `pkgver` and a
  `sha512sums`; nothing tells us there is one.
- `cliamp upgrade` is there. An account cannot write `/usr/bin`, so it
  fails for one; as root it would put upstream's binary, built for glibc,
  over the package's. The package's file comes back with `apk fix cliamp`.
- cliamp's sign-ins are the user's to start, and YouTube Music's listens
  more widely than it should while it waits. That is upstream's to mend.
- The Xfce archiver plugin adds "Extract here" and "Create archive" to
  Thunar's menu.
- Ristretto opens PNG, JPEG, GIF, BMP, TIFF, SVG and WebP. AVIF and HEIF
  need loaders that are not installed, and those two kinds stay unassigned.
- mpv and the terminal's programs take no colours from the theme. lazygit,
  btop, cliamp and mpv each read theirs from a file the theme could write;
  that is separate work.
- Tried on the development machine: the four installed from Alpine's
  packages, cliamp built by `abuild` from the aport in an Alpine 3.24
  container for aarch64, and a build of the same release run there as far
  as listing its commands and themes.
  Ristretto opening a picture with the defaults in place, as one window
  and no message. Not tried: sound out of cliamp, the aport on x86_64, the
  image, and `alpymist-tui` as a package, which only CI builds.

## Addendum, 2026-10-07 — a card game, and it is ours

A desktop had no game at all. Alpine's with a window, `aisleriot`, is
67 MiB in five packages, 51 of them Guile, which nothing else here uses;
`tty-solitaire` is 65 KiB and a terminal's. Neither takes the theme.

**`alpymist-solitaire` is Klondike in a window of our own, and the desktop
depends on it.** A crate like the overview's: the rules with no pixels and
tests of their own, a view that lays the table out for any window, and the
program that joins them to one. Under 2 MiB, no dependency the workspace
did not have, and the table, the backs of the cards and the buttons in the
theme's colours.

**The faces are pictures, and somebody else's.** Byron Knoll's Vector
Playing Cards, which he released into the public domain, drawn once to
fifty-two PNGs of 300 by 436 and carried in the program, 750 KiB.
`crates/alpymist-solitaire/cards/README.md` says where they are from and
the commands that made them. Faces drawn by code would have followed the
theme and looked like nothing anyone has played with; faces made by an
image model were tried and came out with the wrong pips and letters.

Tried: the rules by their tests, the table as pictures drawn offscreen, and
the window opened on the development machine. Not tried by anyone but the
person playing: how dragging feels.
