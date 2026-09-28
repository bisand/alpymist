# ADR 0014 — Copy and paste on Super, and a history kept only when asked

**Status:** accepted · **Date:** 2026-09-28

## Context

Copy and paste differ by window: a terminal takes Ctrl+Shift+C and
Ctrl+Shift+V, since Ctrl+C interrupts and Ctrl+V quotes the next key, and
everything else takes Ctrl+C and Ctrl+V. On Wayland, what was copied belongs
to the window that copied it, so it is gone when that window closes, and
nothing remembers what was copied before.

A history fixes that, and keeps everything copied: passwords from a password
manager, tokens pasted into a terminal, keys. ADR 0011 says nothing that
keeps a secret in the clear is on by default.

## Decision

**Super+C and Super+V work in every window.** `alpymist clipboard copy` and
`paste` ask Hyprland which window has the focus and send it the shortcut it
understands. Terminals are known by their desktop entries (`TerminalEmulator`,
matched to the window's class), not by a list here. Floating a window moves
from Super+V to Super+Shift+F. The screenshot key puts the picture on the
clipboard as well as in a file.

**The history is off until Settings › Clipboard turns it on,** and says what
it keeps. When on:

- It is our own small daemon, `alpymist clipboard daemon`, fed by
  `wl-paste --watch`. `clipman`, the one Alpine packages, always writes its
  history to a file, keeps only text, and cannot pin.
- It is kept in memory, on a socket in the session's runtime directory. Only
  if Settings also says so is it kept across logins, in
  `$XDG_STATE_HOME/alpymist/clipboard`, readable by the account alone.
- What a password manager marks as secret is never taken: wl-clipboard
  reports it (`CLIPBOARD_STATE=sensitive`), and the type
  `x-kde-passwordManagerHint` is checked too.
- The lock screen has it forget all but pinned entries, unless Settings says
  to keep them. Pinned entries are the person's to forget.
- Text up to 1 MiB and images up to 16 MiB are kept; the size in Settings
  bounds how many, pinned ones aside.

The picker is the menu: `alpymist-menu clipboard`, on Super+Shift+V. Enter
pastes, Ctrl+S pins, Delete forgets.

**Older accounts get the keys the theme's way (ADR 0007).** The first change
in Settings › Clipboard turns `bind = SUPER, V, togglefloating`, if still as
shipped, into the four new keys, and the screenshot key into the one that
copies. The file is kept beside it as `hyprland.conf.bak-clipboard`, and
Hyprland is reloaded. If Super+C, Super+Shift+V or Super+Shift+F is already
the account's own, nothing is changed.

## Consequences

- Nothing is kept by default; turning the history on is a choice made on a
  page that says what it keeps.
- An image in the picker is a line naming its size, not a picture of it yet.
- A session's history dies with the session unless remembered, and a crash
  of the daemon loses it too.
