# CI

Not yet wired. Planned gates, in order of introduction:

1. `make lint` and `make test` on every PR.
2. `cargo deny check` and `cargo audit`.
3. Package builds for `x86_64` and `aarch64` in the pinned builder container.
4. ISO build + automated QEMU boot smoke test per tier.
5. Double-build reproducibility diff as a release gate.

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
