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
