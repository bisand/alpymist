# Screensaver, idle and lock
<!-- group: The desktop -->

Three separate things happen when a computer is left alone, each with its own
timing: a screensaver appears, the screen turns off, and, if you want it, the
screen locks.

## What happens when you walk away

| After | Happens | Setting |
|---|---|---|
| 5 minutes | The screensaver appears | *Show the screensaver after* |
| 10 minutes | The screen turns off | *Turn the screen off after* |
| When the screen turns off | The screen locks, if switched on | *Lock when the screen turns off* |

Both times count from the last key or movement, and zero turns either off.
Locking with the screen is **off** as shipped: turn it on for a machine that
is ever left where others can reach it.

```sh
alpymist set screensaver.after 3
alpymist set screensaver.blank-after 15
alpymist set screensaver.lock true
```

The screensaver is a picture, not a lock. The first key or movement takes it
away and asks for nothing.

## Screensavers

Two ship, drawn on the processor at 12 and 20 frames a second so they cost
little battery:

- **Mountains**: ranges drifting past in mist, with an aircraft crossing and,
  now and then, a hot-air balloon.
- **Starfield**: stars streaming past a ship that steers through an asteroid
  field every minute or so.

Settings › Screensaver chooses one, or *A different one each time*, which is
the default. **Screensaver settings…** opens the chosen one's own settings:
how chunky the picture is, how fast and how smoothly it moves, and what
appears in it. Each has a **Preview**.

```sh
alpymist-screensaver                  # show it now
alpymist-screensaver list             # the ones installed
alpymist set screensaver.show starfield
alpymist list screensaver-mountains   # one screensaver's own settings
alpymist set screensaver-mountains.balloon false
```

A screensaver is a package: a program and a file saying what its settings
are. One installed from elsewhere appears in Settings beside these two.

### With more than one screen

The screensaver shows on the main screen and the others go dark. *Main
screen* chooses which, and *Show it on every screen* puts it on them all.

## Locking

<kbd>Super</kbd>+<kbd>L</kbd> locks the screen. So does System › Lock in the
menu, suspending, and closing the lid if [Power](13-power.md) is set to lock.

The lock screen is the login screen: the same picture, the clock, and a card
with your name. Type your password and press <kbd>Enter</kbd>.

A few things are worth knowing about it:

- It covers every screen the moment it is asked for, and the desktop stays
  covered even if the lock program itself were to crash.
- While locked, SSH keys that were unlocked are forgotten, and the clipboard
  history forgets all but its pinned entries unless Settings › Clipboard says
  to keep it.
- There is no way around it but the password, or a finger if you have turned
  that on.

## Unlocking with a fingerprint

On a laptop with a supported reader, an enrolled finger can stand in for the
password in three places, each with a switch of its own in Settings › System.
All three are off until you turn them on, and the password goes on working
everywhere.

| Switch | A finger then | `alpymist set` |
|---|---|---|
| Unlock the screen with a fingerprint | Unlocks the locked screen | `system.fingerprint-lock` |
| Answer administrator prompts with a fingerprint | Answers the window that asks for an administrator's password | `system.fingerprint-prompts` |
| Log in with a fingerprint | Logs in at the login screen | `system.fingerprint-login` |

The installer sets the reader up where it finds one it knows. On another
machine, `doas apk add alpymist-fingerprint` first; the reader also needs a
driver of its own, which for most is `fprintd`.

1. Search the menu for **Fingerprints**, or run `alpymist-fingerprint`.
   Choose a finger, add it, and test it. Adding or removing one asks for your
   password.
2. Turn on the switches you want in Settings › System, or for instance
   `alpymist set system.fingerprint-lock true`. Each asks for an
   administrator's password.

**At the lock screen**, touch the reader or type the password, whichever you
like. After three fingers in a row that are not recognised, the lock stops
listening to the reader until it is unlocked with the password.

**At an administrator prompt**, touch the reader, or type the password and
press Enter.

**At the login screen**, press <kbd>Enter</kbd> with nothing typed, then
touch the reader within ten seconds. Typing the password logs in as it always
did, and does not wait for a finger.

A login by finger leaves your keyring locked. The keyring is opened with your
password, and a finger has no password to give. The first program that wants
something kept there, such as a saved Wi-Fi or git password or an SSH key's
passphrase, asks for your password then. Log in with the password when you
would rather not be asked later.

`doas` in a terminal always takes the password: Alpine builds it without the
part a fingerprint would go through. Alpymist's own commands that need an
administrator, such as `alpymist set` for a system setting, ask through the
administrator prompt, and so take a finger where that switch is on.

A finger is weaker than a good password in two ways: it can be lifted from a
surface, and it cannot be changed. That is why each of these is off until you
turn it on.
