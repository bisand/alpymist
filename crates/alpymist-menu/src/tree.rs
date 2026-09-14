//! The menu as one tree: the configuration's submenus, with the installed
//! applications grafted in.
//!
//! Menus refer to each other by name in the file, which means a menu can be
//! reached from two places and a careless edit can make a loop. Both are fine
//! here: the tree is an arena of menus and entries addressed by index, walking
//! it tracks where it has been, and nothing ever recurses on the structure.

use crate::apps::App;
use crate::config::{self, Config};
use crate::exec::Launch;
use std::collections::HashMap;

/// Index of a menu in [`Tree::menus`].
pub type MenuId = usize;
/// Index of an entry in [`Tree::entries`].
pub type EntryId = usize;

/// What choosing an entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Open a submenu.
    Open(MenuId),
    /// Run something and close.
    Run(Launch),
}

/// One row of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// What it is called.
    pub name: String,
    /// Its glyph, possibly empty.
    pub icon: String,
    /// Dimmer text after the name.
    pub detail: Option<String>,
    /// Other words it is found by.
    pub keywords: Vec<String>,
    /// What choosing it does.
    pub action: Action,
    /// Identifies it in launch history across restarts.
    pub key: String,
}

/// One submenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Its name in the configuration.
    pub name: String,
    /// What the header says while it is open.
    pub title: String,
    /// Its entries, in order.
    pub entries: Vec<EntryId>,
    /// Whether an unfiltered list is ordered by use rather than as written.
    /// True of the applications, where nobody chose an order.
    pub by_use: bool,
}

/// The whole menu.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    /// Every menu.
    pub menus: Vec<Node>,
    /// Every entry.
    pub entries: Vec<Entry>,
    by_name: HashMap<String, MenuId>,
}

/// An entry reachable from some menu, and the way there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reachable {
    /// The entry.
    pub entry: EntryId,
    /// Titles of the submenus between the starting menu and the entry, joined
    /// with ` › `. Empty for the starting menu's own entries.
    pub trail: String,
    /// How many submenus down it is. Zero for the starting menu's own.
    pub depth: usize,
}

impl Tree {
    /// Build from a checked configuration and the installed applications.
    #[must_use]
    pub fn build(config: &Config, apps: Vec<App>) -> Self {
        let mut tree = Self::default();
        // Names first, so an entry can refer to a menu defined after it.
        for (name, def) in &config.menu {
            tree.add_menu(name, def.title.clone().unwrap_or_else(|| name.clone()));
        }
        if !tree.by_name.contains_key(config::APPS) {
            let id = tree.add_menu(config::APPS, "Apps".into());
            tree.menus[id].by_use = true;
            for app in apps {
                let entry = Entry {
                    key: format!("app:{}", app.id),
                    name: app.name,
                    icon: app.icon.into(),
                    detail: app.detail,
                    keywords: app.keywords,
                    action: Action::Run(app.launch),
                };
                tree.push(id, entry);
            }
        }
        for (name, def) in &config.menu {
            let id = tree.by_name[name];
            for item in &def.items {
                let action = match (&item.exec, &item.menu) {
                    (_, Some(target)) => match tree.by_name.get(target) {
                        Some(&m) => Action::Open(m),
                        // Config::parse rejects this; a hand-built Config
                        // might not have been through it.
                        None => continue,
                    },
                    (Some(command), None) => Action::Run(Launch::Shell {
                        command: command.clone(),
                        terminal: item.terminal,
                        hold: item.hold,
                    }),
                    (None, None) => continue,
                };
                let entry = Entry {
                    key: format!("{name}:{}", item.name),
                    name: item.name.clone(),
                    icon: item.icon.clone().unwrap_or_default(),
                    detail: item.detail.clone(),
                    keywords: item.keywords.clone(),
                    action,
                };
                tree.push(id, entry);
            }
        }
        tree
    }

    fn add_menu(&mut self, name: &str, title: String) -> MenuId {
        let id = self.menus.len();
        self.menus.push(Node {
            name: name.to_owned(),
            title,
            entries: Vec::new(),
            by_use: false,
        });
        self.by_name.insert(name.to_owned(), id);
        id
    }

    fn push(&mut self, menu: MenuId, entry: Entry) {
        self.menus[menu].entries.push(self.entries.len());
        self.entries.push(entry);
    }

