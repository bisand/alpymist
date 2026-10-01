# Alpymist

An opinionated, curated desktop on top of Alpine Linux: Hyprland on Wayland,
configured once and kept in focus, secure by default, and shipped as nothing
but signed packages.

The name is Alpine plus mist — the haze on the mountains — and a pun on
*alchemist*, which is roughly what turning a fifteen-year-old laptop back into
a usable desktop amounts to.

Three things make it different from the curated-desktop projects it takes
inspiration from:

- **It is one desktop.** Hyprland on Wayland, and nothing else to maintain
  beside it. The installer probes the machine and says how well Hyprland will
  run there, and why. See [ADR 0001](docs/adr/0001-hardware-tiers.md) and its
  addendum.
- **It ships as signed packages.** No `curl | bash`, no root install scripts,
  no unpinned third-party repos. See [ADR 0002](docs/adr/0002-supply-chain.md).
- **It is secure by default.** Nothing unsafe is on unless its owner turns it
  on, knowing what it gives up. See
  [ADR 0011](docs/adr/0011-secure-by-default.md).

All first-party code is Rust with `unsafe` forbidden. Packaging metadata
(`APKBUILD`) is shell because Alpine's build system requires it; it contains no
logic beyond build recipes.

Proprietary and glibc-only software runs via Flatpak, sandboxed, rather than
natively — see [ADR 0003](docs/adr/0003-foundational-choices.md) for that and
the other founding decisions.

## Status

Early, and installable: the ISOs attached to each release boot an installer,
and the packages come from a signed repository. The build pipeline runs end to
end:

```
make iso     # build a bootable ISO in the Alpine builder container
make smoke   # boot it in QEMU and assert what it reports of Hyprland
```

`make iso` builds `alpymist` into a signed apk, assembles an Alpine image
around it with `mkimage`, and drops the ISO in `out/`. `make smoke` boots that
ISO under QEMU with a virtio-gpu, captures the serial console, and checks that
the first-boot probe said how Hyprland will do and explained itself.

On any Linux machine you can also just run the probe directly:

```
cargo run -p alpymist -- probe
```

## Repository layout

| Path             | Contents                                              |
|------------------|-------------------------------------------------------|
| `crates/`        | Rust workspace: CLI and settings, probes, installer, the desktop's own programs |
| `aports/`        | `APKBUILD`s for Alpymist packages                          |
| `desktop/`       | What the desktop package installs: configuration, PAM and polkit rules |
| `profiles/`      | `mkimage` profiles and `genapkovl` overlays            |
| `builder/`       | The Alpine container everything is built in            |
| `ci/`            | Package and image build scripts, and [how publishing works](ci/README.md) |
| `xtask/`         | Version, smoke-test and publish orchestration (Rust)   |
| `docs/manual/`   | [The manual](https://alpymist.org/manual/), one file a chapter |
| `docs/adr/`      | Architecture decision records                          |
| `site/`          | [alpymist.org](https://alpymist.org), in SvelteKit     |

## Development

Requires Docker (for Alpine builds) and a Rust toolchain. `make help` lists the
targets.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. Contributions are accepted under the same terms.
