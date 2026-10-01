# Updates and channels
<!-- group: System -->

An installed system is updated in place. There is never a reason to install
again from a new image.

## Updating

| Menu entry | Does |
|---|---|
| Update › Updates | Opens the Store at what has updates |
| Update › Everything | System packages, then Flatpak applications |
| Update › System packages | `doas apk upgrade -U` |
| Update › Flatpak applications | `flatpak update --user` |

```sh
doas apk upgrade -U
flatpak update --user
```

Nothing updates by itself. Restart after an update that brings a new kernel.

## Which version is this

Learn › About Alpymist in the menu shows the version, the channel, the system
and the packages. `alpymist-about --print` prints the same, which is what to
paste into a bug report.

## Channels

A system follows one of two channels:

| Channel | Moves | For |
|---|---|---|
| **stable** | At each release | Everyone. What the installer sets up. |
| **dev** | On every change to the main branch | Trying what is coming. It may break. |

```sh
alpymist channel                 # which one this system follows
doas alpymist channel dev        # follow dev, trust its key, upgrade
doas alpymist channel stable     # back: distrust the key, downgrade to the release
```

Update › Release channel in the menu does the same. `--no-upgrade` only
switches, and leaves the upgrade for later. Settings › Updates only switches
too: it changes which repository the system follows, and the upgrade is then
yours to run. Going back to stable that way, a plain `apk upgrade` does not
downgrade; use `doas alpymist channel stable`.

Going back to stable downgrades the Alpymist packages to the last release,
since dev's are newer. The dev channel's key is trusted only on systems that
chose dev, and is removed again on the way back.

## Graphics in a virtual machine

Alpine 3.24's graphics drivers lack the one that lets a virtual machine draw
with its host's graphics card. Alpymist publishes Alpine's Mesa with that
driver added, in a repository of its own that a system follows only when
asked. In a virtual machine with a virtio-gpu display the installer offers
it; afterwards:

```sh
alpymist guest               # whether this system follows it
doas alpymist guest on       # follow it, trust its key, upgrade
doas alpymist guest off      # stop, distrust it, back to Alpine's Mesa
```

It does nothing for a machine that is not a virtual machine. A system that
follows it needs that repository to answer for `apk upgrade` to work, as it
needs its channel's.

## What an update is trusting

Every package comes from a repository whose index is signed, and the index
pins the hash of every package, so apk refuses one that was swapped or
changed. Both channels are built and signed by GitHub Actions from the public
repository. The [download page](https://alpymist.org/download/) says what that does and does not
guarantee.
