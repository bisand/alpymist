# Hotkeys
<!-- group: Start here -->

Every key Alpymist binds, in one place. They are ordinary lines in
`~/.config/hypr/hyprland.conf`, which is yours to change: Setup › Config ›
Hyprland in the menu opens it. Learn › Keybindings in the menu lists the ones
your own file has, apart from the lid, the power and sleep keys and
<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Delete</kbd>. The keyboard's
[control keys](#the-control-keys) come from the package and are not in your
file.

## Opening things

| Key | Does |
|---|---|
| <kbd>Super</kbd>+<kbd>Space</kbd> | The menu |
| <kbd>Super</kbd>+<kbd>Alt</kbd>+<kbd>Space</kbd> | The menu, opened at Apps |
| <kbd>Super</kbd>+<kbd>Esc</kbd> | The menu, opened at System |
| <kbd>Super</kbd>+<kbd>Return</kbd> or <kbd>Super</kbd>+<kbd>Q</kbd> | A terminal |
| <kbd>Super</kbd>+<kbd>B</kbd> | The web browser |
| <kbd>Super</kbd>+<kbd>E</kbd> | The file manager |

The terminal and the browser are the ones chosen in Settings › Default
applications.

## Windows

| Key | Does |
|---|---|
| <kbd>Super</kbd>+<kbd>W</kbd> | Close the window |
| <kbd>Super</kbd>+<kbd>F</kbd> | Full screen, and back |
| <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> | Float, and back |
| <kbd>Super</kbd>+<kbd>←</kbd> <kbd>→</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Move the focus |
| <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>←</kbd> <kbd>→</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Move the window that way, and on to the next screen from the edge |
| <kbd>Super</kbd>+<kbd>Alt</kbd>+<kbd>←</kbd> <kbd>→</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Take the window straight to the screen on that side |
| <kbd>Super</kbd> + left drag | Move the window |
| <kbd>Super</kbd> + right drag | Resize the window |
| Drag the edge of a window | Resize it |
| <kbd>Super</kbd>+<kbd>Ctrl</kbd>+<kbd>←</kbd> <kbd>→</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Resize the window, for as long as it is held |
| <kbd>Super</kbd>+<kbd>J</kbd> | Turn side by side into over and under, and back |

## Workspaces

| Key | Does |
|---|---|
| <kbd>Super</kbd>+<kbd>1</kbd> … <kbd>9</kbd> | Go to workspace 1 to 9 of this screen |
| <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>1</kbd> … <kbd>9</kbd> | Take the window there |
| <kbd>Super</kbd>+<kbd>Tab</kbd> | Show every workspace of this screen, to pick one from |

## Copy, paste and screenshots

| Key | Does |
|---|---|
| <kbd>Super</kbd>+<kbd>C</kbd> | Copy, in any window, terminals included |
| <kbd>Super</kbd>+<kbd>X</kbd> | Cut |
| <kbd>Super</kbd>+<kbd>V</kbd> | Paste |
| <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> | The clipboard history, when it is turned on |
| <kbd>Print</kbd> | Select a region: saved to a file in your home directory and put on the clipboard |

See [Clipboard and screenshots](15-clipboard-and-screenshots.md).

## Session

| Key | Does |
|---|---|
| <kbd>Super</kbd>+<kbd>L</kbd> | Lock the screen |
| <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> | Log out, without asking |
| <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Delete</kbd> | Ask whether the password prompt on screen is really Alpymist's |
| The power button | What Settings › Power says: the System menu by default |
| The sleep key | Lock and suspend |
| Closing the lid | What Settings › Power says: suspend by default |

<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Delete</kbd> is explained in
[Security](22-security.md). Unlike the others it comes from the package, not
from your own file, so it is not lost in an edit of yours.

## On the login screen

| Key | Does |
|---|---|
| <kbd>F11</kbd> | Restart |
| <kbd>F12</kbd> | Power off |

## The control keys

The keys with a picture on them, on whatever keyboard has them: a laptop's
top row, usually with <kbd>Fn</kbd> held, or a row of their own on a desktop
keyboard. A Mac's top row is these keys without <kbd>fn</kbd>, and F1 to F12
with it. Each says what it did in a notification at the corner of the
screen, and all but the last group work on the lock screen too.

| Key | Does |
|---|---|
| Volume up, volume down | The volume, 5% at a time, to 100% and no further. Up also unmutes |
| Mute | Mute the sound, and back |
| Microphone mute | Mute the microphone, and back |
| Brightness up, brightness down | The screen's light, in smaller steps as it gets dim, and never all the way to dark |
| Keyboard light up, down, on/off | The keyboard's light, where the system is what sets it: a Mac's |
| Play/pause, next, previous, stop | Whatever is playing: a browser's tab, a music player |
| Wi-Fi | Wi-Fi off, and on again |
| Bluetooth | Bluetooth off, and on again, once it is turned on in Settings › Bluetooth |
| Airplane mode | Wi-Fi and Bluetooth off; again, and those it switched off are back |
| Settings (a cog) | Settings |
| Display | Settings › Displays |
| Search | The menu |
| Home, web | The web browser |
| Files | The file manager |
| Lock | Lock the screen |
| A Mac's F3 and F4 | Every workspace of this screen, and the menu's applications |

A key that does nothing at all may be one the keyboard handles itself, as
most laptops do their keyboard light, or one the kernel has no name for; see
[Troubleshooting](27-troubleshooting.md#a-control-key-does-nothing).

These come from the package, in `/usr/share/alpymist/hyprland-keys.conf`, so
an upgrade keeps them current. Each runs `alpymist key`, which can be run by
hand or bound to any other key:

```
bind = SUPER, F12, exec, alpymist key volume-up
```

`alpymist key --help` lists them.

## Changing a key

Open `~/.config/hypr/hyprland.conf` and edit the `bind =` line, or add one:

```
bind = SUPER, N, exec, alpymist open editor
```

Hyprland reads the file again as soon as it is saved. A line of your own
comes after Alpymist's packaged ones, so yours wins.

Hyprland runs every line a key has, so to give a key of the package's another
job, take the package's away first:

```
unbind = , XF86AudioPlay
bind = , XF86AudioPlay, exec, my-player --toggle
```
