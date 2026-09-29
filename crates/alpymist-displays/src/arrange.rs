//! Arranging screens by hand: how much room each takes in the layout, and
//! where one lands when it is dragged or moved with the keys.
//!
//! A screen dropped anywhere is put against the nearest edge of another,
//! touching it and not over it, lined up with it when it was dropped close to
//! lined up; Hyprland would accept a gap or an overlap, but the pointer would
//! then fall off one screen before reaching the next, or two screens would
//! show the same part of the desktop. Then the whole layout is moved so its
//! top left corner is at 0,0, which is where Hyprland expects it.

use crate::layout::Output;
use crate::screen::{self, Monitor};

/// A screen's room in the layout, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Room {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

impl Room {
    fn right(self) -> i32 {
        self.x + self.w
    }

    fn bottom(self) -> i32 {
        self.y + self.h
    }

    fn overlaps(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    fn at(self, x: i32, y: i32) -> Self {
        Self { x, y, ..self }
    }
}

/// How close to lined up counts as lined up, in logical pixels.
const ALIGN: i32 = 48;

/// The size `output` takes in the layout: its mode, or the screen's own
/// preferred mode, turned and scaled.
#[must_use]
pub fn size(output: &Output, monitor: &Monitor) -> (i32, i32) {
    let (w, h) = pixels(&output.mode)
        .or_else(|| monitor.available_modes.first().and_then(|m| pixels(m)))
        .unwrap_or((monitor.width, monitor.height));
    let (w, h) = if output.transform % 2 == 1 {
        (h, w)
    } else {
        (w, h)
    };
    (
        screen::logical(w, output.scale),
        screen::logical(h, output.scale),
    )
}

/// `1920x1080@60.00` or `1920x1080@60.00Hz` as its width and height.
#[must_use]
pub fn pixels(mode: &str) -> Option<(i32, i32)> {
    let size = mode.split('@').next()?;
    let (w, h) = size.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// Where the screen at `moved` goes, dropped with its top left corner at
/// `at`: against the nearest edge of another screen. Every room is then moved
/// so the layout starts at 0,0.
pub fn drop(rooms: &mut [Room], moved: usize, at: (i32, i32)) {
    if moved >= rooms.len() {
        return;
    }
    if let Some(best) = places(rooms, moved)
        .into_iter()
        .min_by_key(|p| distance(*p, at))
    {
        rooms[moved] = rooms[moved].at(best.0, best.1);
    }
    settle(rooms);
}

/// Move the screen at `moved` one place in a direction, `(-1, 0)` being
/// left: to the nearest place against another screen that lies that way.
/// Whether it moved.
pub fn nudge(rooms: &mut [Room], moved: usize, (dx, dy): (i32, i32)) -> bool {
    let Some(&from) = rooms.get(moved) else {
        return false;
    };
    // A real step that way, at least half its own size: not a pixel over
    // and a row down.
    let ahead = |p: (i32, i32)| {
        (p.0 - from.x) * dx >= from.w / 2 * dx.abs() && (p.1 - from.y) * dy >= from.h / 2 * dy.abs()
    };
    let Some(best) = places(rooms, moved)
        .into_iter()
        .filter(|p| ahead(*p))
        // In line first, then nearest: Left moves along the row it is in.
        .min_by_key(|p| {
            let along = (p.0 - from.x) * dx + (p.1 - from.y) * dy;
            let across = (p.0 - from.x) * dy.abs() + (p.1 - from.y) * dx.abs();
            (across.abs(), along.abs())
        })
    else {
        return false;
    };
    rooms[moved] = from.at(best.0, best.1);
    settle(rooms);
    true
}

/// Everywhere the screen at `moved` could go against another without
/// covering any: beside each one, top or bottom edges lined up or anywhere
/// they still share an edge, and above or below each, likewise.
fn places(rooms: &[Room], moved: usize) -> Vec<(i32, i32)> {
    let me = rooms[moved];
    let others: Vec<Room> = rooms
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != moved)
        .map(|(_, r)| *r)
        .collect();
    let mut places = Vec::new();
    for o in &others {
        for x in [o.right(), o.x - me.w] {
            for y in [
                o.y,
                o.bottom() - me.h,
                me.y.clamp(o.y - me.h + 1, o.bottom() - 1),
            ] {
                places.push((x, y));
            }
        }
        for y in [o.bottom(), o.y - me.h] {
            for x in [
                o.x,
                o.right() - me.w,
                me.x.clamp(o.x - me.w + 1, o.right() - 1),
            ] {
                places.push((x, y));
            }
        }
    }
    places.retain(|&(x, y)| !others.iter().any(|o| me.at(x, y).overlaps(*o)));
    places
}

/// How far `p` is from `at`, with nearly lined up counting as lined up.
fn distance(p: (i32, i32), at: (i32, i32)) -> i64 {
    let dx = i64::from((p.0 - at.0).abs());
    let dy = i64::from((p.1 - at.1).abs());
    let near = |d: i64| if d <= i64::from(ALIGN) { 0 } else { d };
    near(dx) * near(dx) + near(dy) * near(dy) + dx + dy
}

/// Move everything so the layout's top left corner is at 0,0.
fn settle(rooms: &mut [Room]) {
    let left = rooms.iter().map(|r| r.x).min().unwrap_or(0);
    let top = rooms.iter().map(|r| r.y).min().unwrap_or(0);
    for r in rooms {
        r.x -= left;
        r.y -= top;
    }
}

#[cfg(test)]
mod tests {
    use super::{Room, drop, nudge, pixels};

