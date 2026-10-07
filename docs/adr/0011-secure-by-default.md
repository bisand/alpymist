# ADR 0011 — Secure by default; the unsafe is opt-in

**Status:** accepted · **Date:** 2026-09-26

## Context

Alpine's own promise is "small, simple, secure", and Alpymist is built on it
([ADR 0005](0005-alpine-324-base.md)). Until now that has been true of
Alpymist one decision at a time, without being written down anywhere:

- The installer locks `root` (`passwd -l root`), because the live image's root
  has no password and `setup-disk` would otherwise copy that onto the disk.
- Administration is through doas, for the `wheel` group, with a password:
  `permit persist :wheel` in `/etc/doas.d/20-wheel.conf`.
- The lock screen is an `ext-session-lock-v1` client, so killing it strands the
  session rather than uncovering it ([ADR 0010](0010-lock-screen.md)).
- Packages are signed, and the stable key's custody is recorded
  ([ADR 0002](0002-supply-chain.md) and its addenda).
- The SSH server is off until someone turns it on from the menu.

Several pieces of work now open would each be easier with a weaker default.
Examples: approving every Thunderbolt device so a dock works with no clicks
(#18), a clipboard history that records everything copied (#15), and API
tokens kept in a TOML file (#16). Passwordless doas on a developer's own
machine is a convenience that could slip into a default without anyone
deciding it. The line needs to be written down before it is crossed by
accident.

## Decision

### 1. What ships is safe with nobody touching it

Everything Alpymist installs or configures without being asked must be safe:
the ISO, the live session, the installer, the packages, the files copied into
a new home directory, and the first boot. "Safe" means no weaker than a
careful administrator would leave a single-user laptop:

- no account without a password, no automatic login, and no passwordless
  privilege escalation;
- nothing listening on the network that the user did not turn on;
- no device given access to memory, storage or input without the user
  approving it;
- no secret — password, token, key, clipboard content — written to disk in
  the clear, or kept at all without the user having chosen to keep it;
- nothing mounted, executed or trusted automatically at the login screen or
  while the session is locked.

### 2. The unsafe is a tool, not a default

Alpymist may ship the means to weaken any of this, because the machine is its
owner's. Those means follow three rules:

- **Explicit.** It is turned on by a person, on purpose: a setting, a command
  or a prompt they answer. It is never turned on by an installer default, a
  package's install script, an upgrade, or a "recommended" button.
- **Informed.** Where it is turned on, it says in one sentence what is given
  up ("Anything running as you can become root without a password").
- **Reversible.** It can be turned off the same way, and turning it off
  restores the safe state completely.

### 3. A developer's machine is not the default

A convenience configured by hand on a machine used to build Alpymist
(passwordless doas, an authorized dock, a history of everything copied) stays
on that machine. It is not evidence of what users want, and it never reaches
the image, the installer, the packages or the skeleton files.

### 4. Reviewed like the rest of the design

A feature that touches any of §1's points says in its issue or ADR what its
default is and why that default is safe. A change that weakens a default is a
reversal and is recorded as an addendum to this ADR, the way ADR 0002 records
that CI holds the stable signing key.

## Consequences

- Some things take one more step than they could: approving a dock the first
  time, turning on clipboard history, typing a password for doas. That step is
  the point, and the work is making it quick and clear, not removing it.
- Features are designed with the opt-in in mind from the start: a setting in
  the registry ([ADR 0007](0007-settings.md)), a prompt that names the device
  or the risk, and a way back.
- Reviews have a concrete question to ask of any change to the installer, the
  packages or the skeleton files: does this make the machine less safe for
  someone who never opens Settings?

## Addendum — 2026-09-29: the clock is set from the network by default

Network time was off until someone turned it on in Settings › Date & time, and
nothing said so. The Asus test machine's hardware clock lost its time, came up
in 2012, and stayed there: every certificate was then not yet valid, and apk,
the store, Flatpak and the browser all failed with nothing on screen to say
why (#65).

It is on by default now: the installer adds busybox's `ntpd` to the default
runlevel, and `alpymist-desktop` does the same for systems installed before,
once, leaving a mark in `/var/lib/alpymist` so a later upgrade does not undo
turning it off.

This is not a weaker default, so it is an addendum rather than a reversal.
§1's "nothing listening on the network" is about answering others, and `ntpd`
here is a client: `/etc/conf.d/ntpd` gives it `-p pool.ntp.org` and not `-l`,
so it asks the pool and answers nobody. What it does reveal is that the
machine asks pool.ntp.org the time, which every phone and laptop does too. A
right clock is itself a security property: certificates, signatures and
expiring tokens are all judged against it.

## Addendum — 2026-10-07: Hyprland's questions go to a program that answers none

A screenshot ended the desktop, now and then, on the development machine
and on the Asus, and nobody knew why: Hyprland's crash reports hold no
backtrace on musl. A debugger attached to it on 2026-10-07 gave one. It is
not the screenshot. `CDynamicPermissionManager::askForPermission` makes a
dialog, sets a field on it, and only then looks whether it was made; with no
`hyprland-dialog` on the path it was not, and Hyprland 0.54.3 ends there.
Hyprland's `main` branch read the same that day.

`hyprland-dialog` is part of hyprland-guiutils, which Alpine does not
package. `hyprland-security.conf` was written knowing that, with a rule for
every program so that nothing would need asking. But the rules go by a
program's path, looked up from its process, and a program that has ended has
none: `grim`, finished while the screen was still being redrawn after a
change of workspace, is asked about as a program unknown. Switching
workspace and taking a screenshot at once ended the desktop one time in
two.

The same dialog is made, and used unlooked at, in two more places: when a
window stops responding, and in the Hyprland that `start-hyprland` starts
after a crash, to say that it is in safe mode. The second is why one crash
became two and a login screen saying the desktop stopped as it started.

**Decision.** The desktop ships `/usr/bin/hyprland-dialog`: a shell script
that writes what was asked to the session's log and chooses no button. With
it on the path the dialog is made and nothing is dereferenced that is not
there. Choosing nothing leaves a permission pending for as long as the
program that wanted it lives, which is a refusal; ends no program for being
slow; and leaves the recovery with the configuration it has.

That is safe by §1, and safer than what Hyprland means to do without a
dialog, which is to stop enforcing: "cannot ask! Disabling permission
control" is what its source says of that case. Nothing is allowed here that
a rule did not allow.

What it gives up: nobody is ever asked. A program that hangs is not offered
to be ended, and a program no rule covers is refused without a word. A
dialog of our own that does ask, drawn as `alpymist-auth` draws its, would
take this script's place; so would Alpine's hyprland-guiutils, at the same
path, should it come.

**What it does not mend.** The question about a screenshot should never
have been asked, and once it has been, Hyprland keeps the unanswered
permission by the address of the program's connection, and does not drop it
when the program goes. The next program given that address is `grim` again,
as likely as not, and waits for an answer that was never its question: after
the first such question on the development machine, three screenshots in
eight hung until ended by hand, where forty in a row had been taken before
it. That lasts until the next login. It is a desktop that stays up with a
screenshot that sometimes does nothing, in place of a desktop that ends;
answering yes for the script would mend the screenshot and give the same
address's next owner the screen, whatever the rules say of it, and is not
done. The mending is in Hyprland: look before using the dialog, and drop a
permission with its program.

Seen on the development machine: the backtrace with the debugger, from
`CScreenshareFrame::render` through `clientPermissionMode` to the null in
`askForPermission`; and with the script installed, a change of workspace
and a screenshot twenty-five times over with Hyprland still running, the
question in the session's log as one about an "Unknown application", and a
terminal stopped for twenty-five seconds, asked about as not responding,
with Hyprland still running. Read in the source and not seen: that the last
of those ends Hyprland without the script, and the safe mode's dialog.
