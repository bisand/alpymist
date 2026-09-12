# ADR 0004 — Confining `unsafe`, and why our binaries are not static

**Status:** accepted · **Date:** 2026-09-12

Two decisions forced by building the EGL probe. Both are consequences of the
same thing: asking the graphics stack a question means talking to C.

## `unsafe` is confined to one crate

Alpymist's rule is that first-party code is Rust with `unsafe` forbidden.
Querying EGL cannot honour that — there is no safe way to `dlopen` a library or
to call a function pointer that a C API handed back.

Rather than weaken the rule everywhere, the workspace lint is now
`unsafe_code = "deny"` and every crate except one carries
`#![forbid(unsafe_code)]` at crate level. All `unsafe` in the project lives in
`crates/alpymist-glesprobe/src/ffi.rs`, which does exactly one job and is short
enough to audit in a sitting. Each `unsafe` block carries a `SAFETY:` comment
stating the invariant it relies on.

If a second crate ever needs `unsafe`, that is a decision worth another ADR,
not a quiet edit.

## GL ES version alone is not a capability signal

Mesa's `llvmpipe` software rasteriser advertises **OpenGL ES 3.2** — a higher
version than much of the real GPU hardware Alpymist targets. A tier decision
based on the version alone would promote a machine with no working GPU straight
to Hyprland, which is the exact failure the tier system exists to prevent.

So `GlesInfo` carries the `GL_RENDERER` string, and `select_tier` checks for a
CPU rasteriser *before* it looks at the version. Detection matches whole tokens
rather than substrings, because a bare `swr` substring also fires on hardware
renderer names that merely contain those letters.

This is a denylist, so it will be wrong about a rasteriser nobody has seen yet.
It errs towards calling something hardware, which the version and RAM checks
then still have to agree with.

## Our binaries are dynamically linked against musl

Rust's `*-unknown-linux-musl` targets default to `crt-static`, producing a fully
static binary. **A static binary cannot `dlopen`**, so the EGL probe failed with
"Dynamic loading not supported" before it ever reached libEGL.

`.cargo/config.toml` therefore sets `-C target-feature=-crt-static` for both
musl targets. This matches how Alpine builds its own Rust packages, so it is
also what an `APKBUILD` will produce.

The tradeoff is real but small: our binaries now need `libc.musl-*.so.1` at
runtime, which is in the base system by definition. In exchange, anything we
ship can load an optional system library at runtime rather than hard-linking
against it — which is what lets `alpymistctl` run on a machine with no Mesa
installed at all, and simply report that it learned nothing.
