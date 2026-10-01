# The menu
<!-- group: The desktop -->

The menu is where everything is. <kbd>Super</kbd>+<kbd>Space</kbd> opens it,
as does a click on the mark at the left end of the bar. Typing searches every
entry of every submenu, every installed application and every setting.

| Key | Does |
|---|---|
| Typing | Searches |
| <kbd>↑</kbd> <kbd>↓</kbd> | Move |
| <kbd>Enter</kbd> | Open the entry or submenu |
| <kbd>Esc</kbd> | Back one level, or close |

## What is in it

| Submenu | Holds |
|---|---|
| **Apps** | Every installed application. <kbd>Super</kbd>+<kbd>Alt</kbd>+<kbd>Space</kbd> opens here directly. |
| **Capture** | Screenshots: a region or the whole screen, to the clipboard or to a file. |
| **Toggle** | Switches: the top bar, do not disturb, the SSH server. |
| **Settings** | Open Settings, and a submenu for each area holding every setting in it. |
| **Setup** | Audio, Bluetooth, Wi-Fi, Power, and **Config**, which opens each of your configuration files in the editor. Reload desktop is here too. |
| **Install** | The Store, an application from Flathub by name, an Alpine package by name, the AI tools, and Jottacloud. |
| **Remove** | The same, the other way. |
| **Update** | Updates in the Store, everything at once, system packages or Flatpak applications alone, and the release channel. |
| **Learn** | About Alpymist, your keybindings, and the Alpymist, Hyprland, Alpine and Flathub sites. |
| **AI** | Claude Code, Codex and Gemini CLI, and the usage popup. See [AI](20-ai.md). |
| **About** | What the probe says of this machine. |
| **System** | Lock, suspend, log out, restart, shut down, and Fingerprints where a reader is set up. <kbd>Super</kbd>+<kbd>Esc</kbd> opens here directly. |

You can walk through Settings from the menu, but searching is quicker: type
"wallpaper", "lid" or "time zone" and the entry opens Settings at that
setting.

## Changing the menu

The menu is one TOML file. Setup › Config › Menu writes a copy of the
built-in one to `~/.config/alpymist/menu.toml` the first time, and opens it.
From then on that file is your menu.

```toml
[menu.root]
title = "Alpymist"
items = [
  { name = "Apps", icon = "󰀻", menu = "apps" },
  { name = "Notes", icon = "󰎞", exec = "alpymist open editor ~/notes.md" },
]
```

An entry has a `name`, an `icon` (a Nerd Font glyph), and either `menu`, the
name of a submenu, or `exec`, a command. These can be added:

| Field | Meaning |
|---|---|
| `terminal = true` | Run the command in a terminal |
| `hold = true` | Keep that terminal open afterwards, to read what it printed |
| `detail = "…"` | A few words shown beside the name |
| `keywords = ["…"]` | More words the search finds it by |

The menu is read from the first of these that exists:
`~/.config/alpymist/menu.toml`, `/etc/alpymist/menu.toml`, and the copy built
into the program. `alpymist-menu --print-config` prints the built-in one, and
`alpymist-menu --check` says whether a file is valid.

A file of your own replaces the built-in menu, so it does not gain entries
that a later release adds to the built-in one. If your file does not parse,
the menu says so and shows the built-in entries, so a typo never leaves you
without a way to shut down.

To add entries without taking the menu over, put a fragment in
`/etc/alpymist/menu.d/`: a TOML file holding only `[menu.NAME]` tables, whose
items are added to the menus of those names. That is how packages add
theirs, from `/usr/share/alpymist/menu.d/`: Settings, the AI tools and
Fingerprints all arrive that way, and they are merged into a menu of your own
as well. `--check` reads your file alone, so it complains of a submenu that
only a fragment provides, such as `settings` or `ai`, although the menu loads
it.

## Opening a submenu from a key

`alpymist-menu NAME` opens at a submenu, which is what the System and Apps
keys do:

```
bind = SUPER, P, exec, alpymist-menu capture
```
