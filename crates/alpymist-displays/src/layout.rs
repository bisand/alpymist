//! Layouts: how the screens are arranged, one for each set of screens.
//!
//! The set of screens connected is the key. The laptop alone has one layout,
//! the laptop on the desk's dock another, the laptop and a projector a third,
//! and each is put back as it was when that set is connected again. A set
//! never seen before is extended: the laptop's panel first, the rest to its
//! right in the order Hyprland found them, each as it came up. That is then
//! remembered, and changing it changes it for that set.
//!
//! Kept in `$XDG_CONFIG_HOME/alpymist/displays.toml`, which is the account's:
//! a file that does not parse is left alone and treated as empty, so nothing
//! the user wrote is overwritten because of a typo.

use crate::screen::{self, Monitor};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The account's layouts, under the configuration directory.
pub const FILE: &str = "alpymist/displays.toml";

/// One screen in a layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Output {
    /// Its name: see [`screen::names`].
    pub screen: String,
    /// Whether it shows anything.
    #[serde(default = "on")]
    pub enabled: bool,
    /// `1920x1080@60.00`, or `preferred` for what the screen asks for.
    #[serde(default = "preferred")]
    pub mode: String,
    /// Its top left corner in the layout, in logical pixels.
    #[serde(default)]
    pub position: [i32; 2],
    /// How much larger everything is drawn.
    #[serde(default = "one")]
    pub scale: f64,
    /// Rotation and flip, as `wl_output` numbers them: 0 to 7.
    #[serde(default)]
    pub transform: u8,
}

fn on() -> bool {
    true
}

fn preferred() -> String {
    "preferred".into()
}

fn one() -> f64 {
    1.0
}

impl Output {
    /// The screen as it is now.
    #[must_use]
    pub fn of(name: &str, m: &Monitor) -> Self {
        let lit = m.width > 0 && m.height > 0;
        Self {
            screen: name.to_owned(),
            enabled: !m.disabled,
            mode: if lit { m.mode() } else { preferred() },
            position: [m.x, m.y],
            scale: if m.scale > 0.0 { m.scale } else { 1.0 },
            transform: m.transform,
        }
    }

    /// The monitor rule that puts it there, for the screen matched by
    /// `target`.
    #[must_use]
    pub fn rule(&self, target: &str) -> String {
        if !self.enabled {
            return format!("{target}, disable");
        }
        let mut rule = format!(
            "{target}, {}, {}x{}, {}",
            self.mode,
            self.position[0],
            self.position[1],
            number(self.scale)
        );
        if self.transform != 0 {
            let _ = write!(rule, ", transform, {}", self.transform);
        }
        rule
    }
}

/// A scale as Hyprland reads it: `1`, `1.25`, `1.333333`.
fn number(value: f64) -> String {
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// How one set of screens is arranged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Layout {
    /// The screens, one each.
    #[serde(default, rename = "output")]
    pub outputs: Vec<Output>,
}

impl Layout {
    /// The set of screens it is for, sorted.
    #[must_use]
    pub fn key(&self) -> Vec<String> {
        key(self.outputs.iter().map(|o| o.screen.clone()))
    }

    /// Its screen called `name`.
    pub fn output_mut(&mut self, name: &str) -> Option<&mut Output> {
        self.outputs.iter_mut().find(|o| o.screen == name)
    }

    /// The layout for screens never seen together: each as it is, the
    /// laptop's panel at the left, the rest after it in a row along the top.
    #[must_use]
    pub fn extended(monitors: &[Monitor]) -> Self {
        let names = screen::names(monitors);
        let mut order: Vec<usize> = (0..monitors.len()).collect();
        // Stable: the panel first, the rest as Hyprland listed them.
        order.sort_by_key(|&i| !monitors[i].internal());
        let mut x = 0;
        let outputs = order
            .into_iter()
            .map(|i| {
                let m = &monitors[i];
                let mut output = Output::of(&names[i], m);
                output.enabled = true;
                output.position = [x, 0];
                x += m.laid_width().max(0);
                output
            })
            .collect();
        Self { outputs }
    }

    /// The layout as the screens are now.
    #[must_use]
    pub fn current(monitors: &[Monitor]) -> Self {
        let names = screen::names(monitors);
        Self {
            outputs: monitors
                .iter()
                .zip(&names)
                .map(|(m, name)| Output::of(name, m))
                .collect(),
        }
    }
}

/// A set of screens' names, as a key: sorted, so the order they were found
/// in does not matter.
pub fn key(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut key: Vec<String> = names.into_iter().collect();
    key.sort();
    key
}

/// Every layout the account has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layouts {
    /// Whether closing the lid turns the laptop's panel off while another
    /// screen is on (Settings › Displays).
    #[serde(default = "on", rename = "lid-turns-panel-off")]
    pub lid_off: bool,
    /// Whether each screen has its own workspaces 1 to 9 (Settings ›
    /// Displays), or one set is shared by them all.
    #[serde(default = "on", rename = "workspaces-per-screen")]
    pub per_screen: bool,
    /// Each screen's block of workspaces, by its name: see
    /// [`crate::workspaces`].
    #[serde(default, rename = "workspace-blocks")]
    pub blocks: std::collections::BTreeMap<String, u32>,
    /// One a set of screens.
    #[serde(default, rename = "layout")]
    pub layouts: Vec<Layout>,
}

impl Default for Layouts {
    fn default() -> Self {
        Self {
            lid_off: true,
            per_screen: true,
            blocks: std::collections::BTreeMap::new(),
            layouts: Vec::new(),
        }
    }
}

