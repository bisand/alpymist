//! The menu's state, and what every key does to it.
//!
//! No pixels here. The host turns keystrokes and clicks into calls on
//! [`Menu`], acts on the [`Outcome`], and asks the view to paint whatever the
//! menu now holds — so everything a user can do is testable as a sequence of
//! calls.
//!
//! Searching is scoped to where you are. At the top, typing searches the
//! whole tree — `shut` finds *System › Shut down* without opening System
//! first — and inside a submenu it searches that submenu and what is below it.
//! Going back restores the query and selection the level had, so stepping
//! into a submenu to look and stepping out again loses nothing.

use crate::fuzzy;
use crate::history::History;
use crate::tree::{Action, EntryId, MenuId, Reachable, Tree};

/// What a keypress means to the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Previous entry, wrapping.
    Up,
    /// Next entry, wrapping.
    Down,
    /// A page up.
    PageUp,
    /// A page down.
    PageDown,
    /// First entry.
    Home,
    /// Last entry.
    End,
    /// Choose the selected entry.
    Enter,
    /// Open the selected entry if it is a submenu.
    Right,
    /// Back a level.
    Left,
    /// Back a level, or close at the top.
    Escape,
    /// Delete a character, or go back when there is nothing to delete.
    Backspace,
    /// Delete the last word of the query.
    DeleteWord,
    /// Clear the query.
    ClearQuery,
}

/// What the host should do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing visible changed.
    Unchanged,
    /// Paint again.
    Redraw,
    /// Run this entry, then close.
    Run(EntryId),
    /// Close without running anything.
    Close,
}

/// A row as currently listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The entry.
    pub entry: EntryId,
    /// Character indices of the name's letters the query matched.
    pub positions: Vec<usize>,
    /// Where it lives, when it is not in the open menu itself.
    pub trail: String,
}

/// A level left behind by opening a submenu.
#[derive(Debug, Clone)]
struct Level {
    menu: MenuId,
    query: String,
    selected: usize,
    scroll: usize,
}

/// The menu.
#[derive(Debug)]
pub struct Menu {
    /// Everything in it.
    pub tree: Tree,
    history: History,
    /// The open menu.
    open: MenuId,
    /// Levels to go back to, outermost first.
    stack: Vec<Level>,
    /// Everything searchable from the open menu, computed once per level.
    scope: Vec<Reachable>,
    query: String,
    rows: Vec<Row>,
    selected: usize,
    scroll: usize,
    visible: usize,
}

/// A small head start for entries used before. Enough to decide between
/// close matches, never enough to lift a poor match over a good one.
fn history_bonus(count: u32) -> i32 {
    // 1 use: 6, 3: 12, 7: 18, 15+: 24.
    let steps = 32 - count.saturating_add(1).leading_zeros();
    i32::try_from(steps.saturating_sub(1).min(4) * 6).unwrap_or(0)
}

/// How much less a match on a keyword counts than a match on the name.
const KEYWORD_PENALTY: i32 = 24;
/// How much less an entry counts for each submenu it is below the open one.
const DEPTH_PENALTY: i32 = 6;

impl Menu {
    /// A menu open at `start`, showing `visible` rows at a time.
    #[must_use]
    pub fn new(tree: Tree, history: History, start: MenuId, visible: usize) -> Self {
        let mut menu = Self {
            tree,
            history,
            open: start,
            stack: Vec::new(),
            scope: Vec::new(),
            query: String::new(),
            rows: Vec::new(),
            selected: 0,
            scroll: 0,
            visible: visible.max(1),
        };
        menu.enter_scope();
        menu.refilter();
        menu
    }

    /// The query as typed.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Rows matching the query, best first.
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Index of the selected row.
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Index of the first row on screen.
    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// How many rows fit on screen.
    #[must_use]
    pub fn visible(&self) -> usize {
        self.visible
    }

    /// Titles from the outermost level to the open menu.
    #[must_use]
    pub fn breadcrumbs(&self) -> Vec<&str> {
        self.stack
            .iter()
            .map(|l| l.menu)
            .chain(std::iter::once(self.open))
            .map(|m| self.tree.menus[m].title.as_str())
            .collect()
    }

