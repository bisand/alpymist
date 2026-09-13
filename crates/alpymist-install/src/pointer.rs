//! What the pointer is over.
//!
//! Pure geometry, so every boundary case can be tested without a screen or a
//! mouse. Hit testing is the kind of code where an off-by-one means clicking
//! one row and getting the one above it, which is maddening and easy to miss
//! by eye.

use alpymist_ui::chrome::Chrome;

/// What sits under a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// A body row, by index.
    Row(usize),
    /// The Back button.
    Back,
    /// The primary button.
    Next,
    /// Nothing interactive.
    Nothing,
}

/// Whether `(x, y)` lies inside `(rx, ry, rw, rh)`.
///
/// Half-open on the far edges, so adjacent rectangles that share a boundary
/// cannot both claim the same pixel.
fn inside(rect: (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    let (rx, ry, rw, rh) = rect;
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

/// What is under the pointer.
///
/// Buttons are tested before rows: they are drawn on top, and the footer sits
/// outside the body anyway, but the order makes that explicit rather than
/// incidental.
#[must_use]
pub fn hit_test(chrome: &Chrome, rows: usize, x: i32, y: i32) -> Hit {
    if inside(chrome.back_button, x, y) {
        return Hit::Back;
    }
    if inside(chrome.next_button, x, y) {
        return Hit::Next;
    }
    for index in 0..rows {
        let row = i32::try_from(index).unwrap_or(i32::MAX);
        if inside(chrome.row_rect(row), x, y) {
            return Hit::Row(index);
        }
    }
    Hit::Nothing
}

#[cfg(test)]
mod tests {
    use super::{Hit, hit_test};
    use alpymist_ui::chrome::Chrome;

    fn chrome() -> Chrome {
        Chrome::for_screen(1280, 800)
    }

    fn centre(rect: (i32, i32, i32, i32)) -> (i32, i32) {
        (rect.0 + rect.2 / 2, rect.1 + rect.3 / 2)
    }

    #[test]
    fn clicking_a_row_finds_that_row() {
        let c = chrome();
        for index in 0..5 {
            let (x, y) = centre(c.row_rect(i32::try_from(index).unwrap()));
            assert_eq!(hit_test(&c, 8, x, y), Hit::Row(index));
        }
    }

    #[test]
    fn clicking_a_button_finds_that_button() {
        let c = chrome();
        let (bx, by) = centre(c.back_button);
        assert_eq!(hit_test(&c, 8, bx, by), Hit::Back);
        let (nx, ny) = centre(c.next_button);
        assert_eq!(hit_test(&c, 8, nx, ny), Hit::Next);
    }

    #[test]
    fn clicking_the_backdrop_hits_nothing() {
        let c = chrome();
        assert_eq!(hit_test(&c, 8, 5, 5), Hit::Nothing, "above the panel");
        assert_eq!(
            hit_test(&c, 8, 1275, 795),
            Hit::Nothing,
            "bottom-right corner"
        );
    }

    #[test]
    fn a_row_beyond_the_ones_that_exist_is_not_reported() {
        let c = chrome();
        let (x, y) = centre(c.row_rect(6));
        assert_eq!(hit_test(&c, 3, x, y), Hit::Nothing, "only 3 rows exist");
    }

    /// The bug this guards: adjacent rows sharing an edge, so a click on the
    /// boundary lands on the row above the one under the cursor.
    #[test]
    fn the_boundary_between_two_rows_belongs_to_exactly_one_of_them() {
        let c = chrome();
        let (_, y0, _, h0) = c.row_rect(0);
        let x = c.body.0 + 10;
        assert_eq!(
            hit_test(&c, 4, x, y0),
            Hit::Row(0),
            "top edge belongs to its own row"
        );
        assert_eq!(
            hit_test(&c, 4, x, y0 + h0 - 1),
            Hit::Row(0),
            "last pixel is still row 0"
        );
        assert_eq!(
            hit_test(&c, 4, x, y0 + h0),
            Hit::Row(1),
            "next pixel is row 1"
        );
    }

    #[test]
    fn a_row_is_clickable_across_the_whole_body_width() {
        let c = chrome();
        let (rx, ry, rw, rh) = c.row_rect(1);
        let y = ry + rh / 2;
        assert_eq!(hit_test(&c, 4, rx, y), Hit::Row(1), "left edge");
        assert_eq!(hit_test(&c, 4, rx + rw - 1, y), Hit::Row(1), "right edge");
        assert_eq!(hit_test(&c, 4, rx - 1, y), Hit::Nothing, "just outside");
        assert_eq!(hit_test(&c, 4, rx + rw, y), Hit::Nothing, "just outside");
    }

    #[test]
    fn buttons_win_over_rows_where_they_could_overlap() {
        // Rows are asked about only after both buttons, so even a pathological
        // layout cannot let a row swallow a button.
        let c = chrome();
        let (bx, by) = centre(c.next_button);
        assert_eq!(hit_test(&c, 200, bx, by), Hit::Next);
    }

    #[test]
    fn hit_testing_works_at_every_screen_size_we_support() {
        for (w, h) in [(640, 480), (1024, 768), (1920, 1080), (2560, 1440)] {
            let c = Chrome::for_screen(w, h);
            let (x, y) = centre(c.row_rect(0));
            assert_eq!(hit_test(&c, 4, x, y), Hit::Row(0), "{w}x{h}");
            let (bx, by) = centre(c.next_button);
            assert_eq!(hit_test(&c, 4, bx, by), Hit::Next, "{w}x{h}");
        }
    }
}
