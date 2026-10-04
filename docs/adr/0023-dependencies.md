# ADR 0023 — What Alpymist's programs depend on

**Status:** accepted · **Date:** 2026-10-04

## Context

At 0.3.4 `Cargo.lock` held 317 packages that are not ours, and the programs
the packages ship linked 164 of them. All came from crates.io and none was
chosen carelessly, but nothing said how one gets in: a dependency arrived as
a line in a `Cargo.toml`, in a change about something else, and was reviewed
as that something else.

Each is code that runs as the account, and in the lock screen, the password
dialog and the installer, code that a password passes through. Each is also
somebody else's release schedule, and a second copy of whatever it shares
with its neighbours.

## Decision

**As little as possible that is not ours.** What a few dozen lines of our own
would do is not a reason for a crate. DeniseUI is ours, as is anything else
published from `bisand/*`; those are allowed, and named one by one, since
anybody may publish under a prefix.

**What must be third-party comes from crates.io, and is well known.** No git
dependency, no other registry, no path outside the workspace. Well known
means widely used and maintained by people with a record: a judgement, made
once, at the moment the crate is added.

**That moment is an edit to a list.** `ALLOWED` in `xtask/src/deps.rs` names
every third-party crate a manifest in the workspace may name, with what it is
for. `cargo xtask deps`, which CI runs, fails when:

- a locked package comes from anywhere but crates.io, or from a path that is
  not a workspace member;
- a manifest names a third-party crate that is not on the list;
- the list names a crate nothing uses any more, so that it shrinks as the
  workspace does;
- `Cargo.lock` holds more third-party packages than `LOCKED`, the figure
  recorded beside the list.

The check reads `Cargo.lock` and the manifests a line at a time. A TOML
parser there would be a dependency of the check on dependencies.

## Consequences

- The list is of what the workspace names, not of everything that follows:
  a listed crate may bring others with it at its next release. `LOCKED` is
  what notices that, crudely. It counts the whole lockfile, of which the
  desktop preview's winit is some two thirds and ships nowhere; what a
  package actually links is not counted, because that takes every aport's
  build line and a target to work out.
- `cargo update` can raise the count with no manifest touched. Then the
  figure is raised, in that change, by someone who has looked at what came.
- Crates already there that the rule would not admit today stay on the list
  with the issue that removes them beside them: `nonstick` above all, a
  young crate under the lock screen (#111). The list is honest about what
  is, not only about what should be.
- Nothing here looks for advisories against the crates that are allowed.
  `cargo audit` does that and is not run; it is a tool to install and trust
  in CI, and a decision of its own.
- squint and validity-fprintd are built from their own repositories with
  their own lockfiles, and this check does not see them.
