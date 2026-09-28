//! The clipboard picker: `alpymist-menu clipboard`, on Super+Shift+V.
//!
//! The history's entries, newest first and pinned ones marked, searched like
//! any menu. Enter puts one back on the clipboard and pastes it into the
//! window the picker was opened over; Ctrl+S pins or unpins it and Delete
//! forgets it, and the list is read again with the menu still open. With no
//! history kept, the one entry says where to turn it on.

use crate::exec::Launch;
use crate::tree::{Action, Alternate, Entry};
use alpymist_clipboard::client;

/// The picker's menu name, as `alpymist-menu clipboard` asks for it.
pub const MENU: &str = "clipboard";
/// What its header says.
pub const TITLE: &str = "Clipboard";

fn alpymist(args: &[&str]) -> Launch {
    Launch::Argv {
        argv: std::iter::once("alpymist")
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect(),
        terminal: false,
    }
}

/// The history as entries, or the one entry that says why there is none.
#[must_use]
pub fn entries() -> Vec<Entry> {
    match client::list() {
        Ok(list) if list.is_empty() => vec![note(
            "Nothing copied yet",
            "What you copy appears here",
            alpymist(&["clipboard", "list"]),
        )],
        Ok(list) => list
            .into_iter()
            .map(|item| {
                let id = item.id.to_string();
                let text = alpymist_clipboard::history::is_text(&item.mime);
                Entry {
                    name: item.label.clone(),
                    icon: match (item.pinned, text) {
                        (true, _) => "\u{f0403}".into(),
                        (false, true) => "\u{f014c}".into(),
                        (false, false) => "\u{f02e9}".into(),
                    },
                    detail: item.pinned.then(|| "pinned".to_owned()),
                    keywords: Vec::new(),
                    key: format!("clip:{id}"),
                    action: Action::Run(alpymist(&["clipboard", "restore", &id])),
                    alternates: vec![
                        (
                            Alternate::Pin,
                            alpymist(&[
                                "clipboard",
                                if item.pinned { "unpin" } else { "pin" },
                                &id,
                            ]),
                        ),
                        (Alternate::Forget, alpymist(&["clipboard", "forget", &id])),
                    ],
                }
            })
            .collect(),
        Err(_) => vec![note(
            "The clipboard history is off",
            "Turn it on in Settings › Clipboard",
            Launch::Argv {
                argv: vec!["alpymist-settings".into(), "clipboard".into()],
                terminal: false,
            },
        )],
    }
}

fn note(name: &str, detail: &str, launch: Launch) -> Entry {
    Entry {
        name: name.to_owned(),
        icon: "\u{f014c}".into(),
        detail: Some(detail.to_owned()),
        keywords: Vec::new(),
        key: format!("clip:{name}"),
        action: Action::Run(launch),
        alternates: Vec::new(),
    }
}
