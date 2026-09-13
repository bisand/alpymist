//! The frame every wizard screen is drawn inside.
//!
//! Pure geometry: where the panel sits and how it divides into header, body and
//! footer. Rendering lives in [`crate::render`]. Keeping the two apart means the
//! layout can be checked at every screen size we care about without a display —
//! and the sizes that matter here go down to 640×480.

use crate::convert::px;

/// Where each part of a wizard screen sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chrome {
    /// The panel as `(x, y, width, height)`.
    pub panel: (i32, i32, i32, i32),
    /// Inside the panel, above the divider.
    pub header: (i32, i32, i32, i32),
    /// Top-left of the "STEP n OF m" line.
    pub counter_at: (i32, i32),
    /// Top-left of the screen title.
    pub title_at: (i32, i32),
    /// Top-left of the one-line subtitle.
    pub subtitle_at: (i32, i32),
    /// Y of the rule under the header.
    pub rule_y: i32,
    /// The content area.
    pub body: (i32, i32, i32, i32),
    /// Key hints and advisories, at the bottom.
    pub footer: (i32, i32, i32, i32),
    /// The "Back" button, as `(x, y, width, height)`.
    pub back_button: (i32, i32, i32, i32),
    /// The primary button, right-aligned.
    pub next_button: (i32, i32, i32, i32),
    /// Where an advisory line sits, above the buttons.
    pub advisory_at: (i32, i32),
    /// Text scale for the panel title.
    pub title_scale: i32,
    /// Text scale for body and footer text.
    pub text_scale: i32,
    /// Padding inside the panel edge.
    pub padding: i32,
}

/// Fraction of the screen width the panel occupies, in percent.
const PANEL_WIDTH_PCT: i32 = 74;
/// Fraction of the screen height the panel occupies, in percent.
const PANEL_HEIGHT_PCT: i32 = 62;
/// How far down the screen the panel starts, in percent. Keeps the horizon
/// clear at every size, including the small ones.
const TOP_MARGIN_PCT: i32 = 34;
/// Bottom margins to try, in percent of the height, most generous first. Without
/// a cap the margin would match the sides, which on a wide screen costs the body
/// the rows the longest page needs.
const BOTTOM_MARGIN_STEPS: [i32; 3] = [12, 10, 8];
/// Height of the built-in font cell, before scaling.
const CELL_HEIGHT: i32 = 8;
/// Body rows the longest screen needs. The installer's own test checks every
/// real screen against the panel, so this cannot silently drift below it.
pub const MIN_BODY_ROWS: i32 = 8;
/// Advance width of one glyph cell, before scaling.
const CELL_ADVANCE: i32 = 6;

/// How wide a button has to be to hold `label` with room around it.
fn button_width(label: &str, text_scale: i32, gap: i32) -> i32 {
    let chars = i32::try_from(label.chars().count()).unwrap_or(0);
    chars * CELL_ADVANCE * text_scale + gap * 4
}

impl Chrome {
    /// Lay out a wizard screen for this display size.
    ///
    /// The panel sits low rather than centred: the mountains occupy the upper
    /// half, and covering them with a dialogue would waste the only thing on
    /// screen that makes this look like anything.
    #[must_use]
    pub fn for_screen(width: u32, height: u32) -> Self {
        let w = px(width.max(1));
        let h = px(height.max(1));

        // The largest text that still leaves room for the longest screen.
        // A height threshold was tried first and guessed wrong in both
        // directions: it overflowed a 1366x768 laptop at one setting and made
        // a 1024x600 netbook's text needlessly tiny at the next. Measuring the
        // body at each scale cannot guess wrong.
        let widest = if w >= 1600 {
            3
        } else if w >= 1024 {
            2
        } else {
            1
        };
        //
        // Readable text wins over a perfectly proportioned margin: at each
        // scale the bottom margin may give ground first, down to a floor that
        // still reads as balanced, and only then does the text get smaller.
        // Without that, a 1366x768 laptop fell to 8 px text one row short of
        // fitting, with most of its panel empty.
        (1..=widest)
            .rev()
            .flat_map(|scale| BOTTOM_MARGIN_STEPS.iter().map(move |pct| (scale, *pct)))
            .map(|(scale, pct)| Self::layout(w, h, scale, pct))
            .find(|c| c.body_rows() >= MIN_BODY_ROWS && c.bottom_is_balanced(h))
            .unwrap_or_else(|| Self::layout(w, h, 1, BOTTOM_MARGIN_STEPS[0]))
    }

