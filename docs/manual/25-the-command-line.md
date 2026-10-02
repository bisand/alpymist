# The command line
<!-- group: Reference -->

Everything the desktop does has a command behind it, and each command says
what it takes with `--help`.

## alpymist

The one command for settings and for the system.

| Command | Does |
|---|---|
| `alpymist list [AREA or ID] [--json]` | Lists settings, their values and titles; with one setting's id, its choices too |
| `alpymist get ID` | Prints a setting's value |
| `alpymist set ID VALUE` | Changes a setting. A system setting asks for an administrator's password. `--force` replaces a generated file that was edited by hand. |
| `alpymist reset ID` | Puts a setting back to its default |
| `alpymist probe` | Says how well this machine can run Hyprland, and why |
| `alpymist report [--save FILE] [--issue]` | Prints what this machine is made of and which drivers have it, and sends nothing. `--save` writes it to a file; `--issue` offers to open it as a GitHub issue in the browser. See [Troubleshooting](27-troubleshooting.md). |
| `alpymist channel [stable or dev]` | Shows or changes the release channel |
| `alpymist guest [on or off]` | Shows or changes whether a virtual machine takes its graphics driver from the guest repository |
| `alpymist open CATEGORY [ARGS]` | Opens something with the application chosen for it |
| `alpymist displays …` | The screens: `list`, `set`, `save`, `apply`, `forget`, `workspace N`, `move N`, `overview` |
| `alpymist clipboard …` | `copy`, `cut`, `paste`, and the history: `list`, `restore`, `pin`, `unpin`, `forget`, `clear`, `start`, `stop` |
| `alpymist wallpaper` | Puts the chosen wallpaper on the screen |
| `alpymist autostart [--dry-run]` | Starts the programs that start at login |
| `alpymist watchdog --list` | Lists what the session keeps watch over: the screens, and the bar |
| `alpymist firmware check` | As root: installs firmware a driver asked for and did not find |
| `alpymist firmware broadcom [--to DIR or --from DIR]` | Fetches the firmware Broadcom's older Wi-Fi cards need, which no package may hold. `--to` puts it on a stick for a machine with no network, and `--from` takes it from one. |

`alpymist session` starts a desktop session and is what the login screen
runs, and `alpymist watchdog` alone is what Hyprland runs at login; neither
is for running by hand.

## The desktop's programs

Run with no arguments, each opens its window or popup. Run again, a popup
closes.

| Command | Opens | And also |
|---|---|---|
| `alpymist-menu [MENU]` | The menu, or one of its submenus | `--print-config`, `--check [FILE]` |
| `alpymist-settings [AREA or ID]` | Settings, at an area or a setting | |
| `alpymist-store` | The Store | `installed`, `updates`, `search`, `install`, `remove`, `update`, `refresh` |
| `alpymist-wifi` | The Wi-Fi popup | `status`, `on`, `off`, `toggle`, `scan` |
| `alpymist-power` | The power popup | `status`, `profile [MODE]`, `suspend`, `hibernate`, `restart`, `power-off` |
| `alpymist-ai-usage` | The AI usage popup | `list`, `status`, `enable`, `disable`, `key`, `forget`, `refresh`, `tool run`, `tool install`, `tool remove` |
| `alpymist-screensaver` | The screensaver, now | `list`, `stop` |
| `alpymist-lock` | The lock screen | |
| `alpymist-fingerprint` | The fingerprint window | |
| `alpymist-about` | The About box | `--print` |
| `alpymist-thunderbolt` | | `list`, `forget UUID` |
| `alpymist-jottacloud` | | `install`, `remove` |

## In scripts

`alpymist get` prints only the value, and `alpymist list --json` gives every
setting with its kind, choices, default and scope:

```sh
if [ "$(alpymist get ssh.server)" = true ]; then
    echo "the SSH server is on"
fi

alpymist list --json | jq -r '.[] | select(.scope == "system") | .id'
```

`jq` is not installed as shipped: `doas apk add jq`.

A system setting changed from a script that already runs as root needs no
prompt: `doas alpymist set datetime.timezone Europe/Oslo`.

## The tools underneath

Alpymist's commands sit on standard ones, which work as they do anywhere:

| For | Command |
|---|---|
| Packages | `apk`, `flatpak` |
| Services | `rc-service NAME start`, `rc-update add NAME` |
| Hyprland | `hyprctl` |
| Wi-Fi | `iwctl` |
| Sound | `wpctl`, `pavucontrol` |
| Bluetooth | `bluetuith`, `bluetoothctl` |
| Notifications | `makoctl` |
| Screenshots | `grim`, `slurp` |
| Clipboard | `wl-copy`, `wl-paste` |

Alpine uses OpenRC, not systemd: there is no `systemctl`.
