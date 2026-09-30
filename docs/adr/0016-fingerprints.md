# ADR 0016 — Fingerprints unlock the screen and answer prompts, never log in

**Status:** accepted · **Date:** 2026-09-30

## Context

The X1 Carbon's reader, a Synaptics 06cb:009a, has no libfprint driver.
validity-fprintd (our own, packaged since #77) drives it and serves fprintd's
D-Bus interface in fprintd's place, and the Fingerprints window (#78) enrols
fingers with it. Enrolled fingers did nothing yet: every PAM stack still asked
for the password alone (#58).

Three places could take a finger, and they are not alike:

- **The login screen** is where the password unlocks the keyring (ADR 0013).
  `pam_gnome_keyring` needs the password itself, and a finger has none to give.
  A login by finger opens a session whose keyring stays locked, and every
  program that keeps a secret asks for the password again, in a dialog of its
  own that nobody recognises.
- **The lock screen** is Alpymist's own (ADR 0010). It speaks to PAM itself, so
  it can ask two services at once.
- **Administrator prompts** are polkit's. polkitd trusts only its setuid
  helper, which runs one fixed service, `polkit-1`, one step at a time. A
  module that waits for a finger holds up the password prompt behind it until
  it gives up.

## Decision

**Off until an administrator turns it on**, in Settings › System › Unlock
with a fingerprint (ADR 0011: a way in that is not the password is a weakening,
and ships as an explicit, reversible opt-in). The switch needs fprintd-pam,
which alpymist-fingerprint brings. It changes exactly two things:

1. **`/etc/pam.d/alpymist-lock-fingerprint`**, a service of the lock screen's
   own holding `pam_fprintd` and nothing else. The file being there is the
   switch. The lock asks it on a thread of its own for as long as the screen is
   locked, beside the password service, which is unchanged. Whichever answers
   first unlocks. Three fingers not recognised in a row and the lock stops
   listening to the reader until it is unlocked with the password.
2. **One line in `/etc/pam.d/polkit-1`**: `-auth sufficient pam_fprintd.so`,
   above the password. `sufficient` means a finger lets the prompt through and
   anything else goes on to the password: no reader, no finger enrolled, a
   finger not recognised, or the reader busy. The dash keeps polkit working if
   fprintd-pam is removed while the line is there.

**The admin prompt does not make you wait for the finger to time out.** While
the helper waits for a finger, the dialog takes typing anyway. Enter starts a
second helper beside the first. Its `pam_fprintd` finds the reader claimed by
the first, gives up at once, and hands over to the password. Whichever helper
says yes first closes the dialog, and the other is killed. This needs nothing
but stock `pam_fprintd` and a daemon that refuses a second claim, which fprintd
and validity-fprintd both do.

**The login screen never takes a finger**, for the keyring's sake. Neither
does `doas`: Alpymist leaves its configuration alone.

**The daemon's word is the answer.** Every check goes through `pam_fprintd`,
so whatever decides "that is this account's finger" lives in the fingerprint
daemon, running as root. Nothing in the session judges a finger. That put a
requirement on validity-fprintd, fixed in its v0.1.3: a stopped or abandoned
scan must neither take the next finger nor report a result. With two helpers
and a lock screen sharing one reader, a stale `verify-match` heard by whoever
claimed the reader next would let them in on a finger they never asked about.

## Consequences

- `polkit-1` belongs to the polkit package, and Alpymist edits it. apk keeps
  an edited `/etc` file across an upgrade and puts the package's new one
  beside it as `.apk-new`. Turning the switch off takes out exactly the line
  and the note above it that it put in, and nothing else.
- A finger is weaker than a good password in known ways: it can be lifted, and
  it cannot be changed. That is the administrator's trade to make, and the
  switch says what it does.
- The lock screen holds the reader while it is up, so the Fingerprints window
  cannot use it then. Nobody can reach that window behind a locked screen
  anyway.
- A system without a reader, or an account with no finger enrolled, loses
  nothing. `pam_fprintd` says so at once, and the lock asks again less and less
  often, up to once a minute.
