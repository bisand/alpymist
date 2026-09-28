# ADR 0013 — A keyring unlocked at login, and an SSH agent that forgets

**Status:** accepted · **Date:** 2026-09-28

## Context

Nothing in an Alpymist session kept secrets. Programs that store a password
or a token ask the Secret Service (`org.freedesktop.secrets`) for somewhere to
put it, and there was none. What they do then differs, and none of it is
good:

- **Chromium and every Electron app** (VS Code among them) choose where they
  keep secrets by which desktop they believe they are on. Hyprland is not one
  they know, and on a desktop they do not know they use `basic_text`: a file
  encrypted with a key built into the program, which is to say in the clear.
  VS Code first offers "Use weaker encryption", and on the dev VM that had
  been clicked: its `~/.vscode/argv.json` said `"password-store": "basic"`.
- **libsecret apps** fail to save, or ask every time.
- **git and ssh** asked for a key's passphrase on every connection, since no
  agent ran.

ADR 0011 says no secret is kept in the clear by default. The first two are
exactly that, by default, the moment anyone signs in to anything.

## Decision

**gnome-keyring is the Secret Service, unlocked at login.** It is what
libsecret, VS Code and Flatpak's Secret portal are tested against. Alpine's
base PAM stack already calls `pam_gnome_keyring` wherever the module is
installed: `auth` hands it the password typed at the login screen, `session
… auto_start` starts the daemon, and `password … use_authtok` changes its
password when the login's changes. So `alpymist-desktop` depends on
`gnome-keyring` and `gnome-keyring-pam`, and writes no PAM configuration of
its own. The keyring is encrypted with the login password, on disk in
`~/.local/share/keyrings`.

PAM runs before the session's bus exists; `dbus-run-session` makes that bus
around `alpymist session`. So `alpymist session` runs
`gnome-keyring-daemon --start --components=secrets`, which puts the daemon PAM
started, already unlocked, on that bus. The Secret portal is gnome-keyring's
(`hyprland-portals.conf`), for sandboxed apps that use it rather than the bus.

**Chromium and Electron are told they are on GNOME's keyring by
`DESKTOP_SESSION=gnome`,** set by `alpymist session` for the whole session.
They read it only when `XDG_CURRENT_DESKTOP` names a desktop they do not know,
as Hyprland is to them; tried on the dev VM, VS Code's Flatpak went from
`basic_text` to `gnome_libsecret` with nothing else changed. Setting
`XDG_CURRENT_DESKTOP=Hyprland:GNOME` would do the same, but far more programs
read that variable and would start behaving as if on GNOME; nothing is
configured app by app, which would never keep up. An app someone already told
to use `basic` — as the dev VM's VS Code was — keeps doing so until that line
is taken out of its own configuration.

**git keeps HTTPS credentials there too:** `/etc/gitconfig`, which no Alpine
package owns, sets `credential.helper` to `libsecret`, and the desktop brings
`git-credential-libsecret`. An account's own `~/.gitconfig` can name another
helper. `gh` stores its token in the Secret Service by itself once there is
one.

**The login screen does not get a keyring.** greetd runs it as its own
`greetd` account, through the same PAM stack, so `base-session` started a
keyring daemon for that account too, before anyone logged in. The greeter has
its own PAM service, `alpymist-greeter`, set per session in greetd's
configuration: the runtime directory and the keyboard layout a login gets, and
`base-session-noninteractive`, which starts nothing else.

**`passwd` is shadow's,** which goes through PAM. busybox's writes
`/etc/shadow` itself, so the keyring would keep the old password and ask for
it at the next login. Settings' password button runs `passwd`, and gets
shadow's.

**The SSH agent is OpenSSH's `ssh-agent`, the one that keeps nothing.** It
holds a key only in memory, refuses to be traced or dumped, and stores no
passphrase anywhere. gcr's agent would remember passphrases in the keyring, so
a key would unlock itself at every login after the first. That is more
convenient, and it puts every key's passphrase on disk behind the login
password. So:

- `alpymist session` starts the compositor under `ssh-agent -a
  $XDG_RUNTIME_DIR/ssh-agent.socket`. Given a command, the agent runs it and
  exits when it does, so the keys end with the session.
- `/etc/ssh/ssh_config.d/alpymist.conf` says `AddKeysToAgent yes`: a key's
  passphrase is asked the first time it is used in a session, and not again
  until the session ends or the screen locks.
- The passphrase is asked in OpenSSH's own dialog, from `openssh-askpass`
  (`SSH_ASKPASS`, with `SSH_ASKPASS_REQUIRE=prefer`), from a terminal as from
  an editor: one way of being asked. gcr's `gcr4-ssh-askpass`, chosen at first,
  refuses to run as anything but gcr's own agent's helper; found on the dev
  VM, where no dialog appeared. The dialog is GTK's, not Alpymist's, and
  Ctrl+Alt+Delete cannot vouch for it the way it does for `alpymist-auth`'s
  prompt; an askpass of Alpymist's own is left for later.
- The lock screen runs `ssh-add -D` once the screen is covered, so a machine
  left locked has no key unlocked.

## Consequences

- VS Code, browsers and libsecret apps keep sign-ins in the keyring, which the
  login unlocks. A secret saved before this in `basic` mode is not moved: the
  app asks to sign in again, or keeps using `basic` if told to.
- An account has no keyring until its first login after this: PAM makes the
  login keyring then, with the password just typed, so it starts in step.
- Every SSH key's passphrase is asked once per session and again after each
  lock. `AddKeysToAgent confirm` in `~/.ssh/config` asks before every use too,
  for whoever wants that.
- gnome-keyring's own autostart entries are GNOME's only (`OnlyShowIn`), so
  `alpymist autostart` leaves them alone and nothing starts it twice.