    /// How many entries a search from the open menu looks through.
    #[must_use]
    pub fn searchable(&self) -> usize {
        self.scope.len()
    }

    /// Launch history, for the host to save after a launch.
    #[must_use]
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Note that `entry` was launched.
    pub fn record_launch(&mut self, entry: EntryId) {
        let key = self.tree.entries[entry].key.clone();
        self.history.record(&key);
    }

    fn enter_scope(&mut self) {
        self.scope = self.tree.reachable(self.open);
    }

    /// Recompute the rows for the current query, selecting the best.
    fn refilter(&mut self) {
        let tree = &self.tree;
        let history = &self.history;
        if self.query.trim().is_empty() {
            let node = &tree.menus[self.open];
            let mut ids: Vec<EntryId> = node.entries.clone();
            if node.by_use {
                // Stable, so equally used entries keep their alphabetical order.
                ids.sort_by_key(|&e| std::cmp::Reverse(history.count(&tree.entries[e].key)));
            }
            self.rows = ids
                .into_iter()
                .map(|entry| Row {
                    entry,
                    positions: Vec::new(),
                    trail: String::new(),
                })
                .collect();
        } else {
            let query = self.query.as_str();
            let mut scored: Vec<(i32, usize, Row)> = self
                .scope
                .iter()
                .filter_map(|r| {
                    let entry = &tree.entries[r.entry];
                    let on_name = fuzzy::score(query, &entry.name);
                    let on_keyword = entry
                        .keywords
                        .iter()
                        .filter_map(|k| fuzzy::score(query, k))
                        .map(|m| m.score - KEYWORD_PENALTY)
                        .max();
                    let (score, positions) = match (on_name, on_keyword) {
                        (Some(m), Some(k)) if k > m.score => (k, Vec::new()),
                        (Some(m), _) => (m.score, m.positions),
                        (None, Some(k)) => (k, Vec::new()),
                        (None, None) => return None,
                    };
                    let depth = i32::try_from(r.depth).unwrap_or(i32::MAX / DEPTH_PENALTY);
                    let score =
                        score + history_bonus(history.count(&entry.key)) - depth * DEPTH_PENALTY;
                    let row = Row {
                        entry: r.entry,
                        positions,
                        trail: r.trail.clone(),
                    };
                    Some((score, entry.name.chars().count(), row))
                })
                .collect();
            // Best score, then the shorter name — `Foot` over `Foot server`
            // for `foot` — then the order the tree lists them in.
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            self.rows = scored.into_iter().map(|(_, _, row)| row).collect();
        }
        self.selected = 0;
        self.scroll = 0;
    }

    fn select(&mut self, index: usize) -> Outcome {
        if self.rows.is_empty() {
            return Outcome::Unchanged;
        }
        let index = index.min(self.rows.len() - 1);
        let before = (self.selected, self.scroll);
        self.selected = index;
        if index < self.scroll {
            self.scroll = index;
        } else if index >= self.scroll + self.visible {
            self.scroll = index + 1 - self.visible;
        }
        if before == (self.selected, self.scroll) {
            Outcome::Unchanged
        } else {
            Outcome::Redraw
        }
    }

    fn open(&mut self, sub: MenuId) -> Outcome {
        self.stack.push(Level {
            menu: self.open,
            query: std::mem::take(&mut self.query),
            selected: self.selected,
            scroll: self.scroll,
        });
        self.open = sub;
        self.enter_scope();
        self.refilter();
        Outcome::Redraw
    }

    fn back(&mut self) -> Outcome {
        let Some(level) = self.stack.pop() else {
            return Outcome::Unchanged;
        };
        self.open = level.menu;
        self.query = level.query;
        self.enter_scope();
        self.refilter();
        self.selected = level.selected.min(self.rows.len().saturating_sub(1));
        self.scroll = level.scroll;
        self.select(self.selected);
        Outcome::Redraw
    }

