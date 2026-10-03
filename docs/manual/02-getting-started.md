# Getting started
<!-- group: Start here -->

There are two ways onto an Alpymist system: start a computer from the
installer image, or add Alpymist's package repository to an Alpine Linux
system you already have. Neither is a script piped into a shell.

## What you need

- A 64-bit PC or laptop (x86_64), or a virtual machine. An aarch64 image
  exists for virtual machines on Apple silicon Macs, such as UTM.
- A graphics card with a working Linux driver. The installer tells you how
  Hyprland will do on the machine before anything is written; see
  [what the probe says](#what-the-probe-says) below.
- A disk you are willing to erase. The installer takes a whole disk. It does
  not install beside another operating system.
- A network connection is not required, but updates need one, and firmware
  for some Wi-Fi cards is fetched during the install.

## Download and write the image

Every release has an image for each architecture attached, with a checksum
beside it. Get both from the
[latest release](https://github.com/bisand/alpymist/releases/latest), and
check the download with both files in the same directory:

```sh
sha256sum -c alpymist-*.iso.sha256
```

Write the image to a USB stick. On Linux or macOS, with the stick's device in
place of `/dev/sdX`, and with care, since this erases the stick:

```sh
sudo dd if=alpymist-<version>-x86_64.iso of=/dev/sdX bs=4M status=progress
```

Then start the computer from the stick. Alpymist does nothing to support
Secure Boot: if the machine refuses to start from the stick, turn Secure Boot
off in its firmware settings.

### For a Mac

An Intel Mac lists what it can start from when Option is held down as it
starts, and a stick written with `dd` may not be on that list. Make the stick
with a FAT partition instead, and copy the image's files onto it. This erases
the stick too.

On Linux, with the stick's device in place of `/dev/sdX`:

```sh
printf 'label: gpt\ntype=uefi\n' | sudo sfdisk --wipe always /dev/sdX
sudo mkfs.vfat -F 32 -n ALPYMIST /dev/sdX1
mkdir image stick
sudo mount -o loop,ro alpymist-<version>-x86_64.iso image
sudo mount /dev/sdX1 stick
sudo cp -r image/. stick/
sudo umount image stick
```

On macOS, `diskutil list` says which disk the stick is. With it in place of
`/dev/diskN`:

```sh
diskutil eraseDisk FAT32 ALPYMIST GPT /dev/diskN
tar -xf alpymist-<version>-x86_64.iso -C /Volumes/ALPYMIST
diskutil eject /dev/diskN
```

Hold Option as the Mac starts, and choose *EFI Boot*.

A stick made this way starts any machine with UEFI firmware, not only a Mac,
and can be written to afterwards. It does not start one that boots the older
BIOS way; `dd` is for those.

## The installer

The stick boots straight into the installer. It asks, in order:

| Step | What it asks |
|---|---|
| Keyboard | The layout your keyboard has. Type to search. |
| Region | Your time zone, for the clock. Type a city or country. |
| Network | A wired connection is used as it is. For Wi-Fi, choose a network and type its password. You can install without one. |
| Disk | Which disk to use. All of it is erased, and you are asked to say so. |
| Encryption | Whether to encrypt the disk, and the passphrase. |
| Account | Your name, a login name, a password, and the computer's name. |
| Desktop | How Hyprland will do on this machine, and why. |
| Confirm | Everything you chose. Nothing has been written until you continue from here. |

Then it installs, showing its progress, and says when it is done.

### Encryption

With encryption on, everything but a small boot partition is encrypted, and
you type the passphrase every time the machine starts. Nobody can recover the
disk without it, you included. Without encryption, anyone who has the disk
can read it. For a laptop, turn it on.

### In a virtual machine

On a machine whose display is virtio-gpu, the Desktop step has one more
switch: *Draw with the host's graphics card in this virtual machine*. Left
on, the installed system takes its graphics driver from a second package
repository, and Hyprland draws on the host's graphics card rather than on the
processor, which is the difference between a smooth desktop and a slow one.
See [Updates and channels](21-updates-and-channels.md) for what that
repository is and how to turn it off.

### What the probe says

| Answer | Meaning |
|---|---|
| **Runs** | A graphics card with a real driver, GL ES 3.0 or newer, and at least 3 GiB of memory. Everything as it is meant to be. |
| **Slow** | A software renderer, or less than 3 GiB of memory. It runs, and the installer says why it will not be quick. |
| **Unlikely** | No usable display driver, only a firmware framebuffer, or GL ES older than 3.0. Hyprland may not start. |

Nothing is refused on the answer. On *Unlikely* the installer warns you and
lets you go on. `alpymist probe` gives the same answer on an installed
system.

## On an existing Alpine system

Alpymist is built against Alpine 3.24, and the repository serves only that
release. Trust the key, add the repository, and install:

```sh
doas wget -O /etc/apk/keys/alpymist-2026.rsa.pub https://pkgs.alpymist.org/alpymist-2026.rsa.pub
echo https://pkgs.alpymist.org/v3.24/alpymist | doas tee -a /etc/apk/repositories
doas apk update
doas apk add alpymist
alpymist probe
doas apk add alpymist-desktop
```

The [download page](https://alpymist.org/download/) shows the key's fingerprint to check before
you trust it. The installer also enables `dbus`, `seatd` and `greetd` and
points the login screen at the session; on an existing system that part is
still yours to do.

## The first login

The login screen lists the accounts on the machine. Choose yours, type your
password, and press <kbd>Enter</kbd>. <kbd>F11</kbd> restarts and
<kbd>F12</kbd> powers off from there.

You arrive at an empty desktop with a bar along the top. Press
<kbd>Super</kbd>+<kbd>Space</kbd> for the menu, or
<kbd>Super</kbd>+<kbd>Return</kbd> for a terminal, and read on.
