# Troubleshooting
<!-- group: Reference -->

Things that go wrong, and what to do about each. If yours is not here, the
[issue tracker](https://github.com/bisand/alpymist/issues) is the place, with
the output of `alpymist-about --print` and `alpymist probe`.

## The desktop is slow in a virtual machine

Without the host's graphics card, Hyprland draws every frame on the
processor. Check what it is drawing with:

```sh
alpymist probe
```

If `gles` names `llvmpipe`, turn on guest graphics and restart:

```sh
doas alpymist guest on
```

The virtual machine must also offer 3D acceleration: in UTM or QEMU, a
virtio-gpu display with GL enabled. See
[Updates and channels](21-updates-and-channels.md).

## Logging in goes straight back to the login screen

The session could not start. The login screen says that the desktop stopped
and offers a console on <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>F2</kbd>. Log in
there, and the end of the session's log is printed: it is
`~/.local/state/alpymist/session.log`, and it usually names the cause. Then:

- `rc-service seatd status`: the desktop opens the screen and keyboard
  through `seatd`. Start it with `doas rc-service seatd start`.
- `id`: the account must be in the `seat` group. An account added by hand
  may not be; see [Accounts, language and time](23-system.md).
- `alpymist probe`: if it says *Unlikely*, Hyprland may not be able to start
  on this machine's graphics at all.

## Updates, the Store and web pages all fail at once

Look at the clock. A clock that is wrong by years, as after the battery has
been flat, makes every secure connection fail. Set it and turn network time
on:

```sh
doas date -s "2026-10-01 12:00"
alpymist set datetime.network-time true
```

## Wi-Fi is missing

The card may need firmware that is not installed. Connect by cable or by a
phone's USB tethering, then:

```sh
doas alpymist firmware check
```

and restart. `dmesg | grep -i firmware` shows what a driver asked for and did
not find.

### A Broadcom card in an older laptop

If a notification said the Broadcom Wi-Fi card needs firmware, or
`alpymist report` lists a `network` device with the driver `b43-pci-bridge`:
that firmware is in no package, because Broadcom lets nobody pass it on, and
it is not on Alpymist's image for the same reason. What Broadcom does publish
is its own driver for routers, with the firmware inside it. Every way below
downloads that driver from OpenWrt's mirrors, to the machine that asks, checks
that it is exactly the file expected, and cuts the firmware out there. Nobody
passes the firmware on to anybody, which is how every other distribution does
it too.

**With a network on that machine**, by cable or a phone's USB tethering:

```sh
doas alpymist firmware broadcom
```

and the card shows its networks a moment later.

**With no network on that machine**, the firmware is fetched by another
machine onto a stick. Any stick with room for a megabyte will do, and the
stick Alpymist is installed from does best, if it was made by copying files
and not with `dd`, as [for a Mac](02-getting-started.md#for-a-mac): the
installer looks on the stick it runs from, and a new installation then has
Wi-Fi from its first start.

On another Alpymist machine, with the stick at `/media/usb`:

```sh
alpymist firmware broadcom --to /media/usb
```

On anything else, the same is a script, `broadcom-firmware.sh`. It is at the
top of an install stick made by copying files, and at
<https://alpymist.org/broadcom-firmware.sh>. Run from the stick, it writes to
the stick:

```sh
sh /Volumes/ALPYMIST/broadcom-firmware.sh        # macOS
sh /media/usb/broadcom-firmware.sh               # Linux
```

or, downloaded, with the stick's directory named:

```sh
curl -fsSLO https://alpymist.org/broadcom-firmware.sh
sh broadcom-firmware.sh /Volumes/ALPYMIST
```

It needs `curl` or `wget`, and either `b43-fwcutter`, which most Linux
distributions package under that name, or a C compiler to build it with,
which on macOS is `xcode-select --install`. On Windows, run it in WSL, with
the stick's drive letter in place of `e`: `sh broadcom-firmware.sh /mnt/e`.
A macOS too old to make a secure connection to today's servers cannot
download anything; use another machine.

Either way leaves a directory `alpymist-firmware/b43` on the stick, and
nothing else is changed. If Alpymist is already installed, take it from there:

```sh
doas alpymist firmware broadcom --from /media/usb
```

The live system on the install stick does not use the firmware: Wi-Fi works
from the installed system's first start, not in the installer.

## A control key does nothing

The volume, brightness, media and radio keys are bound for every machine;
see [Hotkeys](04-hotkeys.md#the-control-keys). When one does nothing:

- Run what the key runs, in a terminal: `alpymist key brightness-up`,
  `alpymist key volume-up`. If that works, the key is not reaching the
  desktop. Hold <kbd>Fn</kbd> with it, or look for an *Fn lock*; on a Mac the
  top row is these keys without <kbd>fn</kbd>. Some keys never reach the
  system at all: most laptops light their keyboard, and some switch their
  radios, in their own firmware.
- If it says there is *no screen brightness to change*, the kernel found no
  backlight on this machine: `ls /sys/class/backlight` is empty. That is the
  graphics driver's, and worth an issue with `alpymist report`.
- If it says the account *may not change that light*, the account is not in
  the `video` group (`id` lists them; `doas adduser NAME video`, and log in
  again), or the machine has not been restarted since the upgrade that
  brought the keys.
- The play and next keys act on a program that says it is playing. With
  nothing playing they do nothing.

## A dock's keyboard, mouse or network does nothing

The dock is waiting to be allowed. Its screens work regardless, which makes
this confusing. Log in using the laptop's own keyboard; the question should
appear, and *Always allow* stops it being asked again. See
[Security](22-security.md).

## Settings says a file was edited by hand

Alpymist does not write over a generated file that someone has changed. Either
keep your edit and make that change by hand from now on, or tell it to
replace the file:

```sh
alpymist set notifications.position top-right --force
```

## A change to hyprland.conf broke something

Hyprland shows a red bar along the top naming the line it could not read.
Fix that line, or put the file back as shipped:

```sh
cp /etc/skel/.config/hypr/hyprland.conf ~/.config/hypr/hyprland.conf
```

<kbd>Super</kbd>+<kbd>Return</kbd> for a terminal still works unless that
very line is the broken one; a console on
<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>F2</kbd> always does.

## The menu complains and shows the built-in entries

Your `~/.config/alpymist/menu.toml` does not parse, so the menu fell back to
the built-in one.

```sh
alpymist-menu --check
```

says what is wrong with it. Moving the file away keeps the built-in menu.

## The bar is gone

Toggle › Top bar in the menu brings it back. If it does not stay, start it
from a terminal to see why:

```sh
waybar -c ~/.config/waybar/hyprland.jsonc -s ~/.config/waybar/style.css
```

## The bar shows the wrong workspace

The bar follows Hyprland, and can be cut off from it if it falls far behind.
Alpymist notices within a minute and starts the bar again, which shows as the
bar blinking once. Toggle › Top bar in the menu, twice, does the same at
once.

## An application asks to "use weaker encryption"

It has not found the keyring. Choose no, log out and in again, and try once
more. If it was told yes before, take that choice back out of the
application's own settings; see [Security](22-security.md).

## An SSH key asks for its passphrase again and again

It asks once per session and again after each lock, by design: a locked
machine holds no unlocked key. A key with no passphrase is never asked for.

## An application from Flathub does not start

Run it from a terminal, with `flatpak run` and its id, to see why. One that
says "No such file or directory" about a program that is there has stepped
out of its sandbox, where there is no glibc for it. See
[A Flathub application built for glibc](17-installing-software.md#a-flathub-application-built-for-glibc).

## Screenshots do nothing

`grim` and `slurp` take them. Run the command from a terminal to see what it
says:

```sh
grim -g "$(slurp)" ~/test.png
```

If it says nothing and does not come back, Hyprland is holding it for a
permission nobody can give: a fault in Hyprland 0.54 that a screenshot taken
while the screen was changing can set off, and that lasts until you log out.
Press <kbd>Ctrl</kbd>+<kbd>C</kbd> and try again; most go through.

## The pointer moves but cannot be seen

Some graphics drivers show nothing where Hyprland puts the pointer. On the
nouveau driver, for older Nvidia cards, Alpymist has Hyprland draw it itself.
On another, try it for this session:

```sh
hyprctl keyword cursor:no_hardware_cursors true
```

If that brings it back, put `cursor { no_hardware_cursors = true }` in
`~/.config/hypr/hyprland.conf` to keep it, and tell us with a hardware
report, below, so the next machine like yours needs neither.

## Something in the machine does not work

A Wi-Fi card that is not found, a touchpad that does nothing: tell us, and
the next release may have it. This prints what the machine is made of and
which driver has each part:

```sh
alpymist report
```

It sends nothing. What it prints has the make and model, the devices by
their makers' numbers, the drivers, and the firmware that was asked for and
not found. It has no serial number, no network address, no host name and no
account name. To send it:

```sh
alpymist report --issue
```

shows the report, asks, and then opens the browser at a new issue on GitHub
with the report filled in. Say there what does not work, and submit it;
until you do, nothing has left the machine. An issue is public, and needs a
GitHub account. *Hardware report* in Settings › System does the same.

On a machine with no network, `alpymist report --save report.txt` writes it
to a file to carry to one that has. The live image has the command too, so
a machine that cannot be installed to can still be reported: save onto the
stick. `doas alpymist report --save report.txt` adds the firmware the kernel
log names, which only root may read.

## Starting over with one program's settings

Each program's settings are one file or one directory under `~/.config`.
Moving it away and logging in again gives that program its settings as
shipped, and you still have the old ones to look at.
