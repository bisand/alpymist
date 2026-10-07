//! What the overview shows, and what keys and clicks do to it.

use alpymist_displays::screen::Monitor;
use alpymist_widget::Key;

/// Workspaces across, and down.
pub const SIDE: usize = 3;

/// A screen's workspaces: 1 to 9.
pub const SPACES: usize = SIDE * SIDE;

/// A window, as Hyprland lists it. Only what the overview draws.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
struct Client {
    mapped: bool,
    hidden: bool,
    at: (i32, i32),
    size: (i32, i32),
    workspace: Named,
    floating: bool,
    class: String,
    title: String,
    /// 0 for the window with the focus, 1 for the one before it.
    #[serde(rename = "focusHistoryID")]
    recent: i32,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            // A list that does not say is a list of windows that are there.
            mapped: true,
            hidden: false,
            at: (0, 0),
            size: (0, 0),
            workspace: Named::default(),
            floating: false,
            class: String::new(),
            title: String::new(),
            recent: i32::MAX,
        }
    }
}

/// The workspace a window is on.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
struct Named {
    id: i32,
}

/// A window on a workspace: where it is on the screen, in the screen's
/// logical pixels from its top left corner, and what to call it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub width: i32,
    /// Height.
    pub height: i32,
    /// The program's name: `Thunar`, `Foot`.
    pub name: String,
    /// The window's title.
    pub title: String,
    /// Whether it floats over the tiled ones.
    pub floating: bool,
    /// Whether it is the one the keyboard would go to.
    pub focused: bool,
}

/// One of the nine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Space {
    /// Hyprland's number for it.
    pub id: i32,
    /// Its windows, the ones underneath first.
    pub windows: Vec<Window>,
}

/// What a key or a click came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing to see.
    Nothing,
    /// Paint again.
    Redraw,
    /// Go to this workspace, by Hyprland's number, and close.
    Go(i32),
    /// Close, and stay where we were.
    Close,
}

/// The overview of one screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overview {
    /// The screen's connector: `eDP-1`.
    pub screen: String,
    /// The screen's size in logical pixels, which the windows' places are in.
    pub size: (i32, i32),
    /// The nine, in order.
    pub spaces: Vec<Space>,
    /// The one the screen shows now, when it is one of the nine.
    pub current: Option<usize>,
    /// The one Enter goes to.
    pub chosen: usize,
    /// The one under the pointer.
    pub hovered: Option<usize>,
}

/// A screen's size in the layout's logical pixels: its mode over its scale,
/// and turned when the screen is.
#[must_use]
pub fn logical_size(m: &Monitor) -> (i32, i32) {
    let scale = if m.scale > 0.0 { m.scale } else { 1.0 };
    // A screen is far smaller than an i32 holds.
    #[allow(clippy::cast_possible_truncation)]
    let px = |v: i32| (f64::from(v) / scale).round() as i32;
    let (w, h) = (px(m.width).max(1), px(m.height).max(1));
    if m.transform % 2 == 1 { (h, w) } else { (w, h) }
}

