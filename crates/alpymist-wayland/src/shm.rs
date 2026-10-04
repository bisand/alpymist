//! Pictures in shared memory.
//!
//! A `wl_shm` buffer is a file both sides have: the compositor maps it, and
//! the client puts pixels in it. Most clients map it too. Mapping a file is
//! `unsafe` — nothing stops the other side from cutting it short under a
//! reference — and ADR 0004 keeps `unsafe` out of everything but two files, so
//! this writes instead: the picture is painted in ordinary memory, and handed
//! over with one `pwrite`. That is one copy more than painting into a
//! mapping, and the same copy the lock screen already made on purpose, for
//! the reason its `shadow` gives: a fresh mapping is cold, and blending into
//! it reads every page back.

use std::fs::File;
use std::io;
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;

use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
use wayland_client::QueueHandle;
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_shm, wl_surface::WlSurface};

use crate::state::State;

/// One buffer, and whose it is.
pub(crate) struct Buffer {
    pub(crate) wl: WlBuffer,
    file: File,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) format: wl_shm::Format,
    /// Whether the compositor still has it. Written to only when not.
    pub(crate) busy: bool,
    pub(crate) owner: WlSurface,
}

impl Buffer {
    /// A buffer of `width` by `height` for `owner`.
    pub(crate) fn new(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<State>,
        (width, height): (i32, i32),
        format: wl_shm::Format,
        owner: &WlSurface,
    ) -> io::Result<Self> {
        let bytes = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes > 0)
            .ok_or_else(|| io::Error::other("a picture of no size, or too large for wl_shm"))?;
        let file = File::from(memfd_create(
            c"alpymist-wayland",
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )?);
        file.set_len(u64::from(bytes.unsigned_abs()))?;
        // The compositor maps this; a file that cannot shrink is one it can
        // read without being killed by a truncation.
        let _ = fcntl_add_seals(&file, SealFlags::SHRINK | SealFlags::SEAL);
        let pool = shm.create_pool(file.as_fd(), bytes, qh, ());
        let wl = pool.create_buffer(0, width, height, width * 4, format, qh, ());
        pool.destroy();
        Ok(Self {
            wl,
            file,
            width,
            height,
            format,
            busy: false,
            owner: owner.clone(),
        })
    }

    /// Put `pixels` in it: the whole picture.
    pub(crate) fn fill(&self, pixels: &[u32]) -> io::Result<()> {
        self.file.write_all_at(bytemuck::cast_slice(pixels), 0)
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        self.wl.destroy();
    }
}
