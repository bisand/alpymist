# CLAUDE.md

Notes for coding agents working in this repository. Humans want
[README.md](README.md) for what Alpymist is, [ci/README.md](ci/README.md) for
how it is built and published, and [docs/adr/](docs/adr/) for why any of it is
the way it is. Nothing here repeats those; this is only the things an agent can
break without noticing.

## Orientation

`make help` lists the targets. `cargo run -p xtask -- --help` lists the rest.
Read `ci/README.md` before touching anything under `.github/workflows/`,
`ci/`, or `aports/`.

CI runs exactly this, so run it before pushing:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --features alpymist-ui/render -- -D warnings
cargo test --workspace --features alpymist-ui/render
cargo run -q -p xtask -- version          # every pkgver must equal the workspace version
```

## Invariants

**Never hand-edit a `pkgver` or a `pkgrel`.** One version lives in
`Cargo.toml`'s `[workspace.package]`, and `cargo xtask version <v>` writes it
to every `aports/*/APKBUILD`. `cargo xtask version` with no argument fails if
they disagree; Release runs it with `--expect <tag>`. `pkgrel` is the CI run
number, set by `ci/build-packages.sh` from `BUILD`; the committed value is `0`
and stays `0`. See [ADR 0008](docs/adr/0008-versions-and-build-numbers.md).

**Two lists of independent packages must agree.** `INDEPENDENT` in
`ci/build-packages.sh` and `INDEPENDENT` in `xtask/src/version.rs` both hold
`squint` and `alpymist-keys`. They are not built from this workspace, so
neither the dev stamp nor the build number touches them. Adding a package like
that means editing both.

**squint's `sha512sums` is a real pin.** It is the one aport fetched from
upstream. `ci/build-packages.sh` deliberately does *not* run `abuild checksum`
over it — that subcommand deletes the block and regenerates it from whatever
was downloaded, which verifies nothing. If another aport ever gains a remote
source, extend that exclusion or its pin is decorative.

**`-rN` is not optional.** apk's version grammar is
`digit{.digit}…{letter}{_suf{#}}…{~hash}{-r#}` and abuild dies with "Missing
pkgrel in APKBUILD" without one. There is no way to ship `alpymist-0.0.6.10`.

**The dev stamp is `_git` plus exactly fourteen digits.** `dev_stamped()` in
`xtask/src/publish.rs` uses that width to tell a dev build from an upstream
snapshot, and stable refuses anything it matches. Changing the stamp's format
means changing that function and its tests together.

**Release also runs on `workflow_dispatch`.** `publish-stable.yml` gates on
`github.event.workflow_run.event == 'release'` for that reason. Removing that
condition makes every by-hand test run publish to stable.

**`publish` refuses a run that has not concluded**, which is why publishing is
a separate workflow on `workflow_run` and not a job inside Release. A job
cannot wait for its own run.

**Reversing a decision means an addendum to its ADR, not a quiet edit.** The
ADRs are a record of what was decided and when; the originals stay. ADR 0002's
addendum of 2026-09-16 is the worked example — it records that CI now holds the
stable signing key, and says plainly what that gave up.

## Traps in this environment

- **The builder is Alpine, so `sed` is busybox.** GNU-only forms such as
  `0,/re/` may not work there. `/^name = "x"$/{n;s/…/…/;}` is portable and is
  what `aports/squint/APKBUILD` uses.
- **`gh run view --log` only works after a run finishes.** While one is in
  progress it returns a one-line notice, not logs.
- **Do not use Bash to run the site's dev server.** `.claude/launch.json` has
  the configuration; use the preview tooling.
- Alpine builds cannot always be exercised locally — a sandboxed Docker may
  have no DNS, and `make smoke` is known to hang on macOS (`ci/README.md` has
  the manual QEMU invocation). Where a change can only be proven in CI, say so
  rather than implying it was tested.
