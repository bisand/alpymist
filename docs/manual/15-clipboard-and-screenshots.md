# Clipboard and screenshots
<!-- group: The desktop -->

## Copy and paste, the same everywhere

Terminals and other programs disagree about which keys copy and paste.
Alpymist gives you one set that works in every window:

| Key | Does |
|---|---|
| <kbd>Super</kbd>+<kbd>C</kbd> | Copy |
| <kbd>Super</kbd>+<kbd>X</kbd> | Cut |
| <kbd>Super</kbd>+<kbd>V</kbd> | Paste |

Each sends the focused window the keys it understands:
<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> to a terminal,
<kbd>Ctrl</kbd>+<kbd>C</kbd> to anything else. The usual keys still work as
they always did.

## Clipboard history

A history of what you copied is useful, and it also keeps every password and
token you copy. So it is **off** until you turn it on, in Settings ›
Clipboard:

| Setting | Default | Meaning |
|---|---|---|
| Keep a history | Off | Keep what you copy, to paste again |
| Keep it after logging out | Off | Keep it in a file only you can read, rather than in memory only |
| Entries kept | 50 | The oldest go first. Pinned entries are not counted. |
| Forget it when the screen locks | On | Forget all but pinned entries at every lock |
| Clear the history | | Forget everything but pinned entries, now |

With it on, <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> opens the history
in the menu:

| Key | Does |
|---|---|
| Typing | Searches |
| <kbd>Enter</kbd> | Pastes the entry |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> | Pins it, so it is never forgotten |
| <kbd>Delete</kbd> | Forgets it |

What a password manager marks as secret is never kept. Text up to 1 MiB and
images up to 16 MiB are; an image shows as a line naming its size.

```sh
alpymist set clipboard.history true
alpymist clipboard list
alpymist clipboard pin ID
alpymist clipboard clear
```

## Screenshots

<kbd>Print</kbd> lets you drag out a region. It is saved as
`screenshot-<date>-<time>.png` in your home directory and put on the
clipboard as well, ready to paste.

Capture in the menu has the four combinations:

| Entry | Takes | Goes to |
|---|---|---|
| Region to clipboard | A region you drag | The clipboard |
| Region to file | A region you drag | A file in your home directory |
| Screen to file | The whole screen | A file in your home directory |
| Screen to clipboard | The whole screen | The clipboard |

Press <kbd>Esc</kbd> while choosing a region to give up.

### Sharing your screen

Browsers and Flatpak applications share a screen through the desktop portal,
which asks you first. Apart from that and the screenshot tool, no program is
allowed to read what is on the screen; see [Security](22-security.md).
