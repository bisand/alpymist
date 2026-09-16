# CI

Five workflows:

- **CI** (`.github/workflows/ci.yml`): `cargo fmt --check`, clippy with
  `-D warnings`, and the tests, on every push to `main` and every pull request.
  A couple of minutes.
- **Packages** (`.github/workflows/packages.yml`): every Alpymist package for
  x86_64 and aarch64, on native runners, for one channel. Not run on its own:
  the next two call it.
- **Release** (`.github/workflows/release.yml`): what a release is made of,
  built when a GitHub release is published. Stable packages, then an ISO for
  each architecture from exactly those packages, with the x86_64 one booted in
  QEMU with KVM to assert the reported tier. The ISOs and their `.sha256` files
  are attached to the release, which is where to download them. The packages
  are an artifact, published by hand (below). "Run workflow" in the Actions
  tab runs the same builds without a release, keeping everything as
  artifacts, to try a change before releasing it.
- **Dev** (`.github/workflows/dev.yml`): dev packages on every push to `main`,
  signed and published to `dev.pkgs.alpymist.org` by CI. No ISO.
- **Site** (`.github/workflows/site.yml`): alpymist.org.

squint is built from a pinned upstream release. It is kept in the Actions cache
under a key of its aport and the builder, and built again only when one of
those changes, or at least once a month so it follows Alpine's libraries.
The ISO jobs build no packages at all: they index and sign the ones the
package jobs made. What is left of an image's time is mostly squashing the
kernel's firmware.

## Channels

Installed systems upgrade with `apk upgrade`; an ISO is only for installing
(ADR 0006). They follow one of two channels:

- **stable**, `https://pkgs.alpymist.org/v3.24/alpymist`: Release runs,
  signed by hand. What the installer sets up.
- **dev**, `https://dev.pkgs.alpymist.org/v3.24/alpymist`: every push to
  main, signed by CI with the dev key.

```sh
alpymist channel                  # which one this system follows
doas alpymist channel dev         # follow dev, trust its key, upgrade
doas alpymist channel stable      # back, distrust it, downgrade to stable
```

Dev packages are versioned `<pkgver>_git<UTC time of their last commit>`, which
apk sorts after the pkgver and before the next one, so dev stays ahead of the
release it follows and meets the next one when it arrives.

### Setting up the dev channel (once)

1. Create `bisand/alpymist-packages-dev`, public, empty. In Settings → Pages,
   serve `main` from the root, with the custom domain `dev.pkgs.alpymist.org`
   and HTTPS enforced. Add a DNS `CNAME` from `dev.pkgs.alpymist.org` to
   `bisand.github.io`.
2. Make an SSH key, add its public half to that repository as a deploy key
   with write access, and nowhere else.
3. In `bisand/alpymist`, Settings → Environments, create `dev-channel`, limit
   its deployment branches to `main`, and give it two secrets:
   `DEV_CHANNEL_SIGNING_KEY` (`~/.config/alpymist/keys/alpymist-dev-2026.rsa`)
   and `DEV_CHANNEL_DEPLOY_KEY` (the SSH private key).
4. Publish stable once with `alpymist-keys` 2026-r1 and `alpymistctl`
   0.0.1-r1 (now `alpymist`), so stable systems have the dev key and the command to switch.

Keep an offline copy of the dev signing key as well, and never put it in
`/etc/apk/keys` on a machine that should follow only stable.

## Publishing stable

The repository at `https://pkgs.alpymist.org/v3.24/alpymist` is a GitHub
Pages site (`bisand/alpymist-packages`). Installed systems have it in
`/etc/apk/repositories` and trust it through `alpymist-keys`.

Only the index is signed, with the release key. Since 2026-09-16 that key is
a secret of the `stable-channel` environment and CI signs with it, on every
successful Release run of a published release; ADR 0002's addendum of that date
records what the change costs. apk takes a package whose hash is in a trusted
index and refuses one whose hash is not, whatever key abuild signed the package
with in CI. Dev is published by the same command with
`--channel dev --packages <dir> --commit <sha>`, which stable refuses: stable
takes a Release run by `--run`, so what is signed is a run that finished.

```sh
gh run list -w Release                    # pick a green run of main
cargo xtask publish --run <id>            # download, sign, verify; no push
cargo xtask publish --run <id> --push     # replace the site
```

The key is read from `~/.config/alpymist/keys/alpymist-2026.rsa` (`--key` to
override). Keep a copy offline; losing it means rotating the key through an
`alpymist-keys` update signed by the old one.

Installed systems only upgrade to a higher version, and the build number gives
every run one (Versions, below), so a change is always shipped. CI rebuilds
everything, and the builds are not reproducible yet, so `publish` keeps the
published file for any version already out and lists the packages that are new
— which, since the build number moves every run, is now all of them. Each
publish is a single force-pushed commit, keeping the site under Pages' size
limit; to roll back, publish an older run.

## Versions

Every first-party package is `<the workspace version>-r<the run number of the
build>`: 0.0.4 built by Release run 8 is `0.0.4-r8` (ADR 0008). The version is
written down once, in `Cargo.toml`'s `[workspace.package]`, and repeated as
`pkgver` in each `aports/*/APKBUILD`, which `cargo xtask version` keeps in
step. The `pkgrel` in those files is never edited: `ci/build-packages.sh` sets
it from `BUILD`, which the workflows pass as `github.run_number`, and a local
`make iso` leaves it at 0.

```sh
cargo xtask version                       # CI's check: they all have to agree
cargo xtask version 0.0.5                 # write it everywhere, pkgrel back to 0
cargo update --workspace                  # and into Cargo.lock
```

squint and `alpymist-keys` keep versions of their own: they are built from a
pinned upstream release and from a key file, not from this workspace. Moving to
a new squint is its `pkgver` and `sha512sums` in `aports/squint/APKBUILD`.

### Cutting a release

1. `cargo xtask version <next>` and `cargo update --workspace`.
2. Commit and merge to main. CI's version check passes when they agree.
3. Publish a GitHub release tagged `v<next>`. Release refuses to build if the
   tag and the workspace disagree, then builds the packages and the ISOs and
   attaches the images.
4. Nothing. Publish to stable follows a successful Release run on its own
   (`publish-stable.yml`), signing the index with the release key from the
   `stable-channel` environment. To publish by hand instead — the key off CI,
   as ADR 0002 first had it — `cargo xtask publish --run <id> --push` still
   does exactly that.

Still planned:

1. `cargo deny check` and `cargo audit`.
2. An install test: install onto a scratch disk and boot the result.
3. Double-build reproducibility diff as a release gate.

## Known limitation: `xtask smoke` on a macOS dev box

The smoke test spawns QEMU as a child process. In at least one sandboxed macOS
development environment that spawn hangs before QEMU produces any output — a
five-line `rustc`-built program reproduces it on `qemu-system-aarch64
--version`, so it is not xtask's logic. Running QEMU directly from a shell on
the same machine works every time.

If `make smoke` hangs locally, boot the image by hand instead:

```sh
qemu-system-aarch64 -M virt -cpu cortex-a72 \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -device virtio-gpu-pci -m 2048 -smp 2 -display none -no-reboot -boot d \
  -serial file:/tmp/boot.log -cdrom out/alpymist-0.0.1-aarch64.iso
```

then read the probe report out of `/tmp/boot.log`. The Linux CI runners drive
`xtask smoke` normally.