impl Layouts {
    /// The account's, or none.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Whether the file is there but not a list of layouts, and so must not
    /// be written over.
    #[must_use]
    pub fn unreadable(path: &Path) -> bool {
        std::fs::read_to_string(path).is_ok_and(|text| toml::from_str::<Self>(&text).is_err())
    }

    /// Keep them.
    ///
    /// # Errors
    /// The file could not be written.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if Self::unreadable(path) {
            return Err(format!(
                "{} is not a list of layouts; fix or remove it first",
                path.display()
            ));
        }
        let text = format!(
            "# Screen layouts, one for each set of screens, kept by Alpymist.\n\
             # `alpymist displays` changes them; a screen is named by its make,\n\
             # model and serial, and by its connector where those are not enough.\n\n{}",
            toml::to_string(self).map_err(|e| e.to_string())?
        );
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let temporary = path.with_extension("toml.new");
        std::fs::write(&temporary, text)
            .and_then(|()| std::fs::rename(&temporary, path))
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The layout for this set of screens.
    #[must_use]
    pub fn find(&self, key: &[String]) -> Option<&Layout> {
        self.layouts.iter().find(|l| l.key() == key)
    }

    /// Keep `layout` for its set of screens, replacing the one there was.
    pub fn put(&mut self, layout: Layout) {
        let key = layout.key();
        match self.layouts.iter_mut().find(|l| l.key() == key) {
            Some(old) => *old = layout,
            None => self.layouts.push(layout),
        }
    }

    /// Forget the layout for this set of screens. Whether there was one.
    pub fn forget(&mut self, key: &[String]) -> bool {
        let before = self.layouts.len();
        self.layouts.retain(|l| l.key() != key);
        self.layouts.len() != before
    }
}

/// Keep `layout` for its set of screens in the account's file.
///
/// # Errors
/// The file could not be written, or is not one of layouts.
pub fn keep(layout: Layout) -> Result<(), String> {
    let path = path();
    let mut all = Layouts::load(&path);
    all.put(layout);
    all.save(&path)
}

/// `$XDG_CONFIG_HOME/alpymist/displays.toml`.
#[must_use]
pub fn path() -> PathBuf {
    crate::base("XDG_CONFIG_HOME", ".config").join(FILE)
}

#[cfg(test)]
mod tests {
    use super::{Layout, Layouts, Output};
    use crate::screen::Monitor;

    pub(crate) fn screen(name: &str, description: &str, width: i32) -> Monitor {
        Monitor {
            name: name.into(),
            description: description.into(),
            width,
            height: 1080,
            refresh_rate: 60.0,
            scale: 1.0,
            ..Monitor::default()
        }
    }

    #[test]
    fn a_new_set_is_extended_to_the_right_of_the_panel() {
        let monitors = [
            screen("DP-3", "Samsung S24E650 H4ZK1", 1920),
            Monitor {
                scale: 1.5,
                ..screen("eDP-1", "Panel", 2880)
            },
            screen("DP-5", "Samsung S24C750 H4ZK2", 2560),
        ];
        let layout = Layout::extended(&monitors);
        let placed: Vec<(&str, [i32; 2])> = layout
            .outputs
            .iter()
            .map(|o| (o.screen.as_str(), o.position))
            .collect();
        assert_eq!(
            placed,
            [
                ("Panel", [0, 0]),
                ("Samsung S24E650 H4ZK1", [1920, 0]),
                ("Samsung S24C750 H4ZK2", [3840, 0]),
            ]
        );
        assert_eq!(layout.outputs[0].mode, "2880x1080@60.00");
        assert_eq!(
            layout.key(),
            ["Panel", "Samsung S24C750 H4ZK2", "Samsung S24E650 H4ZK1"]
        );
    }

    #[test]
    fn a_layout_becomes_hyprlands_monitor_rules() {
        let mut o = Output::of("Panel", &screen("eDP-1", "Panel", 1920));
        o.position = [1920, -200];
        o.scale = 1.25;
        assert_eq!(
            o.rule("desc:Panel"),
            "desc:Panel, 1920x1080@60.00, 1920x-200, 1.25"
        );
        o.transform = 1;
        o.scale = 1.0;
        assert_eq!(
            o.rule("eDP-1"),
            "eDP-1, 1920x1080@60.00, 1920x-200, 1, transform, 1"
        );
        o.enabled = false;
        assert_eq!(o.rule("eDP-1"), "eDP-1, disable");
    }

    #[test]
    fn layouts_are_kept_one_a_set_and_survive_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("alpymist-displays-{}", std::process::id()));
        let path = dir.join("displays.toml");
        let _ = std::fs::remove_dir_all(&dir);
        let alone = Layout::extended(&[screen("eDP-1", "Panel", 1920)]);
        let docked = Layout::extended(&[
            screen("eDP-1", "Panel", 1920),
            screen("DP-3", "Samsung", 1920),
        ]);
        let mut all = Layouts::default();
        all.put(alone.clone());
        all.put(docked.clone());
        let mut moved = docked.clone();
        moved.outputs[1].position = [-1920, 0];
        all.put(moved.clone());
        assert_eq!(all.layouts.len(), 2, "one a set");
        all.save(&path).unwrap();
        let back = Layouts::load(&path);
        assert_eq!(back.find(&docked.key()), Some(&moved));
        assert_eq!(back.find(&alone.key()), Some(&alone));

        std::fs::write(&path, "layout = oops").unwrap();
        assert!(Layouts::load(&path).layouts.is_empty());
        assert!(
            all.save(&path).is_err(),
            "a hand-broken file is not replaced"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
