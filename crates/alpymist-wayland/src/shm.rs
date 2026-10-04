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
//!
//! What keeps the copy small is that only what a buffer has wrong is written.
//! A host says which parts of its picture changed, for the compositor's
//! sake; the same says which pixels of each buffer are now old.

use std::fs::File;
use std::io::{self, IoSlice};
use std::ops::Range;
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;

use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
use wayland_client::QueueHandle;
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_shm, wl_surface::WlSurface};

use crate::state::State;
use crate::{Area, Pixels, Rows};

/// What of a buffer is older than the picture: for each row, the pixels from
/// the first that is to past the last, and nothing where the two meet.
///
/// A surface's buffers take turns, so the one about to be shown again last
/// held the picture two or three frames ago, and what it is missing is
/// everything that changed since: more than the last frame's damage, and
/// usually far less than all of it.
struct Stale {
    width: u32,
    rows: Vec<(u32, u32)>,
}

impl Stale {
    /// All of it: a buffer that has never held anything.
    fn all(width: u32, height: u32) -> Self {
        Self {
            width,
            rows: vec![(0, width); height as usize],
        }
    }

    /// Part of the picture changed. `None` is all of it.
    fn mark(&mut self, area: Option<&Area>) {
        let Some(area) = area else {
            self.rows.fill((0, self.width));
            return;
        };
        let edge = |v: i32, most: u32| u32::try_from(v).unwrap_or(0).min(most);
        let height = u32::try_from(self.rows.len()).unwrap_or(u32::MAX);
        let left = edge(area.x, self.width);
        let right = edge(area.x.saturating_add(area.width), self.width);
        let top = edge(area.y, height);
        let bottom = edge(area.y.saturating_add(area.height), height);
        if left >= right || top >= bottom {
            return;
        }
        for (from, to) in &mut self.rows[top as usize..bottom as usize] {
            (*from, *to) = if from < to {
                ((*from).min(left), (*to).max(right))
            } else {
                (left, right)
            };
        }
    }

    /// The pixels to write, counted from the picture's first, and nothing
    /// stale after.
    fn take(&mut self) -> Vec<Range<usize>> {
        let width = self.width as usize;
        let mut runs: Vec<Range<usize>> = Vec::new();
        for (row, (from, to)) in self.rows.iter().enumerate() {
            if from >= to {
                continue;
            }
            let run = row * width + *from as usize..row * width + *to as usize;
            // Rows stale from edge to edge lie end to end in the file, and
            // go in one write however many there are.
            match runs.last_mut() {
                Some(last) if last.end == run.start => last.end = run.end,
                _ => runs.push(run),
            }
        }
        self.rows.fill((0, 0));
        runs
    }
}