    /// Whether the gap under the panel still reads as a margin rather than as
    /// the panel sitting on the floor.
    fn bottom_is_balanced(&self, h: i32) -> bool {
        let bottom = h - (self.panel.1 + self.panel.3);
        bottom >= self.padding * 2 && bottom * 3 >= self.panel.0
    }

    /// Lay out the panel for one text scale and bottom margin.
    fn layout(w: i32, h: i32, text_scale: i32, bottom_pct: i32) -> Self {
        let title_scale = text_scale + 1;
        let padding = (text_scale * 12).max(10);

        // The top margin is fixed first and the panel shrinks to fit below it,
        // rather than the panel taking a fixed share and being positioned after.
        // On a 640x480 screen the second order gives the panel so much height
        // that it rides up over the horizon — which is the one thing on screen
        // worth looking at.
        let top_margin = h * TOP_MARGIN_PCT / 100;
        let panel_w = (w * PANEL_WIDTH_PCT / 100).min(w - padding * 2).max(0);

        // The panel sits low, but not on the floor. Growing it down to fill
        // everything below the top margin left a 24 px gap at 1280x800 against
        // 166 px at the sides, so it looked like it was sliding off the screen.
        // The bottom margin follows the side margins, capped so a very wide
        // screen does not squeeze the body until the longest page stops fitting.
        let side_margin = (w - panel_w) / 2;
        let bottom_margin = side_margin.min(h * bottom_pct / 100).max(padding * 2);
        let panel_h = (h * PANEL_HEIGHT_PCT / 100)
            .min(h - top_margin - bottom_margin)
            .max(0);
        let panel_x = (w - panel_w) / 2;
        let panel_y = top_margin;

        let inner_x = panel_x + padding;
        let inner_w = panel_w - padding * 2;

        // Header rows are positioned here rather than by the caller, so the
        // header's height and the lines inside it cannot disagree — which is
        // exactly how the subtitle ended up overlapping the body.
        let gap = text_scale * 4;
        let header_y = panel_y + padding;
        let counter_at = (inner_x, header_y);
        let title_at = (inner_x, header_y + CELL_HEIGHT * text_scale + gap);
        let subtitle_at = (inner_x, title_at.1 + CELL_HEIGHT * title_scale + gap);
        let header_bottom = subtitle_at.1 + CELL_HEIGHT * text_scale;
        let rule_y = header_bottom + gap;
        let header_h = rule_y - header_y;

        let footer_h = CELL_HEIGHT * text_scale * 3 + padding;
        let body_y = rule_y + gap * 2;
        let footer_y = panel_y + panel_h - padding - footer_h;
        let body_h = (footer_y - body_y - gap).max(0);

        // Buttons sit on the footer's baseline, Back at the left and the primary
        // action at the right — the order people already expect, so nobody has
        // to read them to know which is which.
        let button_h = CELL_HEIGHT * text_scale + gap * 3;
        let button_y = footer_y + footer_h - button_h;
        let back_w = button_width("Esc  Back", text_scale, gap);
        let next_w = button_width("Enter  Continue", text_scale, gap);
        let advisory_at = (inner_x, button_y - CELL_HEIGHT * text_scale - gap * 2);

        Self {
            back_button: (inner_x, button_y, back_w, button_h),
            next_button: (inner_x + inner_w - next_w, button_y, next_w, button_h),
            advisory_at,
            panel: (panel_x, panel_y, panel_w, panel_h),
            header: (inner_x, header_y, inner_w, header_h),
            counter_at,
            title_at,
            subtitle_at,
            rule_y,
            body: (inner_x, body_y, inner_w, body_h),
            footer: (inner_x, footer_y, inner_w, footer_h),
            title_scale,
            text_scale,
            padding,
        }
    }

    /// The rectangle a body row occupies, for hit testing a pointer.
    ///
    /// Full body width rather than the width of the text, so clicking anywhere
    /// along a row selects it — aiming at the glyphs would be needlessly fussy,
    /// especially on a touchpad.
    #[must_use]
    pub fn row_rect(&self, index: i32) -> (i32, i32, i32, i32) {
        (
            self.body.0,
            self.body_row(index),
            self.body.2,
            self.row_height(),
        )
    }