    fn room(x: i32, y: i32, w: i32, h: i32) -> Room {
        Room { x, y, w, h }
    }

    #[test]
    fn a_dropped_screen_goes_against_the_nearest_edge_lined_up() {
        // The laptop, and a screen to its right.
        let mut rooms = [room(0, 0, 1920, 1200), room(1920, 0, 1920, 1080)];
        // Dragged to the left of the laptop, a little low: it goes against
        // its left edge with the tops lined up, and everything moves so the
        // layout starts at 0,0.
        drop(&mut rooms, 1, (-1900, 30));
        assert_eq!(rooms, [room(1920, 0, 1920, 1200), room(0, 0, 1920, 1080)]);
        // Dropped well below the laptop's middle: under it, not beside it.
        drop(&mut rooms, 1, (1900, 1500));
        assert_eq!(rooms[1].y, rooms[0].y + 1200);
    }

    #[test]
    fn a_dropped_screen_never_covers_another() {
        let mut rooms = [
            room(0, 0, 1920, 1080),
            room(1920, 0, 1920, 1080),
            room(3840, 0, 1920, 1080),
        ];
        // Dropped right on top of the middle one.
        drop(&mut rooms, 2, (1920, 0));
        for (i, a) in rooms.iter().enumerate() {
            for b in &rooms[i + 1..] {
                assert!(!a.overlaps(*b), "{a:?} covers {b:?}");
            }
        }
        assert!(rooms.iter().all(|r| r.x >= 0 && r.y >= 0));
    }

    #[test]
    fn the_keys_move_a_screen_one_place_at_a_time() {
        let mut rooms = [room(0, 0, 1920, 1080), room(1920, 0, 1920, 1080)];
        assert!(nudge(&mut rooms, 1, (-1, 0)));
        assert_eq!(rooms, [room(1920, 0, 1920, 1080), room(0, 0, 1920, 1080)]);
        assert!(nudge(&mut rooms, 1, (0, 1)));
        assert_eq!(rooms[1].y, 1080, "under the other now");
        assert!(
            !nudge(&mut rooms[..1], 0, (1, 0)),
            "alone, it has nowhere to go"
        );
    }

    #[test]
    fn modes_are_read_for_their_size() {
        assert_eq!(pixels("1920x1080@60.00"), Some((1920, 1080)));
        assert_eq!(pixels("2560x1440@59.95Hz"), Some((2560, 1440)));
        assert_eq!(pixels("preferred"), None);
    }
}
