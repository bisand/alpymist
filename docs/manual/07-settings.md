# Settings
<!-- group: The desktop -->

Everything that can be set in Alpymist is one setting in one list, and there
are three ways to reach it. They all change the same thing.

## The Settings app

Search the menu for what you want to change and press <kbd>Enter</kbd>, or
run `alpymist-settings`. The window has a search field, the areas down the
side, and a page of settings beside them.

| Key | Does |
|---|---|
| Typing | Searches every setting |
| <kbd>↑</kbd> <kbd>↓</kbd> | Move through areas and results |
| <kbd>Tab</kbd> | Moves into the page |
| <kbd>Enter</kbd> or <kbd>Space</kbd> | Changes what has the focus |
| <kbd>Esc</kbd> | Clears a search or closes a dialog; otherwise closes Settings |

A change is made as soon as you make it. There is no Apply, except on the
Displays page, where a wrong choice could leave you without a picture.

The areas are Appearance, Displays, Keyboard, Language, Touchpad, Mouse &
pointer, Sound, Wi-Fi, Bluetooth, SSH, Power, Notifications, AI usage,
Clipboard, Default applications, Startup, Screensaver, Date & time, Updates
and System.

`alpymist-settings touchpad` opens at an area, and
`alpymist-settings touchpad.natural-scroll` at one setting.

## The command line

```sh
alpymist list                       # every setting, its value and its title
alpymist list power                 # one area
alpymist list keyboard.layout       # one setting, with its choices
alpymist get keyboard.layout
alpymist set keyboard.layout no
alpymist set touchpad.natural-scroll true
alpymist reset touchpad.natural-scroll
```

`alpymist list --json` prints the same as JSON, for scripts.
[Every setting](26-every-setting.md) is that list, with what each one means.

In a terminal, <kbd>Tab</kbd> finishes what you have begun: a command, a
setting's id, and after the id the values it takes.

## The popups

Wi-Fi, power and AI usage each have a popup under the bar for the quick
change. They are the same settings as the pages in the app.

## Yours, or everyone's

Each setting belongs either to your account or to the system.

- **Account settings** change only your desktop: the wallpaper, the
  touchpad, the screensaver. They need no password.
- **System settings** change the machine for everyone: the keyboard layout,
  the time zone, the computer's name, Bluetooth, the SSH server. Changing one
  asks for an administrator's password, in Alpymist's own prompt, and
  remembers it for a few minutes.

## When a change takes effect

Most apply at once. A few say otherwise: the language applies at the next
login, the style and accent colour to windows opened afterwards, and the
release channel at the next update. `alpymist list` and the Settings page say
which.

## Where the values are kept

You never need these, but they are plain files:

| What | Where |
|---|---|
| Your account's settings | TOML files under `~/.config/alpymist/` |
| System settings | Under `/etc/alpymist/` |
| What Hyprland reads of them | `~/.config/alpymist/hypr/settings.conf`, generated |

Files Alpymist generates say so in their first line. If you edit one by hand,
Alpymist notices that it no longer matches what it wrote, says so, and leaves
your edit alone rather than writing over it; `alpymist set ID VALUE --force`
replaces it. Your own `hyprland.conf` is
never generated: anything you set there comes after Alpymist's and wins.
[Configuration files](24-configuration-files.md) has the whole map.
