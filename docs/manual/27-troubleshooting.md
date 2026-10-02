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

## Screenshots do nothing

`grim` and `slurp` take them. Run the command from a terminal to see what it
says:

```sh
grim -g "$(slurp)" ~/test.png
```

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
GitHub account.

On a machine with no network, `alpymist report --save report.txt` writes it
to a file to carry to one that has. The live image has the command too, so
a machine that cannot be installed to can still be reported: save onto the
stick. `doas alpymist report --save report.txt` adds the firmware the kernel
log names, which only root may read.

## Starting over with one program's settings

Each program's settings are one file or one directory under `~/.config`.
Moving it away and logging in again gives that program its settings as
shipped, and you still have the old ones to look at.
