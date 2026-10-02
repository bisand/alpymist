# Hotkeys
<!-- group: Start here -->

Every key Alpymist binds, in one place. They are ordinary lines in
`~/.config/hypr/hyprland.conf`, which is yours to change: Setup › Config ›
Hyprland in the menu opens it. Learn › Keybindings in the menu lists the ones
your own file has, apart from the lid, the power and sleep keys and
<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Delete</kbd>.

## Opening things

| Key | Does |
|---|---|
| <kbd>Super</kbd>+<kbd>Space</kbd> | The menu |
| <kbd>Super</kbd>+<kbd>Alt</kbd>+<kbd>Space</kbd> | The menu, opened at Apps |
| <kbd>Super</kbd>+<kbd>Esc</kbd> | The menu, opened at System |
| <kbd>Super</kbd>+<kbd>Return</kbd> or <kbd>Super</kbd>+<kbd>Q</kbd> | A terminal |
| <kbd>Super</kbd>+<kbd>B</kbd> | The web browser |

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

## What is not bound

Alpymist does not yet bind the volume, brightness or media keys. Volume is
on the bar's speaker icon and in Settings › Sound. You can bind them yourself
in `hyprland.conf`; Hyprland's wiki has the lines.

## Changing a key

Open `~/.config/hypr/hyprland.conf` and edit the `bind =` line, or add one:

```
bind = SUPER, E, exec, alpymist open files
```

Hyprland reads the file again as soon as it is saved. A line of your own
comes after Alpymist's packaged ones, so yours wins.
