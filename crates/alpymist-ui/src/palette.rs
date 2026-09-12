//! The Alpymist palette: cold, misty, high-contrast enough to read on a bad panel.

/// An 8-bit-per-channel colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// A colour from its components.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Blend towards `other` by `amount` percent.
    ///
    /// Used for aerial perspective: each ridge further away is mixed further
    /// towards the sky colour, which is what makes distance read as distance.
    #[must_use]
    pub fn mix(self, other: Self, amount: u32) -> Self {
        let a = amount.min(100);
        let blend = |x: u8, y: u8| -> u8 {
            let x = u32::from(x);
            let y = u32::from(y);
            u8::try_from((x * (100 - a) + y * a) / 100).unwrap_or(u8::MAX)
        };
        Self::new(
            blend(self.r, other.r),
            blend(self.g, other.g),
            blend(self.b, other.b),
        )
    }

    /// Relative luminance, 0-255, for contrast checks.
    #[must_use]
    pub fn luminance(self) -> u8 {
        // Integer approximation of ITU-R BT.601.
        let y =
            (u32::from(self.r) * 299 + u32::from(self.g) * 587 + u32::from(self.b) * 114) / 1000;
        u8::try_from(y).unwrap_or(u8::MAX)
    }
}

/// The colours the splash and installer share.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Sky at the top of the screen.
    pub sky_high: Rgb,
    /// Sky at the horizon, where the mist gathers.
    pub sky_low: Rgb,
    /// The nearest, darkest ridge.
    pub ridge_near: Rgb,
    /// Mist drifting between the ridges.
    pub mist: Rgb,
    /// Primary text.
    pub ink: Rgb,
    /// Secondary text and hints.
    pub ink_dim: Rgb,
    /// Accent for focus rings, progress and the wordmark.
    pub accent: Rgb,
}

impl Palette {
    /// The default cold-mountain palette.
    #[must_use]
    pub const fn alpymist() -> Self {
        Self {
            sky_high: Rgb::new(0x0B, 0x12, 0x1E),
            sky_low: Rgb::new(0x3A, 0x4C, 0x63),
            ridge_near: Rgb::new(0x07, 0x0C, 0x14),
            mist: Rgb::new(0xAF, 0xC2, 0xD6),
            ink: Rgb::new(0xEA, 0xF0, 0xF6),
            ink_dim: Rgb::new(0x9A, 0xAB, 0xBD),
            accent: Rgb::new(0x7F, 0xB8, 0xD9),
        }
    }

    /// The sky colour at `y` of `height`, as a vertical gradient.
    #[must_use]
    pub fn sky_at(&self, y: u32, height: u32) -> Rgb {
        let h = height.max(1);
        self.sky_high.mix(self.sky_low, (y.min(h) * 100) / h)
    }

    /// The colour of a ridge `depth` layers back, hazed towards the horizon.
    ///
    /// `0` is nearest. Further ridges mix towards the sky, which is what makes
    /// the scene read as depth rather than as flat overlapping shapes.
    #[must_use]
    pub fn ridge_at(&self, depth: u32, total: u32) -> Rgb {
        let total = total.max(1);
        let haze = (depth.min(total) * 70) / total;
        self.ridge_near.mix(self.sky_low, haze)
    }
}

#[cfg(test)]
mod tests {
    use super::{Palette, Rgb};

    #[test]
    fn mixing_nothing_leaves_the_colour_alone() {
        let a = Rgb::new(10, 20, 30);
        assert_eq!(a.mix(Rgb::new(200, 200, 200), 0), a);
    }

    #[test]
    fn mixing_everything_gives_the_other_colour() {
        let b = Rgb::new(200, 100, 50);
        assert_eq!(Rgb::new(10, 20, 30).mix(b, 100), b);
    }

    #[test]
    fn mixing_is_clamped_above_one_hundred_percent() {
        let b = Rgb::new(200, 100, 50);
        assert_eq!(Rgb::new(10, 20, 30).mix(b, 250), b);
    }

    #[test]
    fn the_sky_darkens_towards_the_top() {
        let p = Palette::alpymist();
        assert!(p.sky_at(0, 1080).luminance() < p.sky_at(1079, 1080).luminance());
    }

    #[test]
    fn a_zero_height_sky_does_not_divide_by_zero() {
        assert_eq!(
            Palette::alpymist().sky_at(0, 0),
            Palette::alpymist().sky_high
        );
    }

    #[test]
    fn distant_ridges_are_hazier_than_near_ones() {
        let p = Palette::alpymist();
        let near = p.ridge_at(0, 4).luminance();
        let far = p.ridge_at(4, 4).luminance();
        assert!(
            far > near,
            "distant ridge {far} should be lighter than near {near}"
        );
    }

    /// Body text on the darkest part of the scene has to stay readable on a
    /// washed-out ten-year-old TN panel, not just on a good monitor.
    #[test]
    fn text_has_usable_contrast_against_the_sky() {
        let p = Palette::alpymist();
        let gap = i32::from(p.ink.luminance()) - i32::from(p.sky_at(0, 1080).luminance());
        assert!(gap > 120, "ink/sky luminance gap only {gap}");
    }
}
