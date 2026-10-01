# Welcome to Alpymist
<!-- group: Start here -->

Alpymist is a desktop on top of Alpine Linux: Hyprland, a tiling window
manager on Wayland, with a bar, a menu, a Settings app and a set of tools
chosen to work together. It is one desktop, configured once, and it arrives
as signed packages rather than as a script that changes your system.

Three ideas run through all of it, and knowing them makes the rest of this
manual shorter:

- **The keyboard comes first.** Everything has a key or can be found by
  typing a few letters into the menu on <kbd>Super</kbd>+<kbd>Space</kbd>.
  The mouse works everywhere too.
- **Every setting has three ways in.** The Settings app, the `alpymist`
  command, and the menu's search all change the same thing. Use whichever is
  nearest.
- **Nothing unsafe is on until you turn it on.** No automatic login, no
  passwordless administrator, no service listening on the network. Where a
  convenience costs some safety, there is a switch for it that says what it
  gives up.

Alpymist is young. It installs and it runs, and it has rough edges. Where
this manual knows of one, it says so.

## How to read this manual

Read [Getting started](02-getting-started.md) and
[Finding your way around](03-finding-your-way-around.md) in order. After
that, every chapter stands on its own.

- To change something, look in the chapter for that part of the desktop, or
  in [Every setting](26-every-setting.md), which lists them all.
- To learn the keys, see [Hotkeys](04-hotkeys.md).
- To do it from a terminal, see [The command line](25-the-command-line.md).

<kbd>Super</kbd> is the key with the Windows logo on most keyboards, and
<kbd>Cmd</kbd> on a Mac's.

## If something here is wrong

The manual is written from the same repository as the code, and each page
has a link to its source. A correction is a pull request like any other.
