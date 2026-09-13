# CI

Two workflows:

- **CI** (`.github/workflows/ci.yml`): `cargo fmt --check`, clippy with
  `-D warnings`, and the tests, on every push to `main` and every pull request.
  A couple of minutes.
- **ISO** (`.github/workflows/iso.yml`): builds the x86_64 image, boots it in
  QEMU with KVM, and asserts the reported tier. Nightly, skipped when `main`
  has not moved since the last successful image, and on demand from the
  Actions tab ("Run workflow"), which always builds.

Still planned:

1. `cargo deny check` and `cargo audit`.
2. Package builds for `aarch64` in CI.
3. An install test: install onto a scratch disk and boot the result.
4. Double-build reproducibility diff as a release gate.

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
