//! Keyboard focus over a widget's controls.
//!
//! Everything a click can do, a key should do. Tab and Shift+Tab walk a ring
//! of the controls present, in the order they are drawn, and Enter or Space
//! presses the one with the focus. The ring is only drawn once the keyboard
//! has moved it, and hides again at a click, so a pointer user never sees a
//! ring they did not ask for.

/// Which of a widget's controls has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusRing<F> {
    focus: F,
    visible: bool,
}

impl<F: Copy + Eq> FocusRing<F> {
    /// Focus on `first`, with no ring drawn.
    #[must_use]
    pub fn new(first: F) -> Self {
        Self {
            focus: first,
            visible: false,
        }
    }

    /// The control with the focus.
    #[must_use]
    pub fn get(&self) -> F {
        self.focus
    }

    /// Whether to draw the ring.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Whether `focus` has the focus and the ring shows.
    #[must_use]
    pub fn shows(&self, focus: F) -> bool {
        self.visible && self.focus == focus
    }

    /// Move the focus by keyboard: the ring shows.
    pub fn set(&mut self, focus: F) {
        self.focus = focus;
        self.visible = true;
    }

    /// Move the focus by pointer: the ring hides.
    pub fn click(&mut self, focus: F) {
        self.focus = focus;
        self.visible = false;
    }

    /// Tab (`by` = 1) or Shift+Tab (`by` = -1) through `order`. Returns
    /// whether anything changed.
    pub fn step(&mut self, order: &[F], by: isize) -> bool {
        if order.is_empty() {
            return false;
        }
        let len = order.len().cast_signed();
        let next = match order.iter().position(|f| *f == self.focus) {
            // A hidden ring appears where it is, rather than one stop on.
            Some(i) if !self.visible => i.cast_signed(),
            Some(i) => (i.cast_signed() + by).rem_euclid(len),
            None if by > 0 => 0,
            None => len - 1,
        };
        let before = *self;
        self.set(order[next.cast_unsigned()]);
        *self != before
    }

    /// Keep the focus on something in `order`, moving it to the first there
    /// is when its control has gone.
    pub fn settle(&mut self, order: &[F]) {
        if !order.contains(&self.focus)
            && let Some(first) = order.first()
        {
            self.focus = *first;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FocusRing;

    #[test]
    fn the_first_tab_shows_the_ring_where_it_is() {
        let mut f = FocusRing::new(2);
        assert!(!f.visible());
        assert!(f.step(&[1, 2, 3], 1));
        assert_eq!(f.get(), 2);
        f.step(&[1, 2, 3], 1);
        assert_eq!(f.get(), 3);
        f.step(&[1, 2, 3], 1);
        assert_eq!(f.get(), 1, "wraps");
        f.step(&[1, 2, 3], -1);
        assert_eq!(f.get(), 3);
    }

    #[test]
    fn a_click_hides_the_ring_and_a_vanished_control_lets_go() {
        let mut f = FocusRing::new(1);
        f.set(3);
        f.click(2);
        assert!(!f.visible());
        f.settle(&[1, 3]);
        assert_eq!(f.get(), 1);
    }
}
