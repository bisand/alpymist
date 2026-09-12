# CI

Not yet wired. Planned gates, in order of introduction:

1. `make lint` and `make test` on every PR.
2. `cargo deny check` and `cargo audit`.
3. Package builds for `x86_64` and `aarch64` in the pinned builder container.
4. ISO build + automated QEMU boot smoke test per tier.
5. Double-build reproducibility diff as a release gate.
