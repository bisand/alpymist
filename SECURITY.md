# Security policy

Alpy's security posture is described in
[ADR 0002](docs/adr/0002-supply-chain.md). In short: no install scripts,
everything signed, reproducible builds, no third-party repositories by default,
and all first-party code in Rust with `unsafe` forbidden.

## Reporting

Report vulnerabilities privately via GitHub Security Advisories on this
repository. Please do not open a public issue. We aim to acknowledge within
72 hours.

## Scope

In scope: the Rust crates in `crates/`, our `APKBUILD`s and image profiles, the
signing and release pipeline, and default system configuration we ship.

Out of scope: upstream Alpine packages (report to Alpine), and upstream
Hyprland/labwc/wlroots issues (report upstream). We will help route these.
