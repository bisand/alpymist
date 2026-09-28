# ADR 0007 — Settings: one command line, one library, one app

**Status:** accepted · **Date:** 2026-09-15 · **Amends:** [ADR 0002](0002-supply-chain.md)

## Context

Alpymist's settings are wherever they were first needed. The installer writes
the keyboard layout, time zone and hostname once, and after that they are
changed by hand with doas. Nothing sets the touchpad, the monitors or the
language. Power and Wi-Fi have good popups under the bar, each with its own
logic. The menu's appearance lives in the menu's own configuration file.

ADR 0002 said system changes go through `alpymistctl`, writing `/etc/alpymist`
and generating backend configuration from it. Until now only the release
channel did.

People need one place to change what matters, found as quickly as an
application: by keyboard, from the menu's search.

## Decision

### 1. `alpymist` is the command line for every setting

`alpymistctl` is renamed `alpymist`. It is how any setting is read or changed,
by a person in a terminal, by the settings app, and by the popups:

```sh
alpymist list [AREA] [--json]         # every setting, its value and its choices
alpymist get keyboard.layout
alpymist set keyboard.layout no
alpymist set touchpad.natural-scroll true
alpymist reset touchpad.natural-scroll
alpymist channel dev                  # as before
alpymist probe                        # as before
```

A package-provided `alpymistctl` link keeps the old name working for one
release: the live image, the first-boot probe and anyone's habits.

### 2. One library behind it: `alpymist-settings`

The command line is thin. The settings are a library, so the command line,
the settings app and the popups share one implementation and nothing is written
twice:

- A **registry**: every setting has an id (`area.name`), a title, a
  description, search keywords, a kind (switch, choice, number in a range,
  text), a scope (the account or the system) and whether a change applies at
  once or at the next login.
- **Areas** read and write their settings through the domain crates that
  already exist: power through `alpymist-power`, Wi-Fi through `alpymist-wifi`.
  Their popups keep their own look and move their writes onto the same
  functions.

### 3. Where values live, and hand edits

- **Account settings** are TOML under `~/.config/alpymist/` (`power.toml`
  already is). What a program reads is generated from them into files of
  Alpymist's own that the account's configuration includes, as the top bar
  already does: `~/.config/alpymist/hypr/settings.conf`, sourced by the
  packaged Hyprland configuration, never an edit to `hyprland.conf`.
- **System settings** are under `/etc/alpymist/`, generated into what Alpine
  reads (`/etc/conf.d/`, `/etc/alpymist/hyprland-keyboard.conf`,
  `session.env`).
- A generated file says so in its first line and carries a hash of what was
  written. If the file no longer matches, it was edited by hand: `alpymist`
  says so and leaves it, unless told `--force`.
- Live where the program allows it (`hyprctl keyword`, iwd over D-Bus, a
  signal); otherwise the setting says when it takes effect.

### 4. Root

A system setting needs root. `alpymist set` run as a person re-runs itself as
`pkexec /usr/bin/alpymist set ID VALUE`. polkit's `org.alpymist.settings.system`
asks an administrator's password, in `alpymist-auth`'s dialog, and keeps it a
few minutes. Run as root it writes. Nothing else of the caller reaches it:
pkexec clears the environment, and a value is checked against the setting's
kind before anything is written.

### 5. The settings app: `alpymist-settings`

- One window, drawn with DeniseUI's widgets and theme on the CPU, as every
  Alpymist window is: a search field, the areas down the side, a page of
  settings beside them. Pages are built when first opened.
- **Keyboard first.** Typing searches every setting; arrows move through areas
  and results; Tab moves into a page; Enter and Space change what has focus;
  Escape goes back. Everything works with the mouse.
- **Deep links.** `alpymist-settings ID` opens at a page or a setting and
  focuses it; a second run hands the id to the open window, which comes
  forward.
- Pages may say more than the popups: the popups stay as they are, for the
  quick change under the bar.

### 6. The menu finds settings

The menu reads fragments from `/usr/share/alpymist/menu.d/*.toml`, each adding
entries or submenus like its own configuration does. `alpymist-settings` ships
`settings.toml`, generated at build time from the registry, with an entry per
setting that runs `alpymist-settings ID`. So "natural scroll" typed in the menu
finds Settings › Touchpad › Natural scrolling, and the ids cannot drift from
the app. Anything else can add entries the same way.

### 7. One theme

`~/.config/alpymist/theme.toml`, then `/etc/alpymist/theme.toml`, then a
built-in default: dark or light, the seed colours, the fonts and their size.
It becomes a DeniseUI `Theme` for the settings app, and the colours and fonts
the existing hand-drawn windows use (menu, popups, store, About), so a change in
Settings › Appearance changes all of them. The top bar, foot, mako, Hyprland's
borders and GTK's dark preference follow from the same file later.

### 8. DeniseUI grows where it should

What any DeniseUI program would want (a theme read from a file, a layout
helper, a missing widget) is added to DeniseUI and released, not worked around
in Alpymist.

## Phases

1. `alpymist` and the settings library; theme.toml; the menu's fragments; the
   app, with Appearance, Keyboard, Touchpad & mouse, Wi-Fi, Power, Updates and
   About.
