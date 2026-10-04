# ADR 0025 — A D-Bus client of our own

**Status:** accepted · **Date:** 2026-10-04

## Context

Three things are spoken to over the system bus: iwd, the fingerprint daemon,
and polkit. All three were spoken to with zbus, through its blocking API.

zbus is well known and well kept, and would pass ADR 0023's test on its own.
What it brought with it is the question. Thirty-two packages that nothing
else in what ships needed: an async executor, reactor, channels, locks and
process handling, used here only to be blocked on; three crates of derive
macros; tracing; and more. The password dialog linked 110 third-party crates,
and 47 of them were there for one registration call and one served method.
The installer, which joins a network before anything is installed, linked the
same.

What Alpymist asks of the bus is small and does not grow: call a method and
wait, hear a signal, read and set a property, and serve two objects of a few
methods each — the agents iwd and polkit ask for a passphrase and a password
through. The wire format it is asked in has not changed since 2006.

## Decision

**`alpymist-dbus`: a blocking client, over a unix socket, with no
dependencies.** One thread reads the socket for as long as the connection is
kept. An answer goes to the caller waiting for it, a signal to whoever asked
to hear it, and a call of a method served here is answered on a thread of its
own — so that polkit's `BeginAuthentication` may take as long as a person
does, and `CancelAuthentication` may still arrive; and so that iwd, asked to
join a network, may ask the one who asked for the passphrase.

**It does that and no more.** No file descriptors in messages: they are
never negotiated, and one that came anyway is an error. No session bus, and
none of the ways one is found. No introspection, no object manager of its
own, nothing asynchronous. A program that needs one of those is a reason to
revisit this, not a feature to add in passing.

**It says who it is without knowing.** `AUTH EXTERNAL` with no identity
given: the bus reads the user from the socket's credentials, so nothing here
has to find its own user's number, and no system-call crate is needed for it.

**One type for every value.** A `Value` that knows its own signature, looked
into by name and kind where it is used. Typed bindings generated from an
interface are what a derive macro is for, and the interfaces here are a
handful of calls.

**Nothing read is trusted.** A message comes from a daemon running as root,
through a bus that has already checked it. It is read as if neither were so:
every length held against what is there, text checked to be text and paths to
be paths, booleans to be 0 or 1, nesting counted and stopped at the
specification's limit, a message refused past 32 MiB before anything is
allocated for it. What does not hold together is an error and never a panic.

## How it is held to that

- **The wire format**, in `wire.rs`: every kind of value out and back; the
  bytes of `Hello` written out by hand from the specification; a message with
  the big end first; signatures taken apart and refused; a good message cut
  short at every kind of place, with each of its bytes changed in turn and
  twenty thousand times at random, and a thousand variants nested in one
  another. None may do worse than an error.
- **A real bus**, in `tests/bus.rs`: a `dbus-daemon` of the tests' own, which
  checks every message it is handed and hangs up on one that is not D-Bus.
  Every kind of value through it and back between two connections; a call
  answered while one of our own is waiting, as iwd's agent is; a method that
  waits not keeping another from being called, as polkit's cancel needs;
  signals asked for heard and others not; properties; and a bus that goes
  away while somebody waits.

## Consequences

- zbus and thirty-five packages with it leave the lockfile, 303 to 267. The
  password dialog goes from 110 third-party crates to 63, the installer from
  103 to 51, Settings from 119 to 72.
- A parser of our own now stands where a password's dialog is asked for and
  where a network's passphrase is handed over. The passphrase and the
  password themselves do not pass through it as anything it reads: the first
  is written to iwd, the second goes from the prompt to polkit's helper and
  never onto the bus.
- A mistake in it is ours to find. It is about eleven hundred lines apart
  from its tests, and the tests are most of that again.
- An interface that needs a kind of value not handled, or a file descriptor,
  cannot be spoken to until this is extended, deliberately.
- Calls wait for as long as the other end takes. A peer that leaves the bus
  is answered for by the bus; a peer that stays and says nothing is waited
  for, as it was with zbus.
