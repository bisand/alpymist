# ADR 0024 — PAM bindings of our own, and a second crate with `unsafe`

**Status:** accepted · **Date:** 2026-10-04

## Context

The lock screen checks the password, and a finger, through libpam. ADR 0010
weighed writing the conversation ourselves against a safe wrapper, and took
the wrapper: `nonstick`, so that ADR 0004's rule held as written, with all of
the project's `unsafe` in `alpymist-glesprobe`.

ADR 0023 then asked of every dependency whether it is well known. `nonstick`
is not: a 0.1 crate with few users, and three more crates behind it. And
where it sits is the worst place for the question to have that answer —
between a locked screen and the account, with the password in its hands. The
`unsafe` it kept out of this repository was never absent. It was somewhere
nobody here had read.

ADR 0004 said a second crate needing `unsafe` would be worth another ADR.
This is that one.

## Decision

**`alpymist-pam` is ours, and is the second crate permitted `unsafe`.** All
of it is in `crates/alpymist-pam/src/ffi.rs`, under the same terms as the
first: one module, one job, short enough to audit in a sitting, a `SAFETY:`
comment on every block. Every other crate still carries
`#![forbid(unsafe_code)]`, `alpymist-lock` among them.

**It declares what the lock uses and nothing else.** `pam_start`,
`pam_authenticate` and `pam_end`, and the C library's `calloc`, `malloc` and
`free`, because PAM frees a reply with `free`. No account management, no
session, no credentials, no changing a password: what is not declared cannot
be called, by this code or by a mistake in it.

**It is Linux-PAM's ABI, written out.** The structures and return values of
`_pam_types.h`, by hand, with no bindgen and no clang in the build; the
builder needs `linux-pam-dev`, as before. Linux-PAM is the only PAM Alpine
ships, so none of a wrapper's portability across implementations was in use.

**It depends on nothing.** Not on `libc`, not on `zeroize`: an answer is
wiped where it is copied, in the module that already has to be read.

## What the module has to get right

The conversation callback is where PAM is got wrong, so it is what the tests
are about:

- replies are made with the C allocator, and are PAM's only once the callback
  returns success; until then each one made is wiped and freed, and PAM is
  handed nothing;
- an answer with a NUL in it is refused, not cut short and sent;
- a message of a kind that is not one of the four is not answered;
- a panic in the conversation is a failed conversation, and does not cross
  into C;
- what PAM would never pass — no messages, too many, a null pointer — is
  checked before anything is followed.

Those run anywhere, with the test playing PAM. On Linux, more run against
libpam itself: `pam_start_confdir` takes a directory of services, so a stack
of `pam_permit`, `pam_deny`, `pam_echo` or `pam_unix` can be asked without
root and without touching `/etc/pam.d`.

## Consequences

- `nonstick`, `libpam-sys`, `libpam-sys-helpers` and `libpam-sys-impls` leave
  the lockfile.
- There are two places to read when asking what `unsafe` Alpymist has, not
  one. A third is still a decision worth an ADR.
- A mistake here is ours, and under the lock screen. The module is three
  hundred lines apart from its tests, a third of them comment, and should stay
  near that.
- The lock asks nothing new of PAM, and its services are as they were.