2. Displays, Date & time, Language, Sound, Notifications, Default apps,
   Hostname & accounts, Bluetooth, the desktop tier and login screen; the theme
   reaching the top bar, foot, mako, Hyprland and GTK.
3. The popups and the hand-drawn windows drawn with DeniseUI's widgets and
   theme where that looks at least as good, and their logic fully on the shared
   library.

## Consequences

- One implementation per setting, testable without a display, and scriptable.
- Hand edits are never lost silently, but a generated file edited by hand stops
  following the settings app until it is reset.
- Renaming `alpymistctl` touches the live image, the installer and the docs
  once.
- The menu gains a general mechanism it did not have, and parses a few more
  entries at start: a small TOML file, well inside a frame.

---

## Addendum, 2026-09-25 — the wallpaper, and the one line of an account's own file it changes

**Status:** accepted · makes an exception to §3.

Settings › Appearance › Wallpaper chooses the desktop's picture from those
installed in `/usr/share/backgrounds/alpymist/`. The choice is the account's,
in `~/.config/alpymist/wallpaper.toml`, and `alpymist wallpaper` puts it on
the screen: a new `swaybg` on the picture, and then the old one stopped, or
`feh` on X11. Each tier's configuration runs that at login, and Settings runs
it after a change.

§3 says a setting reaches a program through a file of Alpymist's own that the
account's configuration includes, "never an edit to `hyprland.conf`". The
wallpaper cannot keep to that for the accounts that already exist. They start
`swaybg` or `feh` themselves, from `hyprland.conf`, labwc's `autostart` or
i3's `config`, and labwc and i3 include nothing of Alpymist's. A setting that
could not reach them would change the picture until the next login and then
silently put the old one back.

So the first time an account's wallpaper is changed, the line in those files
that starts `swaybg` or `feh` is replaced by the one that runs `alpymist
wallpaper`. That one line and nothing else: comments, binds and every other
program are left alone. The file as it was is kept beside it, as `….bak-wallpaper`,
and the change is said in a note when it is made. It happens only because the
person changed their wallpaper, never on an upgrade, and once it is done there
is nothing left for it to find. Accounts made from now on start with the line
already there.

---

## Addendum, 2026-09-27 — mako's configuration is generated whole

**Status:** accepted · makes an exception to §3.

Settings › Notifications sets where notifications appear and for how long, and
switches do not disturb. §3 would have those in a file of Alpymist's own that
`~/.config/mako/config` includes. mako 1.11 has `include=`, but it refuses to
start at all when an included file is missing: tried on the dev VM, it says
"Failed to parse config" and exits. An account whose include line arrived
before the file it names — a new account, a restored home directory, a file
deleted by hand — would have no notifications, and nothing to say why.

So `~/.config/mako/config` itself is the generated file, with the usual header
and hash. The one every account started with, from the skeleton, is taken over
as it stands; one edited by hand is left alone, and Settings says so rather
than changing it, as for any generated file. `alpymist session` writes it
before the compositor starts, so the first login after an upgrade already has
it.

Do not disturb is mako's `do-not-disturb` mode, switched in the running mako
with `makoctl`, and lasts until it is turned off or the session ends. The mode
only hides anything because the generated file gives it `invisible=1`. The
skeleton's file had no such section, so the menu's toggle, which has switched
that mode since the desktop was first configured, has had nothing to switch.

---

## Addendum, 2026-09-27 — the theme, and the one line of each of three of an account's files

**Status:** accepted · makes an exception to §3, as the wallpaper's addendum does.

§7 says the top bar, foot, mako, Hyprland's borders and GTK's dark preference
follow the theme "later". They now do, through files of Alpymist's own written
from the account's `theme.toml`: `~/.config/alpymist/waybar/colours.css`,
`~/.config/alpymist/foot/theme.ini`, GTK 3's and 4's `settings.ini` where the
account has none of its own, and the border colours in the Hyprland
`settings.conf` the input settings already write. GTK's `color-scheme` and
`accent-color` go through `gsettings`, which libadwaita and Flatpak's apps
follow at once.

Three of them reach a program only through a file that is the account's:
waybar's `style.css` has to import the colours, foot's `foot.ini` has to
include the palettes, and Hyprland's `hyprland.conf` set its own border
colours after sourcing Alpymist's, so its won. Accounts made from now on have
the import and the include, and no border colours of Hyprland's own. Accounts
made before cannot follow the theme without a change to those files.

So the first time an account changes its scheme or accent, each of those files
that is still in the shape Alpymist gave it gets that one change and nothing
else: the import after the line that imports Alpymist's style, the include at
the top of `[main]`, and Hyprland's two border lines taken out only when they
are still the colours every account started with; colours of the account's
own are left. The file as it was is kept beside it, as `….bak-theme`, and the
change is said in a note. It happens only because the person changed their
theme, never on an upgrade.

waybar and foot both refuse to start when a file they import or include is
missing: tried on the dev VM, waybar exits on a missing `@import` and foot
calls a missing include a configuration error. `alpymist session` writes
every one of these files before the desktop starts, at every login, so a line
that names one never names nothing.

mako's colours are not here yet. They belong in the mako configuration that
Settings › Notifications generates, per the addendum above, and come with a
change of their own.
