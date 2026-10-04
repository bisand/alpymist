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
# What the installer, splash and greeter ship as. Linux only: the DRM
# backend does not build on macOS.
cargo clippy --no-default-features --features drm -p alpymist-greeter \
  -p alpymist-install -p alpymist-splash --all-targets -- -D warnings
cargo run -q -p xtask -- version          # every pkgver must equal the workspace version
cargo run -q -p xtask -- deps             # every crate from crates.io, every third-party one on the list
```

## Invariants

**Nothing unsafe ships on by default.** No passwordless privilege escalation,
no automatic login, no listening service, no auto-authorized device, no secret
kept in the clear, in the image, installer, packages or skel files. The means
to weaken any of it may ship, as an explicit, reversible opt-in. A convenience
set up by hand on a developer's machine is not a default. See
[ADR 0011](docs/adr/0011-secure-by-default.md).

**Never hand-edit a `pkgver` or a `pkgrel`.** One version lives in
`Cargo.toml`'s `[workspace.package]`, and `cargo xtask version <v>` writes it
to every `aports/*/APKBUILD`. `cargo xtask version` with no argument fails if
they disagree; Release runs it with `--expect <tag>`. `pkgrel` is the CI run
number, set by `ci/build-packages.sh` from `BUILD`; the committed value is `0`
and stays `0`. See [ADR 0008](docs/adr/0008-versions-and-build-numbers.md).

**A new dependency is a line in `xtask/src/deps.rs`, with its reason.**
As little as possible that is not ours, from crates.io, and well known.
`cargo xtask deps` fails on a locked package from anywhere else, on a
third-party crate a manifest names that `ALLOWED` does not, on one `ALLOWED`
names that nothing uses, and on a lockfile grown past `LOCKED`. Denise's
crates are ours and listed by name in `OURS`, never by prefix. Adding a line
to make the check pass is not the fix when something of ours, or something
already there, does the job; raising `LOCKED` wants a reason too. See
[ADR 0023](docs/adr/0023-dependencies.md).

**Two lists of independent packages must agree.** `INDEPENDENT` in
`ci/build-packages.sh` and `INDEPENDENT` in `xtask/src/version.rs` both hold
`squint`, `validity-fprintd` and `alpymist-keys`. They are not built from this workspace, so
neither the dev stamp nor the build number touches them. Adding a package like
that means editing both.

**A new aport is not built until it is named.** `cargo xtask version` finds it
by listing `aports/`, so its `pkgver` is kept honest from the day it exists —
but `ci/build-packages.sh` builds the packages in the order its `for pkg in …`
list gives, and a package missing from that list is simply never built, with no
error anywhere. Add it there, before anything that depends on it, and add it to
whatever `depends` should pull it in.

**A screensaver is a package, not a branch in a match.** Adding one means a
binary and a `/usr/share/alpymist/screensavers/<id>.toml` beside it declaring
its settings — nothing in `alpymist-screensaver` or `alpymist-settings` names
any screensaver, and adding a name to either is the wrong fix for anything.
`crates/alpymist-saver-mountains` is the worked example, and its
`tests/definition.rs` is what stops a declared setting and the program that
reads it drifting apart. See [ADR 0009](docs/adr/0009-screensaver-and-idle.md)
and its addendum.

**An AI usage provider is a package too.** A program that prints a report,
and a `/usr/share/alpymist/ai-usage/<id>.toml` naming it and the keys it
needs. Nothing in `alpymist-ai-usage`'s bar or command line names a provider,
keys go to a provider on its standard input and to the keyring, never into an
argument or a file, and a vendor's own tool is installed the vendor's way and
never added to `depends`. `crates/alpymist-ai-usage/tests/definitions.rs`
holds the shipped files to that. See [ADR 0017](docs/adr/0017-ai-usage.md).

**A watch is an entry in a list, not a new `exec-once`.** What the session
keeps an eye on is `WATCHES` in `crates/alpymist-watchdog`, each run in a
process of its own by `alpymist watchdog`. The watchdog is the account's:
no root, no listening socket, no configuration. Anything that needs root, or
holds secrets, or is the thing itself and not a watch over something, is not
a watch. See [ADR 0019](docs/adr/0019-watchdog.md).

**A hardware report is run by hand and names nobody.** `alpymist report`
prints, `--save` writes a file, `--issue` asks and opens the browser; nothing
in the installer, a service or the session runs it, and nothing posts;
Settings' button is that command in a terminal. What
goes in is decided in `crates/alpymist-hwprobe/src/report.rs`: devices by
their makers' numbers and never the names they announce, a network card by
kind and never by name, no serial, address, host or account name. Its test
builds a machine holding each of those and fails if one comes out; a new
section means adding what it could leak to that test. See
[ADR 0020](docs/adr/0020-hardware-reports.md).

**Firmware nobody may ship is not shipped.** Broadcom's `b43` firmware is
in no package and not on the image; `alpymist firmware broadcom` downloads
Broadcom's driver from OpenWrt's mirrors, held to the SHA-256 in
`crates/alpymist/src/broadcom.rs`, and cuts it out on the machine that asks,
or onto a stick with `--to`. Nothing downloads it unasked: the firmware
watch and the installer only say which command to run. What the installer
and `--from` copy off a stick is `.fw` files into the directories in
`alpymist-core`'s `CARRIED_DIRS` and nothing else. A new version of that
driver is a new pin, not a looser check. The pin is written twice:
`site/static/broadcom-firmware.sh` is the same fetch for a machine that is
not Alpymist, served by the site and put at the top of the image, and a test
in `broadcom.rs` fails when the two disagree. The script goes on the image;
nothing it downloads ever does. See
[ADR 0021](docs/adr/0021-firmware-nobody-may-ship.md).

**A control key is a bind and a name, in two files that must agree.**
`desktop/hypr/hyprland-keys.conf` binds each volume, brightness, media and
radio key to `alpymist key NAME`, for every machine alike, and
`crates/alpymist/src/keys.rs` is what the names do; its tests fail when
either has a name the other lacks. No per-machine table, nothing as root:
lights go to the `video` group by `desktop/udev/91-alpymist-backlight.rules`
and never to `input`, and a radio is switched by iwd or bluetoothd, not by
rfkill. See [ADR 0022](docs/adr/0022-control-keys.md).

**Tab completion is asked of the program, not generated.** The three files
in `desktop/completion` hold no command's name: each hands the words typed to
the hidden `alpymist complete` and shows what it prints, so a new subcommand,
option or setting is completed the day it exists. What clap cannot list — the
settings, a setting's values, the screens and their modes — is in `named` in
`crates/alpymist/src/complete.rs`; an argument that takes names of that sort
goes there. No completion crate: clap's own description of the command line
is enough, and a test there holds the shells' files to how the command is run
and how it ends for a file's name.

**The manual's "Every setting" page is made, not written.**
`docs/manual/26-every-setting.md` is what `crates/alpymist/tests/every_setting.rs`
makes from `alpymist list --json`, and that test fails when the two differ. A
setting added, renamed or given another default or description means writing
the page again, with `ALPYMIST_BLESS=1 cargo test -p alpymist --test
every_setting`, and committing it. Editing the page by hand only fails the
test; its wording lives in the test.

**Mesa is not built here, and the guest key is trusted only on request.**
The Mesa with the virgl driver comes from `bisand/alpymist-mesa`'s own
repository, which a system follows only when asked (`alpymist guest on`, or
the installer's switch in a virtual machine). Putting a Mesa in `aports/`
would install it on every machine at the next upgrade, and installing
`alpymist-guest-2026.rsa.pub` to `/etc/apk/keys` in `alpymist-keys` would have
every system trust that repository's CI. See
[ADR 0018](docs/adr/0018-guest-graphics.md).

**squint's and validity-fprintd's `sha512sums` are real pins.** They are the
aports fetched from upstream, named in `FETCHED` in `ci/build-packages.sh`,
which deliberately does *not* run `abuild checksum` over them — that
subcommand deletes the block and regenerates it from whatever was downloaded,
which verifies nothing. If another aport ever gains a remote source, add it to
`FETCHED` or its pin is decorative.

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