    fn activate(&mut self, index: usize) -> Outcome {
        let Some(row) = self.rows.get(index) else {
            return Outcome::Unchanged;
        };
        match &self.tree.entries[row.entry].action {
            Action::Open(sub) => self.open(*sub),
            Action::Run(_) => Outcome::Run(row.entry),
        }
    }

    /// A key was pressed.
    pub fn key(&mut self, key: Key) -> Outcome {
        let n = self.rows.len();
        let page = self.visible.saturating_sub(1).max(1);
        match key {
            Key::Up if n > 0 => self.select(if self.selected == 0 {
                n - 1
            } else {
                self.selected - 1
            }),
            Key::Down if n > 0 => self.select(if self.selected + 1 >= n {
                0
            } else {
                self.selected + 1
            }),
            Key::PageUp => self.select(self.selected.saturating_sub(page)),
            Key::PageDown => self.select(self.selected + page),
            Key::Home => self.select(0),
            Key::End => self.select(n.saturating_sub(1)),
            Key::Enter => self.activate(self.selected),
            Key::Right => match self
                .rows
                .get(self.selected)
                .map(|r| &self.tree.entries[r.entry].action)
            {
                Some(Action::Open(sub)) => self.open(*sub),
                _ => Outcome::Unchanged,
            },
            Key::Left => self.back(),
            Key::Escape => {
                if self.stack.is_empty() {
                    Outcome::Close
                } else {
                    self.back()
                }
            }
            Key::Backspace => {
                if self.query.pop().is_some() {
                    self.refilter();
                    Outcome::Redraw
                } else {
                    self.back()
                }
            }
            Key::DeleteWord => {
                if self.query.is_empty() {
                    return Outcome::Unchanged;
                }
                let kept = self.query.trim_end().rfind(' ').map_or(0, |i| i + 1);
                self.query.truncate(kept);
                self.refilter();
                Outcome::Redraw
            }
            Key::ClearQuery => {
                if self.query.is_empty() {
                    return Outcome::Unchanged;
                }
                self.query.clear();
                self.refilter();
                Outcome::Redraw
            }
            Key::Up | Key::Down => Outcome::Unchanged,
        }
    }

    /// Text was typed.
    pub fn text(&mut self, ch: char) -> Outcome {
        if ch.is_control() {
            return Outcome::Unchanged;
        }
        self.query.push(ch);
        self.refilter();
        Outcome::Redraw
    }

    /// The pointer is over the row `offset` places below the first visible one.
    pub fn hover(&mut self, offset: usize) -> Outcome {
        let index = self.scroll + offset;
        if index < self.rows.len() {
            self.select(index)
        } else {
            Outcome::Unchanged
        }
    }

    /// The row `offset` places below the first visible one was clicked.
    pub fn click(&mut self, offset: usize) -> Outcome {
        let index = self.scroll + offset;
        if index < self.rows.len() {
            self.selected = index;
            self.activate(index)
        } else {
            Outcome::Unchanged
        }
    }

