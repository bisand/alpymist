# Alpymist

An opinionated, curated desktop on top of Alpine Linux — Wayland first, with a
software-rendered path for hardware that cannot do better, and an X11 fallback
for hardware that cannot do Wayland at all.

The name is Alpine plus mist — the haze on the mountains — and a pun on
*alchemist*, which is roughly what turning a fifteen-year-old laptop back into
a usable desktop amounts to.

Two things make it different from the curated-desktop projects it takes
inspiration from:

- **It scales down.** One desktop definition, rendered onto Hyprland, labwc
  (GPU or CPU), or i3 depending on what the machine can actually drive. See
  [ADR 0001](docs/adr/0001-hardware-tiers.md).
- **It ships as signed packages.** No `curl | bash`, no root install scripts,
  no unpinned third-party repos. See [ADR 0002](docs/adr/0002-supply-chain.md).

All first-party code is Rust with `unsafe` forbidden. Packaging metadata
(`APKBUILD`) is shell because Alpine's build system requires it; it contains no
logic beyond build recipes.

Proprietary and glibc-only software runs via Flatpak, sandboxed, rather than
natively — see [ADR 0003](docs/adr/0003-foundational-choices.md) for that and
the other founding decisions.

## Status

Pre-alpha. Nothing is installable yet. What works today:

```
cargo run -p alpymistctl -- probe
```

on a Linux machine, which reports the desktop tier that machine would get and
why.

## Repository layout

| Path             | Contents                                              |
|------------------|-------------------------------------------------------|
| `crates/`        | Rust workspace: domain model, probes, CLI             |
| `aports/`        | `APKBUILD`s for Alpymist packages                          |
| `profiles/`      | `mkimage` profiles and `genapkovl` overlays            |
| `builder/`       | The Alpine container everything is built in            |
| `docs/adr/`      | Architecture decision records                          |

## Development

Requires Docker (for Alpine builds) and a Rust toolchain. `make help` lists the
targets.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. Contributions are accepted under the same terms.
