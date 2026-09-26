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