    /// Scroll by whole rows, positive downwards. The selection comes along so
    /// that it stays on screen.
    pub fn scroll_by(&mut self, rows: i32) -> Outcome {
        let n = self.rows.len();
        if n <= self.visible {
            return Outcome::Unchanged;
        }
        let max = n - self.visible;
        let step = usize::try_from(rows.unsigned_abs()).unwrap_or(usize::MAX);
        let before = self.scroll;
        self.scroll = if rows < 0 {
            self.scroll.saturating_sub(step)
        } else {
            (self.scroll + step).min(max)
        };
        self.selected = self
            .selected
            .clamp(self.scroll, self.scroll + self.visible - 1);
        if before == self.scroll {
            Outcome::Unchanged
        } else {
            Outcome::Redraw
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, Menu, Outcome, history_bonus};
    use crate::apps::App;
    use crate::config::Config;
    use crate::exec::Launch;
    use crate::history::History;
    use crate::tree::{Action, Tree};

    const CONFIG: &str = r#"
        [menu.root]
        title = "Alpymist"
        items = [
          { name = "Apps", menu = "apps" },
          { name = "Capture", menu = "capture" },
          { name = "System", menu = "system", keywords = ["power"] },
        ]
        [menu.capture]
        title = "Capture"
        items = [
          { name = "Region", exec = "slurp" },
          { name = "Screen", exec = "grim" },
        ]
        [menu.system]
        title = "System"
        items = [
          { name = "Lock", exec = "alpymist-lock" },
          { name = "Restart", exec = "reboot", keywords = ["reboot"] },
          { name = "Shut down", exec = "poweroff", keywords = ["poweroff", "halt"] },
        ]
    "#;

    fn app(name: &str) -> App {
        App {
            id: format!("{}.desktop", name.to_lowercase()),
            name: name.into(),
            detail: None,
            keywords: vec![],
            icon: "",
            launch: Launch::Argv {
                argv: vec![name.to_lowercase()],
                terminal: false,
            },
        }
    }

    fn menu_with(history: History, visible: usize) -> Menu {
        let config = Config::parse(CONFIG).unwrap();
        let apps = ["Btop", "Foot", "Foot server", "LibreWolf", "Pavucontrol"]
            .into_iter()
            .map(app)
            .collect();
        let tree = Tree::build(&config, apps);
        let root = tree.find("root").unwrap();
        Menu::new(tree, history, root, visible)
    }

    fn menu() -> Menu {
        menu_with(History::default(), 4)
    }

    fn names(m: &Menu) -> Vec<&str> {
        m.rows()
            .iter()
            .map(|r| m.tree.entries[r.entry].name.as_str())
            .collect()
    }

    fn selected_name(m: &Menu) -> &str {
        &m.tree.entries[m.rows()[m.selected()].entry].name
    }

    fn typed(m: &mut Menu, text: &str) {
        for ch in text.chars() {
            m.text(ch);
        }
    }

    #[test]
    fn it_opens_on_the_menu_as_written() {
        let m = menu();
        assert_eq!(names(&m), ["Apps", "Capture", "System"]);
        assert_eq!(m.breadcrumbs(), ["Alpymist"]);
    }

    #[test]
    fn typing_at_the_top_searches_the_whole_tree() {
        let mut m = menu();
        typed(&mut m, "shut");
        assert_eq!(names(&m)[0], "Shut down");
        assert_eq!(m.rows()[0].trail, "System");
    }

    #[test]
    fn keywords_find_entries_their_names_do_not() {
        let mut m = menu();
        typed(&mut m, "poweroff");
        assert_eq!(names(&m)[0], "Shut down");
        assert!(
            m.rows()[0].positions.is_empty(),
            "nothing in the name to highlight"
        );
    }

    #[test]
    fn a_name_match_beats_a_keyword_match() {
        let mut m = menu();
        typed(&mut m, "re");
        // Restart and Region match on the name; Restart's `reboot` keyword
        // must not lift anything above them.
        let first_two: Vec<_> = names(&m).into_iter().take(2).collect();
        assert!(
            first_two.contains(&"Restart") && first_two.contains(&"Region"),
            "{first_two:?}"
        );
    }

    #[test]
    fn a_shorter_name_wins_an_equal_match() {
        let mut m = menu();
        typed(&mut m, "foot");
        assert_eq!(&names(&m)[..2], ["Foot", "Foot server"]);
    }

    #[test]
    fn enter_on_a_submenu_opens_it_and_escape_comes_back() {
        let mut m = menu();
        m.key(Key::Down);
        m.key(Key::Down);
        assert_eq!(m.key(Key::Enter), Outcome::Redraw);
        assert_eq!(names(&m), ["Lock", "Restart", "Shut down"]);
        assert_eq!(m.breadcrumbs(), ["Alpymist", "System"]);
        assert_eq!(m.key(Key::Escape), Outcome::Redraw);
        assert_eq!(selected_name(&m), "System", "selection is restored");
        assert_eq!(m.key(Key::Escape), Outcome::Close);
    }

    #[test]
    fn going_back_restores_the_query() {
        let mut m = menu();
        typed(&mut m, "cap");
        assert_eq!(selected_name(&m), "Capture");
        m.key(Key::Enter);
        assert_eq!(m.query(), "");
        m.key(Key::Backspace);
        assert_eq!(m.query(), "cap");
        assert_eq!(selected_name(&m), "Capture");
    }

    #[test]
    fn a_submenu_search_stays_inside_it() {
        let mut m = menu();
        m.key(Key::End);
        m.key(Key::Right);
        typed(&mut m, "r");
        assert!(!names(&m).contains(&"Region"), "{:?}", names(&m));
        assert!(names(&m).contains(&"Restart"));
    }

    #[test]
    fn enter_on_a_command_asks_for_it_to_run() {
        let mut m = menu();
        typed(&mut m, "lock");
        let Outcome::Run(entry) = m.key(Key::Enter) else {
            panic!("expected Run");
        };
        assert!(matches!(m.tree.entries[entry].action, Action::Run(_)));
        assert_eq!(m.tree.entries[entry].name, "Lock");
    }

    #[test]
    fn nothing_matching_leaves_nothing_to_run() {
        let mut m = menu();
        typed(&mut m, "zzzz");
        assert!(m.rows().is_empty());
        assert_eq!(m.key(Key::Enter), Outcome::Unchanged);
        assert_eq!(m.key(Key::Down), Outcome::Unchanged);
    }

    #[test]
    fn selection_wraps_and_scrolls_into_view() {
        let mut m = menu();
        m.key(Key::Enter); // Apps: five entries, four visible
        m.key(Key::Up);
        assert_eq!(m.selected(), 4);
        assert_eq!(m.scroll(), 1);
        m.key(Key::Down);
        assert_eq!((m.selected(), m.scroll()), (0, 0));
    }

    #[test]
    fn the_apps_list_puts_the_most_used_first() {
        let mut history = History::default();
        for _ in 0..3 {
            history.record("app:pavucontrol.desktop");
        }
        history.record("app:foot.desktop");
        let mut m = menu_with(history, 4);
        m.key(Key::Enter);
        assert_eq!(
            names(&m),
            ["Pavucontrol", "Foot", "Btop", "Foot server", "LibreWolf"]
        );
    }

    #[test]
    fn history_breaks_ties_in_a_search() {
        let mut history = History::default();
        for _ in 0..5 {
            history.record("app:foot server.desktop");
        }
        let mut m = menu_with(history, 4);
        typed(&mut m, "foot");
        assert_eq!(names(&m)[0], "Foot server");
    }

    #[test]
    fn history_never_lifts_a_poor_match_over_a_good_one() {
        assert!(history_bonus(u32::MAX) < 32);
        assert_eq!(history_bonus(0), 0);
        assert!(history_bonus(1) > 0);
    }

    #[test]
    fn deleting_words_and_clearing() {
        let mut m = menu();
        typed(&mut m, "shut down");
        m.key(Key::DeleteWord);
        assert_eq!(m.query(), "shut ");
        m.key(Key::ClearQuery);
        assert_eq!(m.query(), "");
        assert_eq!(names(&m), ["Apps", "Capture", "System"]);
    }

    #[test]
    fn hovering_and_clicking_are_relative_to_the_scroll() {
        let mut m = menu();
        m.key(Key::Enter);
        m.key(Key::End);
        assert_eq!(m.scroll(), 1);
        m.hover(0);
        assert_eq!(selected_name(&m), "Foot");
        assert_eq!(m.hover(9), Outcome::Unchanged);
        let Outcome::Run(e) = m.click(3) else {
            panic!("expected Run")
        };
        assert_eq!(m.tree.entries[e].name, "Pavucontrol");
    }

    #[test]
    fn scrolling_carries_the_selection_along() {
        let mut m = menu();
        m.key(Key::Enter);
        assert_eq!(m.scroll_by(5), Outcome::Redraw);
        assert_eq!(m.scroll(), 1);
        assert_eq!(m.selected(), 1);
        assert_eq!(m.scroll_by(1), Outcome::Unchanged);
        m.scroll_by(-3);
        assert_eq!(m.scroll(), 0);
    }

    #[test]
    fn control_characters_are_not_typed() {
        let mut m = menu();
        assert_eq!(m.text('\u{8}'), Outcome::Unchanged);
        assert_eq!(m.query(), "");
    }
}