    /// The vertical distance between one body row and the next.
    #[must_use]
    pub fn row_height(&self) -> i32 {
        CELL_HEIGHT * self.text_scale + self.text_scale * 4
    }

    /// The y of a body row, `index` lines down from the top of the body.
    #[must_use]
    pub fn body_row(&self, index: i32) -> i32 {
        self.body.1 + index * self.row_height()
    }

    /// How many rows fit in the body.
    #[must_use]
    pub fn body_rows(&self) -> i32 {
        (self.body.3 / self.row_height().max(1)).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::Chrome;
    use crate::convert::px;

    /// Every resolution we claim to support, from a netbook to a modern panel.
    const SIZES: [(u32, u32); 7] = [
        (640, 480),
        (800, 600),
        (1024, 600),
        (1024, 768),
        (1366, 768),
        (1920, 1080),
        (2560, 1440),
    ];

    #[test]
    fn the_panel_stays_on_screen_at_every_size() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            let (x, y, pw, ph) = c.panel;
            assert!(x >= 0 && y >= 0, "{w}x{h}: panel at {x},{y}");
            assert!(x + pw <= px(w), "{w}x{h}: panel overflows right");
            assert!(y + ph <= px(h), "{w}x{h}: panel overflows bottom");
        }
    }

    #[test]
    fn header_body_and_footer_stay_inside_the_panel_and_do_not_overlap() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            let (px_, py, pw, ph) = c.panel;
            for (name, (x, y, rw, rh)) in
                [("header", c.header), ("body", c.body), ("footer", c.footer)]
            {
                assert!(
                    x >= px_ && y >= py,
                    "{w}x{h}: {name} starts outside the panel"
                );
                assert!(
                    x + rw <= px_ + pw,
                    "{w}x{h}: {name} overflows the panel width"
                );
                assert!(
                    y + rh <= py + ph,
                    "{w}x{h}: {name} overflows the panel height"
                );
            }
            assert!(
                c.header.1 + c.header.3 <= c.body.1,
                "{w}x{h}: header overlaps body"
            );
            assert!(
                c.body.1 + c.body.3 <= c.footer.1,
                "{w}x{h}: body overlaps footer"
            );
        }
    }

    #[test]
    fn the_panel_is_horizontally_centred() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            let (x, _, pw, _) = c.panel;
            let right_gap = px(w) - (x + pw);
            assert!(
                (x - right_gap).abs() <= 1,
                "{w}x{h}: off-centre by {}",
                (x - right_gap).abs()
            );
        }
    }

    /// The mountains are the whole point of the backdrop; a panel that covers
    /// them makes the installer look like a dialogue box on a wallpaper.
    #[test]
    fn the_panel_leaves_the_upper_third_of_the_screen_clear() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            assert!(
                c.panel.1 > px(h) / 3,
                "{w}x{h}: panel starts at {} which covers the horizon",
                c.panel.1
            );
        }
    }

    #[test]
    fn the_body_always_has_room_for_at_least_a_few_rows() {
        for (w, h) in SIZES {
            let rows = Chrome::for_screen(w, h).body_rows();
            assert!(rows >= 3, "{w}x{h}: only {rows} body rows");
        }
    }

    #[test]
    fn body_rows_advance_downwards_and_stay_within_the_body() {
        let c = Chrome::for_screen(1280, 800);
        assert!(c.body_row(1) > c.body_row(0));
        let last = c.body_row(c.body_rows() - 1);
        assert!(
            last < c.body.1 + c.body.3,
            "last row {last} spills past the body"
        );
    }

    /// The bug this guards: the header reported a height that did not match
    /// where its own lines were drawn, so the subtitle ran into the body.
    #[test]
    fn the_subtitle_never_runs_into_the_body() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            let subtitle_bottom = c.subtitle_at.1 + 8 * c.text_scale;
            assert!(
                subtitle_bottom <= c.rule_y,
                "{w}x{h}: subtitle ends at {subtitle_bottom}, rule at {}",
                c.rule_y
            );
            assert!(c.rule_y < c.body.1, "{w}x{h}: rule sits inside the body");
        }
    }

    #[test]
    fn header_lines_run_top_to_bottom_in_order() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            assert!(c.counter_at.1 < c.title_at.1, "{w}x{h}");
            assert!(c.title_at.1 < c.subtitle_at.1, "{w}x{h}");
            assert_eq!(
                c.counter_at.0, c.title_at.0,
                "{w}x{h}: header not left-aligned"
            );
            assert_eq!(
                c.title_at.0, c.subtitle_at.0,
                "{w}x{h}: header not left-aligned"
            );
        }
    }

    #[test]
    fn a_row_rectangle_spans_the_body_and_sits_on_its_row() {
        let chrome = Chrome::for_screen(1280, 800);
        for index in 0..3 {
            let (left, top, width, height) = chrome.row_rect(index);
            assert_eq!(left, chrome.body.0);
            assert_eq!(
                width, chrome.body.2,
                "rows are clickable across the whole body"
            );
            assert_eq!(top, chrome.body_row(index));
            assert!(height > 0);
        }
    }

    #[test]
    fn row_rectangles_touch_without_overlapping() {
        let c = Chrome::for_screen(1280, 800);
        let (_, y0, _, h0) = c.row_rect(0);
        let (_, y1, _, _) = c.row_rect(1);
        assert_eq!(y0 + h0, y1, "a gap here would be a dead strip between rows");
    }

    #[test]
    fn bigger_screens_get_bigger_text() {
        assert!(
            Chrome::for_screen(1920, 1080).text_scale > Chrome::for_screen(640, 480).text_scale
        );
    }

    #[test]
    fn the_buttons_sit_inside_the_panel_and_do_not_overlap_each_other() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            let (px_, py, pw, ph) = c.panel;
            for (name, (x, y, bw, bh)) in [("back", c.back_button), ("next", c.next_button)] {
                assert!(
                    x >= px_ && y >= py,
                    "{w}x{h}: {name} starts outside the panel"
                );
                assert!(x + bw <= px_ + pw, "{w}x{h}: {name} overflows the panel");
                assert!(
                    y + bh <= py + ph,
                    "{w}x{h}: {name} runs past the panel bottom"
                );
            }
            let back_right = c.back_button.0 + c.back_button.2;
            assert!(back_right < c.next_button.0, "{w}x{h}: the buttons overlap");
        }
    }

    #[test]
    fn the_primary_button_is_right_aligned_with_the_body() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            let button_right = c.next_button.0 + c.next_button.2;
            let body_right = c.body.0 + c.body.2;
            assert_eq!(
                button_right, body_right,
                "{w}x{h}: primary button not flush right"
            );
        }
    }

    #[test]
    fn an_advisory_has_room_above_the_buttons() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            assert!(
                c.advisory_at.1 + 8 * c.text_scale <= c.back_button.1,
                "{w}x{h}: advisory would run into the buttons"
            );
            assert!(
                c.advisory_at.1 >= c.footer.1,
                "{w}x{h}: advisory sits above the footer"
            );
        }
    }

    /// The bug this guards: the panel grew to the bottom edge and left a
    /// margin a seventh the size of the ones beside it.
    #[test]
    fn the_panel_does_not_sit_on_the_bottom_edge() {
        for (w, h) in SIZES {
            let c = Chrome::for_screen(w, h);
            let (x, y, _, ph) = c.panel;
            let bottom = px(h) - (y + ph);
            assert!(
                bottom >= c.padding * 2,
                "{w}x{h}: bottom margin only {bottom}px"
            );
            assert!(
                bottom * 3 >= x,
                "{w}x{h}: bottom margin {bottom}px is out of proportion to the sides ({x}px)"
            );
        }
    }

    /// Shrinking the text is the last resort, not the first. Every size with
    /// room for scale 2 at width must get scale 2 at the heights we support.
    #[test]
    fn ordinary_laptop_screens_keep_readable_text() {
        for (w, h) in [(1024, 768), (1280, 800), (1366, 768), (1920, 1080)] {
            let c = Chrome::for_screen(w, h);
            assert!(
                c.text_scale >= 2,
                "{w}x{h}: text fell to scale {}",
                c.text_scale
            );
            assert!(c.body_rows() >= super::MIN_BODY_ROWS, "{w}x{h}");
        }
    }

    #[test]
    fn a_degenerate_size_does_not_panic() {
        for (w, h) in [(1, 1), (16, 16), (320, 240)] {
            let c = Chrome::for_screen(w, h);
            assert!(c.panel.2 >= 0 && c.panel.3 >= 0);
        }
    }
}