    /// A menu by its configuration name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<MenuId> {
        self.by_name.get(name).copied()
    }

    /// Every entry reachable from `from`, breadth first, each menu visited once.
    ///
    /// Breadth first so a menu reachable two ways is credited to the shorter
    /// path, which is also the one its trail should show.
    #[must_use]
    pub fn reachable(&self, from: MenuId) -> Vec<Reachable> {
        let mut out = Vec::new();
        let mut visited = vec![false; self.menus.len()];
        let mut queue = std::collections::VecDeque::new();
        visited[from] = true;
        queue.push_back((from, String::new(), 0));
        while let Some((menu, trail, depth)) = queue.pop_front() {
            for &entry in &self.menus[menu].entries {
                out.push(Reachable {
                    entry,
                    trail: trail.clone(),
                    depth,
                });
                if let Action::Open(sub) = self.entries[entry].action
                    && !visited[sub]
                {
                    visited[sub] = true;
                    let title = &self.menus[sub].title;
                    let next = if trail.is_empty() {
                        title.clone()
                    } else {
                        format!("{trail} › {title}")
                    };
                    queue.push_back((sub, next, depth + 1));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Tree};
    use crate::apps::App;
    use crate::config::Config;
    use crate::exec::Launch;

    fn app(id: &str, name: &str) -> App {
        App {
            id: id.into(),
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

    const LOOPY: &str = r#"
        [menu.root]
        items = [
          { name = "Apps", menu = "apps" },
          { name = "System", menu = "system" },
          { name = "Lock", exec = "swaylock" },
        ]
        [menu.system]
        title = "System"
        items = [
          { name = "Back to the top", menu = "root" },
          { name = "Power", menu = "power" },
        ]
        [menu.power]
        title = "Power"
        items = [{ name = "Shut down", exec = "poweroff" }]
    "#;

    fn tree() -> Tree {
        let config = Config::parse(LOOPY).unwrap();
        Tree::build(&config, vec![app("foot.desktop", "Foot")])
    }

    #[test]
    fn menus_are_found_by_name_and_apps_are_grafted_in() {
        let t = tree();
        let apps = t.find("apps").unwrap();
        assert!(t.menus[apps].by_use);
        assert_eq!(t.entries[t.menus[apps].entries[0]].name, "Foot");
        assert_eq!(t.entries[t.menus[apps].entries[0]].key, "app:foot.desktop");
        assert!(t.find("nowhere").is_none());
    }

    #[test]
    fn a_menu_may_refer_to_one_defined_later() {
        let t = tree();
        let system = t.find("system").unwrap();
        let power = t.entries[t.menus[system].entries[1]].action.clone();
        assert_eq!(power, Action::Open(t.find("power").unwrap()));
    }

    #[test]
    fn walking_a_loop_terminates_and_lists_everything_once() {
        let t = tree();
        let all = t.reachable(t.find("root").unwrap());
        let names: Vec<&str> = all
            .iter()
            .map(|r| t.entries[r.entry].name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "Apps",
                "System",
                "Lock",
                "Foot",
                "Back to the top",
                "Power",
                "Shut down"
            ]
        );
    }

    #[test]
    fn trails_name_the_way_down() {
        let t = tree();
        let all = t.reachable(t.find("root").unwrap());
        let shut = all
            .iter()
            .find(|r| t.entries[r.entry].name == "Shut down")
            .unwrap();
        assert_eq!(shut.trail, "System › Power");
        assert_eq!(shut.depth, 2);
        let lock = all
            .iter()
            .find(|r| t.entries[r.entry].name == "Lock")
            .unwrap();
        assert_eq!((lock.trail.as_str(), lock.depth), ("", 0));
    }

    #[test]
    fn a_configured_apps_menu_replaces_the_built_in_one() {
        let text = r#"
            [menu.root]
            items = [{ name = "Apps", menu = "apps" }]
            [menu.apps]
            items = [{ name = "Only this", exec = "true" }]
        "#;
        let t = Tree::build(
            &Config::parse(text).unwrap(),
            vec![app("foot.desktop", "Foot")],
        );
        let apps = t.find("apps").unwrap();
        assert_eq!(t.menus[apps].entries.len(), 1);
        assert!(!t.menus[apps].by_use);
    }
}