/// What to call a window: the last part of its class, capitalised
/// (`org.gnome.Nautilus` is Nautilus, `foot` is Foot), or its title when it
/// has no class.
#[must_use]
pub fn name(class: &str, title: &str) -> String {
    let last = class.rsplit('.').next().unwrap_or(class).trim();
    if last.is_empty() {
        return title.trim().to_owned();
    }
    let mut chars = last.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

impl Overview {
    /// The overview of the screen with the focus, whose first workspace is
    /// `first` as Hyprland numbers it, from Hyprland's list of windows.
    ///
    /// # Errors
    /// No screen has the focus, or the list is not one.
    pub fn new(monitors: &[Monitor], first: i32, clients: &[u8]) -> Result<Self, String> {
        let screen = monitors
            .iter()
            .find(|m| m.focused && !m.disabled)
            .or_else(|| monitors.iter().find(|m| !m.disabled))
            .ok_or("no screen is on")?;
        let clients: Vec<Client> = serde_json::from_slice(clients)
            .map_err(|e| format!("Hyprland's list of windows: {e}"))?;
        let size = logical_size(screen);
        let spaces: Vec<Space> = (0..SPACES)
            .map(|i| {
                let id = first + i32::try_from(i).unwrap_or(0);
                let mut on: Vec<&Client> = clients
                    .iter()
                    .filter(|c| c.workspace.id == id && c.mapped && !c.hidden)
                    .filter(|c| c.size.0 > 0 && c.size.1 > 0)
                    .collect();
                // Tiled under floating, and of each the longest left alone
                // underneath: what is on top on the screen is on top here.
                on.sort_by_key(|c| (c.floating, std::cmp::Reverse(c.recent)));
                let windows = on
                    .into_iter()
                    .map(|c| Window {
                        x: c.at.0 - screen.x,
                        y: c.at.1 - screen.y,
                        width: c.size.0,
                        height: c.size.1,
                        name: name(&c.class, &c.title),
                        title: c.title.trim().to_owned(),
                        floating: c.floating,
                        focused: c.recent == 0,
                    })
                    .collect();
                Space { id, windows }
            })
            .collect();
        let current = spaces
            .iter()
            .position(|s| s.id == screen.active_workspace.id);
        Ok(Self {
            screen: screen.name.clone(),
            size,
            spaces,
            current,
            chosen: current.unwrap_or(0),
            hovered: None,
        })
    }

    /// Move the choice by a column and a row, stopping at the edges.
    fn step(&mut self, dx: isize, dy: isize) -> Step {
        let (col, row) = (self.chosen % SIDE, self.chosen / SIDE);
        let moved = |at: usize, by: isize| at.saturating_add_signed(by).min(SIDE - 1);
        let next = moved(row, dy) * SIDE + moved(col, dx);
        self.choose(next)
    }

    fn choose(&mut self, next: usize) -> Step {
        if next == self.chosen || next >= self.spaces.len() {
            return Step::Nothing;
        }
        self.chosen = next;
        Step::Redraw
    }

    /// Go to the workspace at `index`.
    fn go(&self, index: usize) -> Step {
        self.spaces
            .get(index)
            .map_or(Step::Nothing, |s| Step::Go(s.id))
    }

    /// A key.
    pub fn key(&mut self, key: Key) -> Step {
        match key {
            Key::Escape => Step::Close,
            Key::Enter | Key::Space => self.go(self.chosen),
            Key::Left => self.step(-1, 0),
            Key::Right => self.step(1, 0),
            Key::Up => self.step(0, -1),
            Key::Down => self.step(0, 1),
            Key::Home => self.choose(0),
            Key::End => self.choose(SPACES - 1),
            Key::Tab => self.choose((self.chosen + 1) % SPACES),
            Key::BackTab => self.choose((self.chosen + SPACES - 1) % SPACES),
            Key::Backspace | Key::Clear => Step::Nothing,
        }
    }

    /// A character typed: 1 to 9 go there, as Super and the number would.
    #[must_use]
    pub fn typed(&self, ch: char) -> Step {
        match ch.to_digit(10) {
            Some(n @ 1..=9) => self.go(n as usize - 1),
            _ => Step::Nothing,
        }
    }

    /// The pointer is over the workspace at `index`, or over none.
    pub fn hover(&mut self, index: Option<usize>) -> Step {
        if index == self.hovered {
            return Step::Nothing;
        }
        self.hovered = index;
        Step::Redraw
    }

    /// A press on the workspace at `index`, or beside them all, which closes.
    #[must_use]
    pub fn press(&self, index: Option<usize>) -> Step {
        index.map_or(Step::Close, |i| self.go(i))
    }

    /// A screen with windows on it, to draw without a desktop.
    #[must_use]
    pub fn sample() -> Self {
        let window = |x, y, width, height, name: &str, title: &str| Window {
            x,
            y,
            width,
            height,
            name: name.into(),
            title: title.into(),
            floating: false,
            focused: false,
        };
        let mut spaces: Vec<Space> = (1..=9)
            .map(|id| Space {
                id,
                windows: Vec::new(),
            })
            .collect();
        spaces[0].windows = vec![
            window(10, 36, 625, 754, "Foot", "~/dev/alpymist"),
            window(645, 36, 625, 372, "Thunar", "bisand - Thunar"),
            window(645, 418, 625, 372, "Squint", "hyprland.conf"),
        ];
        spaces[0].windows[1].focused = true;
        spaces[1].windows = vec![window(10, 36, 1260, 754, "Librewolf", "Alpine Linux")];
        spaces[3].windows = vec![
            window(10, 36, 1260, 754, "Code", "main.rs - alpymist"),
            Window {
                floating: true,
                ..window(440, 250, 400, 300, "Pavucontrol", "Volume Control")
            },
        ];
        Self {
            screen: "eDP-1".into(),
            size: (1280, 800),
            spaces,
            current: Some(0),
            chosen: 0,
            hovered: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Overview, Step, logical_size, name};
    use alpymist_displays::screen::Monitor;
    use alpymist_widget::Key;

    fn screens() -> Vec<Monitor> {
        alpymist_displays::screen::parse(
            br#"[
              {"id":0,"name":"eDP-1","width":2560,"height":1600,"x":0,"y":0,"scale":2.0,
               "transform":0,"focused":false,"activeWorkspace":{"id":2,"name":"2"}},
              {"id":1,"name":"DP-3","width":1920,"height":1080,"x":1280,"y":0,"scale":1.0,
               "transform":0,"focused":true,"activeWorkspace":{"id":12,"name":"12"}}
            ]"#,
        )
        .unwrap()
    }

    const CLIENTS: &[u8] = br#"[
      {"mapped":true,"hidden":false,"at":[1290,36],"size":[940,1034],
       "workspace":{"id":12,"name":"12"},"floating":false,"class":"foot","title":"zsh",
       "focusHistoryID":1},
      {"mapped":true,"hidden":false,"at":[2240,36],"size":[950,1034],
       "workspace":{"id":12,"name":"12"},"floating":false,"class":"org.gnome.Nautilus",
       "title":"Home","focusHistoryID":0},
      {"mapped":true,"hidden":false,"at":[1700,300],"size":[400,300],
       "workspace":{"id":12,"name":"12"},"floating":true,"class":"pavucontrol",
       "title":"Volume Control","focusHistoryID":2},
      {"mapped":true,"hidden":false,"at":[10,36],"size":[1260,754],
       "workspace":{"id":2,"name":"2"},"floating":false,"class":"librewolf","title":"x",
       "focusHistoryID":3},
      {"mapped":false,"hidden":false,"at":[1290,36],"size":[100,100],
       "workspace":{"id":14,"name":"14"},"floating":false,"class":"ghost","title":"",
       "focusHistoryID":4},
      {"mapped":true,"hidden":true,"at":[1290,36],"size":[100,100],
       "workspace":{"id":14,"name":"14"},"floating":false,"class":"grouped","title":"",
       "focusHistoryID":5},
      {"at":[1290,36],"size":[700,500],"workspace":{"id":19,"name":"19"},
       "class":"","title":" A title "}
    ]"#;

    #[test]
    fn the_focused_screens_nine_with_windows_where_they_are() {
        let o = Overview::new(&screens(), 11, CLIENTS).unwrap();
        assert_eq!(o.screen, "DP-3");
        assert_eq!(o.size, (1920, 1080));
        assert_eq!(
            o.spaces.iter().map(|s| s.id).collect::<Vec<_>>(),
            (11..=19).collect::<Vec<_>>()
        );
        // Workspace 12 is the one on screen, and the choice starts there.
        assert_eq!((o.current, o.chosen), (Some(1), 1));
        let on = &o.spaces[1].windows;
        // The screen starts at 1280 in the layout: places are from its corner.
        assert_eq!((on[0].name.as_str(), on[0].x, on[0].y), ("Foot", 10, 36));
        assert_eq!((on[1].name.as_str(), on[1].x), ("Nautilus", 960));
        assert!(on[1].focused && !on[0].focused);
        // Floating last, so it is drawn over the tiled ones.
        assert_eq!((on[2].name.as_str(), on[2].floating), ("Pavucontrol", true));
        // Another screen's window is not this screen's.
        assert!(
            o.spaces
                .iter()
                .all(|s| s.windows.iter().all(|w| w.name != "Librewolf"))
        );
        // Not mapped, and hidden behind its group: neither is to be seen.
        assert!(o.spaces[3].windows.is_empty());
        // What Hyprland left out is taken as a window that is there.
        assert_eq!(o.spaces[8].windows[0].name, "A title");
    }

    #[test]
    fn a_screens_size_is_its_mode_over_its_scale_and_turned_with_it() {
        let mut m = screens().remove(0);
        assert_eq!(logical_size(&m), (1280, 800));
        m.transform = 1;
        assert_eq!(logical_size(&m), (800, 1280));
        m.scale = 0.0;
        assert_eq!(logical_size(&m), (1600, 2560), "no scale is a scale of one");
    }

    #[test]
    fn a_window_is_called_by_the_end_of_its_class() {
        assert_eq!(name("foot", "zsh"), "Foot");
        assert_eq!(name("com.visualstudio.code", "x"), "Code");
        assert_eq!(name("org.gnome.Nautilus", "x"), "Nautilus");
        assert_eq!(name("", " Untitled "), "Untitled");
        assert_eq!(name("", ""), "");
    }

    #[test]
    fn arrows_stop_at_the_edges_and_tab_goes_round() {
        let mut o = Overview::sample();
        assert_eq!(o.key(Key::Left), Step::Nothing);
        assert_eq!(o.key(Key::Up), Step::Nothing);
        assert_eq!(o.key(Key::Right), Step::Redraw);
        assert_eq!(o.key(Key::Down), Step::Redraw);
        assert_eq!(o.chosen, 4);
        assert_eq!(o.key(Key::End), Step::Redraw);
        assert_eq!(o.key(Key::Right), Step::Nothing);
        assert_eq!(o.key(Key::Down), Step::Nothing);
        assert_eq!(o.key(Key::Tab), Step::Redraw);
        assert_eq!(o.chosen, 0);
        assert_eq!(o.key(Key::BackTab), Step::Redraw);
        assert_eq!(o.chosen, 8);
    }

    #[test]
    fn enter_a_number_and_a_click_go_and_escape_and_a_click_beside_close() {
        let mut o = Overview::new(&screens(), 11, CLIENTS).unwrap();
        assert_eq!(o.key(Key::Enter), Step::Go(12));
        assert_eq!(o.typed('7'), Step::Go(17));
        assert_eq!(o.typed('0'), Step::Nothing);
        assert_eq!(o.typed('x'), Step::Nothing);
        assert_eq!(o.press(Some(4)), Step::Go(15));
        assert_eq!(o.press(None), Step::Close);
        assert_eq!(o.key(Key::Escape), Step::Close);
        assert_eq!(o.hover(Some(2)), Step::Redraw);
        assert_eq!(o.hover(Some(2)), Step::Nothing);
    }

    #[test]
    fn no_screen_on_is_said() {
        assert!(Overview::new(&[], 1, b"[]").is_err());
        assert!(Overview::new(&screens(), 1, b"not json").is_err());
    }
}
