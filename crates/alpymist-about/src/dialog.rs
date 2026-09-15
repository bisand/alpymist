//! What every key and click does in the About box, with no pixels.

use crate::info::About;

/// Something in the box that takes a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Put the details on the clipboard.
    Copy,
    /// Close the box.
    Close,
}

impl Target {
    /// In Tab order.
    pub const ALL: [Self; 2] = [Self::Copy, Self::Close];
}

/// What the window has to do for the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing visible changed.
    Unchanged,
    /// Paint again.
    Redraw,
    /// Put this text on the clipboard, then paint again.
    Copy(String),
    /// Close the window.
    Close,
}

/// A key, as far as the box cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Tab.
    Tab,
    /// Shift+Tab.
    BackTab,
    /// Enter or Space: press what has focus.
    Activate,
    /// Ctrl+C: copy.
    Copy,
    /// Escape, or Ctrl+Q / Ctrl+W.
    Close,
}

/// The box's state.
#[derive(Debug, Clone)]
pub struct Dialog {
    about: About,
    hover: Option<Target>,
    focus: Option<Target>,
    copied: Option<Result<(), String>>,
}

impl Dialog {
    /// A box about `about`, with nothing hovered or focused.
    #[must_use]
    pub fn new(about: About) -> Self {
        Self {
            about,
            hover: None,
            focus: None,
            copied: None,
        }
    }

    /// What it is about.
    #[must_use]
    pub fn about(&self) -> &About {
        &self.about
    }

    /// What the pointer is over.
    #[must_use]
    pub fn hover(&self) -> Option<Target> {
        self.hover
    }

    /// What has keyboard focus.
    #[must_use]
    pub fn focus(&self) -> Option<Target> {
        self.focus
    }

    /// Whether the last copy worked, once there has been one.
    #[must_use]
    pub fn copied(&self) -> Option<&Result<(), String>> {
        self.copied.as_ref()
    }

    /// The pointer moved over `target`, or off everything.
    pub fn hover_over(&mut self, target: Option<Target>) -> Outcome {
        if self.hover == target {
            return Outcome::Unchanged;
        }
        self.hover = target;
        Outcome::Redraw
    }

    /// `target` was clicked.
    pub fn click(&mut self, target: Target) -> Outcome {
        match target {
            Target::Copy => Outcome::Copy(self.about.text()),
            Target::Close => Outcome::Close,
        }
    }

    /// A key was pressed.
    pub fn key(&mut self, key: Key) -> Outcome {
        let step = |from: Option<Target>, by: usize| {
            let n = Target::ALL.len();
            let at = from.and_then(|t| Target::ALL.iter().position(|&x| x == t));
            Some(Target::ALL[at.map_or(if by == 1 { 0 } else { n - 1 }, |i| (i + by) % n)])
        };
        match key {
            Key::Tab => {
                self.focus = step(self.focus, 1);
                Outcome::Redraw
            }
            Key::BackTab => {
                self.focus = step(self.focus, Target::ALL.len() - 1);
                Outcome::Redraw
            }
            Key::Activate => self.focus.map_or(Outcome::Close, |t| self.click(t)),
            Key::Copy => self.click(Target::Copy),
            Key::Close => Outcome::Close,
        }
    }

    /// The clipboard took the text, or did not.
    pub fn finished_copy(&mut self, result: Result<(), String>) -> Outcome {
        self.copied = Some(result);
        Outcome::Redraw
    }
}

#[cfg(test)]
mod tests {
    use super::{Dialog, Key, Outcome, Target};
    use crate::info::About;

    #[test]
    fn tab_goes_round_the_buttons_both_ways() {
        let mut d = Dialog::new(About::sample());
        d.key(Key::Tab);
        assert_eq!(d.focus(), Some(Target::Copy));
        d.key(Key::Tab);
        assert_eq!(d.focus(), Some(Target::Close));
        d.key(Key::Tab);
        assert_eq!(d.focus(), Some(Target::Copy));
        d.key(Key::BackTab);
        assert_eq!(d.focus(), Some(Target::Close));
    }

    #[test]
    fn copy_hands_over_the_whole_text() {
        let mut d = Dialog::new(About::sample());
        let Outcome::Copy(text) = d.key(Key::Copy) else {
            panic!("no copy");
        };
        assert!(text.starts_with("Alpymist 0.0.1, dev\n"));
        assert!(text.contains("alpymist-menu"));
        assert_eq!(d.finished_copy(Ok(())), Outcome::Redraw);
        assert_eq!(d.copied(), Some(&Ok(())));
    }

    #[test]
    fn enter_with_nothing_focused_closes() {
        let mut d = Dialog::new(About::sample());
        assert_eq!(d.key(Key::Activate), Outcome::Close);
    }
}
