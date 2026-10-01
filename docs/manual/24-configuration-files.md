# Configuration files
<!-- group: System -->

Settings covers what most people change. Beyond it, the desktop is made of
ordinary programs with ordinary configuration files, and those files are
yours. This chapter is the map.

## The rule

There are three kinds of file, and it helps to know which you are looking at:

| Kind | Where | Who changes it |
|---|---|---|
| **Yours** | `~/.config/hypr/hyprland.conf`, `~/.zshrc` and the others below | You. An upgrade never touches them. |
| **Settings' own** | Under `~/.config/alpymist/` and `/etc/alpymist/` | Settings and `alpymist set`: small TOML files holding the values, and the files generated from them, which say so in their first line. |
| **Packaged** | Under `/usr/share/alpymist/` | Upgrades. Do not edit these; override them from a file of yours. |

Your files include the packaged ones, so you get fixes and new features
through upgrades without losing your changes, and a line of your own always
wins because it comes last.

If you edit a generated file by hand, Alpymist sees that it no longer matches
what it wrote, tells you, and leaves it alone.

One exception to "never touches": on an account made before the theme, the
default applications or the clipboard keys existed, the first change to one
of those in Settings rewrites the one line of your file that it needs, and
keeps the file as it was beside it, as `.bak-theme`, `.bak-defaults` or
`.bak-clipboard`.

## Your files

Setup › Config in the menu opens each of these in the editor.

| File | What it configures |
|---|---|
| `~/.config/hypr/hyprland.conf` | Hyprland: keys, gaps, borders, animations, what starts at login |
| `~/.config/waybar/hyprland.jsonc` | The top bar's modules |
| `~/.config/waybar/style.css` | The top bar's look |
| `~/.config/alpymist/menu.toml` | The menu, once you have taken it over |
| `~/.config/foot/foot.ini` | The terminal |
| `~/.zshrc` | The shell |
| `~/.config/starship.toml` | The prompt |
| `~/.config/mako/config` | Notifications. Generated until you edit it. |

A new account gets them from `/etc/skel`.

## What Settings writes

| File | Holds |
|---|---|
| `~/.config/alpymist/settings.toml` | Your account's settings |
| `~/.config/alpymist/power.toml` | What the lid and the power button do, and what the bar shows of the battery |
| `~/.config/alpymist/wallpaper.toml` | The wallpaper |
| `~/.config/alpymist/screensaver.toml` | The screensaver and the idle timings |
| `~/.config/alpymist/clipboard.toml` | The clipboard history's settings |
| `~/.config/alpymist/ai-usage.toml` | Which AI providers are on. Never a key. |
| `~/.config/alpymist/displays.toml` | The layout for each set of screens |
| `~/.config/alpymist/theme.toml` | Style, accent, fonts |
| `~/.config/alpymist/hypr/settings.conf` | The touchpad, mouse, key repeat and theme, as Hyprland reads them |
| `~/.config/alpymist/hypr/displays.conf` | The screens, as Hyprland reads them |
| `~/.config/alpymist/waybar/colours.css`, `foot/theme.ini` | The theme, as the bar and the terminal read it |
| `~/.config/mimeapps.list` | Default applications |
| `~/.config/xdg-terminals.list` | The default terminal |
| `/etc/alpymist/` | System settings: the keyboard layout among them |

## What the package provides

| File | Holds |
|---|---|
| `/usr/share/alpymist/hyprland-security.conf` | What programs may do, <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Delete</kbd>, and the services the desktop starts |
| `/usr/share/alpymist/hyprland-windows.conf` | Which windows float as dialogs |
| `/usr/share/alpymist/waybar/` | The bar as shipped |
| `/usr/share/alpymist/menu.d/` | Menu entries that packages add |
| `/usr/share/alpymist/screensavers/` | The screensavers installed |
| `/usr/share/alpymist/ai-usage/` | The AI usage providers installed |
| `/usr/share/backgrounds/alpymist/` | Wallpapers |

## Common changes

**Gaps and borders.** In `hyprland.conf`:

```
general {
    gaps_in = 2
    gaps_out = 4
    border_size = 1
}
```

**Blur and shadows**, off as shipped for the sake of slow machines:

```
decoration {
    blur {
        enabled = true
    }
}
```

**No animations:**

```
animations {
    enabled = false
}
```

**A window that should always float:**

```
windowrule = match:class ^(pavucontrol)$, float on, center on
```

**Something to start at login:**

```
exec-once = nextcloud --background
```

Hyprland reads `hyprland.conf` again as soon as it is saved, and Setup ›
Reload desktop does it by hand. Hyprland's own wiki, under Learn in the menu,
has everything the file can say.

## Taking your configuration to another machine

Everything above under "Your files" and `~/.config/alpymist/` can be kept in
a git repository or copied across. The generated files are rebuilt from the
TOML ones at login, so those are the ones worth keeping.

## Starting again

To put one file back as shipped, copy it from `/etc/skel`:

```sh
cp /etc/skel/.config/hypr/hyprland.conf ~/.config/hypr/hyprland.conf
```

To put one setting back, `alpymist reset ID`.