/// One buffer, and whose it is.
pub(crate) struct Buffer {
    pub(crate) wl: WlBuffer,
    file: File,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) format: wl_shm::Format,
    /// Whether the compositor still has it. Written to only when not.
    pub(crate) busy: bool,
    stale: Stale,
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
            stale: Stale::all(width.unsigned_abs(), height.unsigned_abs()),
            owner: owner.clone(),
        })
    }

    /// Note that part of the picture has changed, whether or not this is the
    /// buffer it is about to be written to. `None` is all of it.
    pub(crate) fn changed(&mut self, area: Option<&Area>) {
        self.stale.mark(area);
    }

    /// Bring it up to the picture by writing what it has wrong.
    pub(crate) fn fill(&mut self, pixels: &mut Pixels<'_>) -> io::Result<()> {
        let width = self.width.unsigned_abs() as usize;
        for run in self.stale.take() {
            match pixels {
                Pixels::Whole(pixels) => {
                    let start = run.start;
                    let run = pixels
                        .get(run)
                        .ok_or_else(|| io::Error::other("a picture smaller than its buffer"))?;
                    self.file
                        .write_all_at(bytemuck::cast_slice(run), (start * 4) as u64)?;
                }
                Pixels::Rows(rows) => self.fill_from(&mut **rows, run, width)?,
            }
        }
        Ok(())
    }

    /// Write the pixels `run` of the picture, asking for each row it touches.
    fn fill_from(&self, rows: &mut dyn Rows, run: Range<usize>, width: usize) -> io::Result<()> {
        let mut at = run.start;
        while at < run.end {
            let from = at % width;
            let to = width.min(from + (run.end - at));
            let y = u32::try_from(at / width).map_err(io::Error::other)?;
            let (row, same) = rows.row(y);
            let row = row
                .get(..width)
                .ok_or_else(|| io::Error::other("a row narrower than its picture"))?;
            if from == 0 && to == width {
                // Whole rows, and as many of them as are this one again, in
                // one write: they lie end to end in the file.
                let most = (run.end - at) / width;
                let count = (same.max(1) as usize).min(most).clamp(1, MOST_AT_ONCE);
                self.write_again(bytemuck::cast_slice(row), count, (at * 4) as u64)?;
                at += count * width;
            } else {
                self.file
                    .write_all_at(bytemuck::cast_slice(&row[from..to]), (at * 4) as u64)?;
                at += to - from;
            }
        }
        Ok(())
    }

    /// Write `row` `count` times over, one after another, from `offset`.
    fn write_again(&self, row: &[u8], count: usize, offset: u64) -> io::Result<()> {
        let again = [IoSlice::new(row); MOST_AT_ONCE];
        let wrote = rustix::io::pwritev(&self.file, &again[..count], offset)?;
        // All of it, nearly always. What a short write left is written a row
        // at a time, from wherever in a row it stopped.
        let mut done = wrote;
        while done < row.len() * count {
            let rest = &row[done % row.len()..];
            self.file.write_all_at(rest, offset + done as u64)?;
            done += rest.len();
        }
        Ok(())
    }
}

/// The most rows written in one go. More than any block is tall.
const MOST_AT_ONCE: usize = 64;

impl Drop for Buffer {
    fn drop(&mut self) {
        self.wl.destroy();
    }
}

#[cfg(test)]
// One run to write is a list of one range, and is meant.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn area(x: i32, y: i32, width: i32, height: i32) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_new_buffer_is_written_whole_and_in_one_piece() {
        let mut stale = Stale::all(8, 4);
        assert_eq!(stale.take(), [0..32]);
        assert!(stale.take().is_empty(), "and then has nothing wrong");
    }

    #[test]
    fn what_changed_is_what_is_written() {
        let mut stale = Stale::all(8, 4);
        stale.take();
        stale.mark(Some(&area(2, 1, 3, 2)));
        assert_eq!(stale.take(), [10..13, 18..21]);
    }

    #[test]
    fn changes_to_a_row_join_into_one_span() {
        let mut stale = Stale::all(8, 4);
        stale.take();
        stale.mark(Some(&area(1, 2, 1, 1)));
        stale.mark(Some(&area(6, 2, 1, 1)));
        assert_eq!(stale.take(), [17..23]);
    }

    #[test]
    fn whole_rows_one_after_another_are_one_write() {
        let mut stale = Stale::all(8, 4);
        stale.take();
        stale.mark(Some(&area(0, 1, 8, 2)));
        stale.mark(Some(&area(0, 3, 4, 1)));
        assert_eq!(stale.take(), [8..28]);
    }

    #[test]
    fn a_change_past_the_edges_is_cut_to_the_buffer() {
        let mut stale = Stale::all(8, 4);
        stale.take();
        stale.mark(Some(&area(-3, -1, 5, 2)));
        stale.mark(Some(&area(6, 3, 100, 100)));
        stale.mark(Some(&area(9, 0, 4, 4)));
        stale.mark(Some(&area(0, 0, 0, 4)));
        stale.mark(Some(&area(i32::MAX, i32::MAX, i32::MAX, i32::MAX)));
        assert_eq!(stale.take(), [0..2, 30..32]);
    }

    #[test]
    fn everything_changed_is_everything() {
        let mut stale = Stale::all(8, 4);
        stale.take();
        stale.mark(Some(&area(3, 3, 1, 1)));
        stale.mark(None);
        assert_eq!(stale.take(), [0..32]);
    }
}
