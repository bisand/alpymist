# Security
<!-- group: System -->

Alpymist's rule is that what it installs is safe with nobody touching it, and
that anything which weakens that is a switch you turn on yourself, which says
what it gives up and turns off again. This chapter is what that means in
practice.

## How it ships

| | As installed |
|---|---|
| The `root` account | Locked. Nobody logs in as root. |
| Administration | Through `doas`, for administrators, with your password |
| Login | Asks for a password, or for an enrolled finger where that is turned on. There is no automatic login. |
| Network services | None listening. The SSH server is not installed until turned on. |
| Bluetooth | Off |
| Thunderbolt and USB4 devices | Asked about before they are let in |
| USB sticks, cards and other drives | Opened when you open them in Files, never when plugged in |
| Clipboard history | Off |
| A fingerprint, at the lock screen, at administrator prompts and at login | Off, each |
| Secrets | In a keyring encrypted with your login password |
| Locking when the screen turns off | Off: see below |

That last one is the default to change on any machine that leaves the house:
Settings › Screensaver › *Lock when the screen turns off*. Disk encryption is
chosen in the installer and cannot be added afterwards without installing
again.

## Becoming administrator

`doas` runs one command as root and asks for *your* password:

```sh
doas apk add htop
```

It remembers the password for a few minutes in that terminal. Only accounts
that are administrators may use it; Settings › System › *Administrator* is
the switch.

When a window needs administrator rights, such as Settings changing the time
zone or the Store installing an Alpine package, a prompt appears in the
middle of a dimmed screen and asks for an administrator's password.

## Is this prompt real?

A program could draw something that looks like that prompt to collect your
password. Press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Delete</kbd> whenever you
are asked for a password and want to be sure: the desktop itself tells you,
in a notification, whether the prompt on screen is Alpymist's. Programs never
see the key: Hyprland takes it first.

It is not proof against everything. A program that knows to rebind the key
through Hyprland's own control socket, which every program you run can reach,
could defeat it. It stops a lookalike drawn by anything that does not.

The same goes for the prompt that asks for an SSH key's passphrase.

## The keyring

Programs that save a password or a sign-in, such as browsers, VS Code, git
and `gh`, keep it in your keyring, which is encrypted with your login
password and unlocked when you log in. You never see it.

Two consequences:

- Change your password with Settings › System › *Password*, or `passwd`, so
  the keyring's changes with it.
- An application told earlier to "use weaker encryption" keeps doing so until
  that choice is taken out of its own settings. For VS Code that is the
  `password-store` line in `~/.vscode/argv.json`.

## SSH keys

An SSH key with a passphrase is asked for once, the first time you use it,
and stays unlocked for the session. It is forgotten when you lock the screen
or log out, and no passphrase is ever written to disk.

To be asked before every use of a key as well, add `AddKeysToAgent confirm`
to `~/.ssh/config`.

## Docks and other Thunderbolt devices

A Thunderbolt or USB4 device can reach the computer's memory, so one that has
not been seen before waits until you allow it. A dialog names the device and
offers:

| Choice | Meaning |
|---|---|
| Don't allow | It stays out. This is the highlighted button. |
| Allow once | Until it is unplugged |
| Always allow | Now, and whenever it comes back |

Allowing asks for an administrator's password. Screens on a dock work without
being allowed; its keyboard, network and other devices do not.

A device is never let in while the screen is locked or at the login screen,
unless it can prove who it is. So a laptop started on its dock with the lid
shut may not have the dock's keyboard at the login screen: open the lid.

```sh
alpymist-thunderbolt list          # the devices, and what each is allowed
alpymist-thunderbolt forget UUID
```

## Sticks, cards and other drives

A drive you plug in appears in the file manager's sidebar and is opened when
you click it there; nothing is mounted when it is plugged in, at the login
screen or while the screen is locked. Opening and ejecting one asks for no
password from someone sitting at the machine. An encrypted drive asks for
its own passphrase.

The machine's own disks are another matter: mounting one that is not
already, formatting and partitioning ask for an administrator's password.

Folders show no thumbnails, which is deliberate: making them would mean
reading every picture and document in a folder you had only opened.

## What programs may do to the desktop

- **Reading the screen** is allowed to the screenshot tool, and to the
  portal that browsers and Flatpak applications share a screen through, which
  asks you first. Nothing else may.
- **Typing as if it were a keyboard** is denied to every program.
- **Hyprland plugins** are denied, all but one: hyprexpo, the overview that
  <kbd>Super</kbd>+<kbd>Tab</kbd> opens, which is Hyprland's own and comes
  from Alpine's package. It is allowed by the file it is,
  `/usr/lib/libhyprexpo.so`, which only root can change.
- **Flatpak applications** run in a sandbox, with access to what each
  declares.

These rules are in `/usr/share/alpymist/hyprland-security.conf`, which your
`hyprland.conf` sources. Taking that line out removes all of it.

## Where the software comes from

Everything Alpymist installs is a signed package from Alpine or from
Alpymist, or a Flatpak from Flathub. See
[Updates and channels](21-updates-and-channels.md) for what the signature
covers.

## Reporting a problem

A security problem goes through
[SECURITY.md](../../SECURITY.md), not the public issue tracker.
