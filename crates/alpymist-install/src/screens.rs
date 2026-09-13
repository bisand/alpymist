//! What each screen offers, and what choosing a row does.
//!
//! Pure: rows in, changed [`Answers`] out. The interactive app and the snapshot
//! example both read from here, so what you see in a rendered screenshot is
//! what you get when you walk the wizard — they cannot drift apart.

use crate::answers::{Answers, DiskPlan, Network};
use crate::catalog;
use crate::wizard::Step;
use alpymist_core::Tier;

/// What kind of thing a row is, which decides how the cursor treats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// One of a set: moving onto it selects it, the way a radio list works.
    Radio,
    /// An independent on/off: moving onto it must *not* flip it.
    Toggle,
    /// An editable line. Moving onto it focuses it; typing changes it.
    Text {
        /// Which answer this line edits.
        field: TextTarget,
        /// Whether to mask what is shown.
        secret: bool,
    },
    /// Only there to be read.
    Static,
}

/// Which answer a text row edits.
///
/// A small enum rather than a closure or an index, so the mapping from row to
/// answer is data the tests can enumerate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTarget {
    /// Display name.
    FullName,
    /// Login name.
    Username,
    /// Password.
    Password,
    /// Password confirmation.
    PasswordConfirm,
    /// System hostname.
    Hostname,
    /// The keyboard screen's search.
    KeyboardSearch,
    /// The time zone screen's search.
    ZoneSearch,
    /// Disk encryption passphrase.
    Passphrase,
    /// Passphrase confirmation.
    PassphraseConfirm,
}

impl TextTarget {
    /// The fields on the Encryption screen, in the order shown.
    pub const ENCRYPTION: [Self; 2] = [Self::Passphrase, Self::PassphraseConfirm];

    /// Every editable field on the Account screen, in the order shown.
    pub const ACCOUNT: [Self; 5] = [
        Self::FullName,
        Self::Username,
        Self::Password,
        Self::PasswordConfirm,
        Self::Hostname,
    ];

    /// The label shown to the left of the field.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::FullName => "Full name",
            Self::Username => "Username",
            Self::Password => "Password",

            Self::Hostname => "Hostname",
            Self::KeyboardSearch | Self::ZoneSearch => "Search",
            Self::Passphrase => "Passphrase",
            Self::PasswordConfirm | Self::PassphraseConfirm => "Confirm",
        }
    }

    /// Whether this field should be masked.
    #[must_use]
    pub fn is_secret(self) -> bool {
        matches!(
            self,
            Self::Password | Self::PasswordConfirm | Self::Passphrase | Self::PassphraseConfirm
        )
    }

    /// The answer this field edits.
    pub fn value_mut(self, a: &mut Answers) -> &mut String {
        match self {
            Self::FullName => &mut a.full_name,
            Self::Username => &mut a.username,
            Self::Password => &mut a.password,
            Self::PasswordConfirm => &mut a.password_confirm,
            Self::Hostname => &mut a.hostname,
            Self::KeyboardSearch => &mut a.keyboard_filter,
            Self::ZoneSearch => &mut a.timezone_filter,
            Self::Passphrase => &mut a.passphrase,
            Self::PassphraseConfirm => &mut a.passphrase_confirm,
        }
    }

    /// The answer this field edits.
    #[must_use]
    pub fn value(self, a: &Answers) -> &str {
        match self {
            Self::FullName => &a.full_name,
            Self::Username => &a.username,
            Self::Password => &a.password,
            Self::PasswordConfirm => &a.password_confirm,
            Self::Hostname => &a.hostname,
            Self::KeyboardSearch => &a.keyboard_filter,
            Self::ZoneSearch => &a.timezone_filter,
            Self::Passphrase => &a.passphrase,
            Self::PassphraseConfirm => &a.passphrase_confirm,
        }
    }
}

/// One line in a screen's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What to draw.
    pub text: String,
    /// How the cursor treats this row.
    pub kind: RowKind,
    /// Whether this is the currently chosen value.
    pub chosen: bool,
}

impl Row {
    /// One of a set of mutually exclusive choices.
    fn radio(text: impl Into<String>, chosen: bool) -> Self {
        Self {
            text: text.into(),
            kind: RowKind::Radio,
            chosen,
        }
    }

    /// An independent on/off.
    fn toggle(text: impl Into<String>, on: bool) -> Self {
        Self {
            text: text.into(),
            kind: RowKind::Toggle,
            chosen: on,
        }
    }

    /// Whether the cursor can land here.
    #[must_use]
    pub fn selectable(&self) -> bool {
        !matches!(self.kind, RowKind::Static)
    }

    /// The field this row edits, if it is an editable one.
    #[must_use]
    pub fn text_target(&self) -> Option<TextTarget> {
        match self.kind {
            RowKind::Text { field, .. } => Some(field),
            _ => None,
        }
    }

    /// Whether landing on this row should choose it.
    ///
    /// True for radio rows, so arrowing through a list of keyboard layouts
    /// picks as you go. False for toggles — moving past a checkbox must never
    /// flip it — and false for text, where landing merely focuses.
    #[must_use]
    pub fn selects_on_focus(&self) -> bool {
        matches!(self.kind, RowKind::Radio)
    }

    /// A line of live progress. Read-only, like a note.
    #[must_use]
    pub fn progress(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: RowKind::Static,
            chosen: false,
        }
    }

    /// A row that is only there to be read.
    fn note(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: RowKind::Static,
            chosen: false,
        }
    }

    /// A blank spacer.
    fn gap() -> Self {
        Self::note("")
    }
}

/// How many entries of a long list are shown at once.
///
/// With the search row above and the count below, this fills the eight body
/// rows every supported screen size has.
pub const LIST_ROWS: usize = 6;

/// Whether this screen is a searchable list rather than a set of rows.
///
/// On these the cursor stays in the search field, so typing always searches,
/// and the arrow keys move the choice through the list instead of the cursor.
#[must_use]
pub fn is_picker(step: Step) -> bool {
    matches!(step, Step::Keyboard | Step::Region)
}

/// Indices into the catalogue matching this screen's search, and the position
/// of the current choice among them.
fn matching(step: Step, a: &Answers) -> (Vec<usize>, Option<usize>) {
    let (found, chosen): (Vec<usize>, Option<usize>) = match step {
        Step::Keyboard => (
            catalog::keymaps()
                .iter()
                .enumerate()
                .filter(|(_, k)| k.matches(&a.keyboard_filter))
                .map(|(i, _)| i)
                .collect(),
            a.keyboard
                .as_deref()
                .zip(a.keyboard_variant.as_deref())
                .and_then(|(l, v)| catalog::keymap_index(l, v)),
        ),
        Step::Region => (
            catalog::zones()
                .iter()
                .enumerate()
                .filter(|(_, z)| z.matches(&a.timezone_filter))
                .map(|(i, _)| i)
                .collect(),
            a.timezone.as_deref().and_then(catalog::zone_index),
        ),
        _ => return (Vec::new(), None),
    };
    let at = chosen.and_then(|c| found.iter().position(|i| *i == c));
    (found, at)
}

/// The first match shown, chosen so the current choice is on screen.
///
/// Derived from the answers rather than stored, so the list cannot scroll away
/// from what is selected and there is no scroll state to get out of step.
fn window_start(total: usize, at: Option<usize>) -> usize {
    let last_start = total.saturating_sub(LIST_ROWS);
    at.map_or(0, |at| at.saturating_sub(LIST_ROWS / 2).min(last_start))
}

/// Choose an entry of the catalogue behind a picker screen.
fn choose_match(step: Step, catalogue_index: usize, a: &mut Answers) {
    match step {
        Step::Keyboard => {
            if let Some(k) = catalog::keymaps().get(catalogue_index) {
                a.keyboard = Some(k.layout.to_string());
                a.keyboard_variant = Some(k.variant.to_string());
            }
        }
        Step::Region => {
            if let Some(z) = catalog::zones().get(catalogue_index) {
                a.timezone = Some(z.zone.to_string());
            }
        }
        _ => {}
    }
}

/// Move the choice on a picker screen by `delta` matches, stopping at the ends.
///
/// With nothing chosen among the matches, any movement picks the first.
pub fn move_choice(step: Step, delta: isize, a: &mut Answers) {
    let (found, at) = matching(step, a);
    if found.is_empty() {
        return;
    }
    let next = at.map_or(0, |at| at.saturating_add_signed(delta).min(found.len() - 1));
    choose_match(step, found[next], a);
}

/// Keep a picker's choice among its matches after the search changed.
///
/// Typing `oslo` should leave Oslo chosen, so Enter takes the obvious match. If
/// nothing matches, the previous choice stands rather than vanishing.
pub fn follow_search(step: Step, a: &mut Answers) {
    let (found, at) = matching(step, a);
    if at.is_none()
        && let Some(first) = found.first()
    {
        choose_match(step, *first, a);
    }
}

/// The rows of a picker screen: search, the visible matches, then a count.
fn picker_rows(step: Step, a: &Answers) -> Vec<Row> {
    let (field, filter, total, noun) = match step {
        Step::Keyboard => (
            TextTarget::KeyboardSearch,
            &a.keyboard_filter,
            catalog::keymaps().len(),
            "layouts",
        ),
        _ => (
            TextTarget::ZoneSearch,
            &a.timezone_filter,
            catalog::zones().len(),
            "time zones",
        ),
    };
    let (found, at) = matching(step, a);
    let mut rows = vec![Row {
        text: field.label().to_string(),
        kind: RowKind::Text {
            field,
            secret: false,
        },
        chosen: false,
    }];
    let start = window_start(found.len(), at);
    for (offset, index) in found.iter().skip(start).take(LIST_ROWS).enumerate() {
        let label = match step {
            Step::Keyboard => catalog::keymaps()[*index].label(),
            _ => catalog::zones()[*index].label(),
        };
        rows.push(Row::radio(label, at == Some(start + offset)));
    }
    if found.is_empty() {
        rows.push(Row::note(format!("Nothing matches \"{}\".", filter.trim())));
    }
    while rows.len() < LIST_ROWS + 1 {
        rows.push(Row::gap());
    }
    rows.push(Row::note(if filter.trim().is_empty() {
        format!("{total} {noun}. Arrows choose, typing searches.")
    } else {
        format!("{} of {total} {noun} match.", found.len())
    }));
    rows
}

/// The tiers a user may pick, with what each actually runs.
pub const TIERS: [(Tier, &str); 4] = [
    (Tier::Full, "Full — Hyprland, animated and composited"),
    (Tier::Lite, "Lite — labwc, GPU accelerated"),
    (Tier::Potato, "Potato — labwc, rendered on the CPU"),
    (Tier::Legacy, "Legacy — X11 with i3"),
];

/// The rows this screen shows.
///
/// One long match by design: every screen's content in one place reads far
/// better than ten small functions you have to chase between.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn rows(step: Step, a: &Answers) -> Vec<Row> {
    match step {
        Step::Welcome => vec![
            // The subtitle already says what this is; the body says what to
            // expect, which is what someone about to commit a disk wants to know.
            Row::note("You will be asked about your keyboard, time zone,"),
            Row::note("network, disk and account. It takes a few minutes."),
            Row::gap(),
            Row::note("Nothing is written to any disk until you confirm."),
        ],
        Step::Keyboard | Step::Region => picker_rows(step, a),
        Step::Network => vec![
            Row::radio("Automatic (DHCP)", matches!(a.network, Some(Network::Dhcp))),
            Row::radio(
                "Static address",
                matches!(a.network, Some(Network::Static { .. })),
            ),
            Row::radio("Set up later", matches!(a.network, Some(Network::Offline))),
        ],
        Step::Disk => {
            if a.disks.is_empty() {
                return vec![
                    Row::note("No disk was found that Alpymist could be installed to."),
                    Row::gap(),
                    Row::note("The disk you booted from is never offered."),
                ];
            }
            let mut rows: Vec<Row> = a
                .disks
                .iter()
                .map(|disk| {
                    if disk.mounted_at.is_some() {
                        // Still one row per disk, so row indices stay disk indices.
                        Row::note(disk.label())
                    } else {
                        Row::radio(
                            disk.label(),
                            a.disk.as_ref().is_some_and(|d| d.device() == disk.device),
                        )
                    }
                })
                .collect();
            rows.push(Row::gap());
            if a.disks.iter().all(|d| d.mounted_at.is_some()) {
                rows.push(Row::note(
                    "Every disk here is in use, so none can be erased.",
                ));
                return rows;
            }
            rows.push(Row::toggle(
                format!(
                    "[{}] Yes, erase everything on this disk",
                    mark(a.disk_confirmed)
                ),
                a.disk_confirmed,
            ));
            rows
        }
        Step::Encryption => {
            let encrypt = a.encrypts();
            let mut rows = vec![
                Row::toggle(
                    format!("[{}] Encrypt this disk (LUKS2)", mark(encrypt)),
                    encrypt,
                ),
                Row::gap(),
            ];
            if encrypt {
                rows.extend(TextTarget::ENCRYPTION.iter().map(|field| Row {
                    text: field.label().to_string(),
                    kind: RowKind::Text {
                        field: *field,
                        secret: true,
                    },
                    chosen: false,
                }));
                rows.push(Row::gap());
                rows.push(Row::note(
                    "You will type this every time the machine starts.",
                ));
                rows.push(Row::note("Nobody can recover the disk without it."));
            } else {
                rows.push(Row::note("The disk will be readable by anyone who has it."));
            }
            rows
        }
        Step::Account => TextTarget::ACCOUNT
            .iter()
            .map(|field| Row {
                text: field.label().to_string(),
                kind: RowKind::Text {
                    field: *field,
                    secret: field.is_secret(),
                },
                chosen: false,
            })
            .collect(),
        Step::Desktop => {
            let mut rows = vec![Row::note(match a.detected_tier {
                Some(t) => format!("This machine reports: {t:?}"),
                None => "This machine was not probed.".into(),
            })];
            rows.push(Row::gap());
            rows.extend(
                TIERS
                    .iter()
                    .map(|(tier, label)| Row::radio(*label, a.effective_tier() == Some(*tier))),
            );
            rows
        }
        Step::Confirm => vec![
            Row::note(format!(
                "Keyboard    {}",
                a.keyboard_variant
                    .as_deref()
                    .or(a.keyboard.as_deref())
                    .unwrap_or("-")
            )),
            Row::note(format!(
                "Time        {}",
                a.timezone.as_deref().unwrap_or("-")
            )),
            Row::note(format!("Network     {}", describe_network(a))),
            Row::note(format!("Disk        {}", describe_disk(a))),
            Row::note(format!("Account     {} on {}", a.username, a.hostname)),
            Row::note(format!(
                "Desktop     {}",
                a.effective_tier().map_or("-".into(), |t| format!("{t:?}"))
            )),
            // Last, after everything it summarises, so it is the final thing
            // read before pressing Install.
            Row::gap(),
            Row::note(match a.disk.as_ref() {
                Some(plan) if plan.is_destructive() => format!(
                    "Everything on {} will be erased when you continue.",
                    plan.device()
                ),
                _ => "No disk will be erased.".into(),
            }),
        ],
        Step::Install => vec![
            Row::note("Partitioning the disk"),
            Row::note("Creating filesystems"),
            Row::note("Installing the base system"),
            Row::note("Installing the desktop"),
            Row::note("Configuring the bootloader"),
        ],
        Step::Done => vec![
            Row::note("Remove the installation media, then restart."),
            Row::gap(),
            Row::note("Sign in with the account you just created."),
        ],
    }
}

fn mark(on: bool) -> char {
    if on { 'x' } else { ' ' }
}

fn describe_network(a: &Answers) -> String {
    match &a.network {
        Some(Network::Dhcp) => "Automatic (DHCP)".into(),
        Some(Network::Static { address, .. }) => format!("Static {address}"),
        Some(Network::Offline) => "Set up later".into(),
        None => "-".into(),
    }
}

fn describe_disk(a: &Answers) -> String {
    match &a.disk {
        Some(DiskPlan::WholeDisk { device, encrypt }) => {
            format!(
                "{device}  erase{}",
                if *encrypt { " and encrypt" } else { "" }
            )
        }
        Some(DiskPlan::Manual { root }) => format!("{root}  use as-is"),
        None => "-".into(),
    }
}

/// Apply the choice at `index` on this screen.
///
/// Rows that are not selectable are ignored, so a caller cannot accidentally
/// "choose" a heading.
pub fn choose(step: Step, index: usize, a: &mut Answers) {
    let rows = rows(step, a);
    if !rows.get(index).is_some_and(Row::selectable) {
        return;
    }
    match step {
        Step::Keyboard | Step::Region => {
            // Row 0 is the search field; the visible matches follow it.
            let (found, at) = matching(step, a);
            if let Some(offset) = index.checked_sub(1)
                && offset < LIST_ROWS
                && let Some(i) = found.get(window_start(found.len(), at) + offset)
            {
                choose_match(step, *i, a);
            }
        }
        Step::Network => {
            a.network = Some(match index {
                0 => Network::Dhcp,
                1 => Network::Static {
                    address: String::new(),
                    gateway: String::new(),
                    dns: String::new(),
                },
                _ => Network::Offline,
            });
        }
        Step::Disk if a.disks.is_empty() => {}
        Step::Disk => match index {
            i if i < a.disks.len() && a.disks[i].mounted_at.is_none() => {
                let device = a.disks[i].device.clone();
                // Agreeing to erase one disk is not agreeing to erase another.
                if a.disk.as_ref().is_some_and(|d| d.device() != device) {
                    a.disk_confirmed = false;
                }
                // Encrypted unless the user already said otherwise.
                let encrypt = a.disk.is_none() || a.encrypts();
                a.disk = Some(DiskPlan::WholeDisk { device, encrypt });
            }
            // The gap row follows the disks; the erase confirmation follows it.
            i if i == a.disks.len() + 1 => a.disk_confirmed = !a.disk_confirmed,
            _ => {}
        },
        Step::Desktop => {
            // Two rows of preamble sit above the tier list.
            if let Some(i) = index.checked_sub(2)
                && let Some((tier, _)) = TIERS.get(i)
            {
                a.tier_override = Some(*tier);
            }
        }
        Step::Encryption => {
            if index == 0
                && let Some(DiskPlan::WholeDisk { device, encrypt }) = &a.disk
            {
                a.disk = Some(DiskPlan::WholeDisk {
                    device: device.clone(),
                    encrypt: !encrypt,
                });
            }
        }
        Step::Welcome | Step::Account | Step::Confirm | Step::Install | Step::Done => {}
    }
}

/// The row indices the cursor may land on, in order.
#[must_use]
pub fn selectable(step: Step, a: &Answers) -> Vec<usize> {
    rows(step, a)
        .iter()
        .enumerate()
        .filter(|(_, r)| r.selectable())
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{LIST_ROWS, TIERS, choose, follow_search, move_choice, rows, selectable};
    use crate::answers::{Answers, DiskPlan, Network};
    use crate::wizard::Step;

    #[test]
    fn a_disk_in_use_is_shown_but_cannot_be_chosen() {
        let mut a = answers();
        a.disks[0].mounted_at = Some("/".into());
        assert!(!selectable(Step::Disk, &a).contains(&0));
        assert!(rows(Step::Disk, &a)[0].text.contains("in use at /"));
        choose(Step::Disk, 0, &mut a);
        assert_eq!(a.disk, None);
        choose(Step::Disk, 1, &mut a);
        assert_eq!(a.disk.as_ref().map(DiskPlan::device), Some("/dev/sdb"));
    }

    #[test]
    fn a_machine_with_no_free_disk_says_so_and_offers_no_toggles() {
        let mut a = answers();
        for d in &mut a.disks {
            d.mounted_at = Some("/".into());
        }
        assert!(selectable(Step::Disk, &a).is_empty());
        let none = Answers::default();
        assert!(selectable(Step::Disk, &none).is_empty());
        assert!(rows(Step::Disk, &none)[0].text.contains("No disk"));
    }

    /// Answers on a machine that has the sample disks.
    fn answers() -> Answers {
        Answers {
            disks: crate::disks::sample(),
            ..Answers::default()
        }
    }
    use alpymist_core::Tier;

    #[test]
    fn every_screen_offers_at_least_one_row() {
        let a = answers();
        for step in Step::ALL {
            assert!(!rows(step, &a).is_empty(), "{step:?} shows nothing");
        }
    }

    #[test]
    fn the_screens_that_ask_something_have_somewhere_for_the_cursor_to_go() {
        let a = answers();
        for step in [
            Step::Keyboard,
            Step::Region,
            Step::Network,
            Step::Disk,
            Step::Encryption,
            Step::Desktop,
        ] {
            assert!(
                !selectable(step, &a).is_empty(),
                "{step:?} has no selectable row"
            );
        }
    }

    #[test]
    fn the_screens_that_only_report_have_nothing_to_select() {
        let a = answers();
        for step in [Step::Welcome, Step::Confirm, Step::Install, Step::Done] {
            assert!(
                selectable(step, &a).is_empty(),
                "{step:?} should not be selectable"
            );
        }
    }

    /// The rows of a picker that show an entry, with their indices.
    fn entries(step: Step, a: &Answers) -> Vec<(usize, String, bool)> {
        rows(step, a)
            .into_iter()
            .enumerate()
            .filter(|(_, r)| r.kind == super::RowKind::Radio)
            .map(|(i, r)| (i, r.text, r.chosen))
            .collect()
    }

    #[test]
    fn a_picker_opens_with_its_search_field_first() {
        let a = answers().with_defaults();
        for step in [Step::Keyboard, Step::Region] {
            let r = rows(step, &a);
            assert!(
                r[0].text_target().is_some(),
                "{step:?} row 0 is not the search"
            );
            assert_eq!(entries(step, &a).len(), LIST_ROWS);
        }
    }

    #[test]
    fn choosing_a_layout_records_both_layout_and_variant() {
        let mut a = answers().with_defaults();
        a.keyboard_filter = "norway nodeadkeys".into();
        follow_search(Step::Keyboard, &mut a);
        let (row, ..) = entries(Step::Keyboard, &a)
            .into_iter()
            .find(|(_, text, _)| text.starts_with("no-nodeadkeys "))
            .expect("no-nodeadkeys is listed under norway");
        choose(Step::Keyboard, row, &mut a);
        assert_eq!(a.keyboard.as_deref(), Some("no"));
        assert_eq!(a.keyboard_variant.as_deref(), Some("no-nodeadkeys"));
    }

    /// Typing the obvious thing and pressing Enter should get the obvious thing.
    #[test]
    fn searching_chooses_the_first_match_when_the_choice_is_filtered_out() {
        let mut a = answers().with_defaults();
        a.timezone_filter = "oslo".into();
        follow_search(Step::Region, &mut a);
        assert_eq!(a.timezone.as_deref(), Some("Europe/Oslo"));

        a.keyboard_filter = "norway".into();
        follow_search(Step::Keyboard, &mut a);
        assert_eq!(a.keyboard.as_deref(), Some("no"));
    }

    #[test]
    fn searching_keeps_a_choice_that_still_matches() {
        let mut a = answers().with_defaults();
        a.timezone = Some("Europe/Stockholm".into());
        a.timezone_filter = "europe".into();
        follow_search(Step::Region, &mut a);
        assert_eq!(a.timezone.as_deref(), Some("Europe/Stockholm"));
    }

    #[test]
    fn a_search_with_no_matches_keeps_the_choice_and_says_so() {
        let mut a = answers().with_defaults();
        a.timezone_filter = "atlantis".into();
        follow_search(Step::Region, &mut a);
        assert_eq!(a.timezone.as_deref(), Some("UTC"));
        assert!(entries(Step::Region, &a).is_empty());
        assert!(
            rows(Step::Region, &a)
                .iter()
                .any(|r| r.text.contains("Nothing matches"))
        );
    }

    /// However far the choice moves, it stays on screen and marked.
    #[test]
    fn the_list_scrolls_to_keep_the_choice_visible() {
        let mut a = answers().with_defaults();
        a.timezone = Some(crate::catalog::zones()[0].zone.into());
        for _ in 0..50 {
            move_choice(Step::Region, 1, &mut a);
            let chosen: Vec<_> = entries(Step::Region, &a)
                .into_iter()
                .filter(|e| e.2)
                .collect();
            assert_eq!(chosen.len(), 1, "exactly one visible row is chosen");
        }
        assert_eq!(
            a.timezone.as_deref(),
            Some(crate::catalog::zones()[50].zone)
        );
        for _ in 0..10_000 {
            move_choice(Step::Region, 1, &mut a);
        }
        assert_eq!(
            a.timezone.as_deref(),
            crate::catalog::zones().last().map(|z| z.zone),
            "moving past the end stops at the end"
        );
        move_choice(Step::Region, -10_000, &mut a);
        assert_eq!(a.timezone.as_deref(), Some(crate::catalog::zones()[0].zone));
    }

    #[test]
    fn the_chosen_row_is_the_one_marked_chosen() {
        let mut a = answers().with_defaults();
        let (row, ..) = entries(Step::Region, &a)[2].clone();
        choose(Step::Region, row, &mut a);
        let marked: Vec<String> = entries(Step::Region, &a)
            .into_iter()
            .filter(|e| e.2)
            .map(|e| e.1)
            .collect();
        assert_eq!(marked.len(), 1);
        let zone = a.timezone.clone().unwrap().replace('_', " ");
        assert!(marked[0].starts_with(&zone), "{marked:?} vs {zone}");
    }

    #[test]
    fn choosing_a_disk_keeps_the_encryption_setting_and_forgets_the_erase() {
        let mut a = answers();
        choose(Step::Disk, 0, &mut a);
        assert!(a.encrypts(), "a first choice is encrypted by default");
        choose(Step::Encryption, 0, &mut a);
        assert!(!a.encrypts());
        choose(Step::Disk, crate::disks::sample().len() + 1, &mut a);
        assert!(a.disk_confirmed);
        choose(Step::Disk, 1, &mut a);
        assert!(
            matches!(&a.disk, Some(DiskPlan::WholeDisk { device, encrypt: false }) if device == "/dev/sdb"),
            "changing disk must keep the encryption choice: {:?}",
            a.disk
        );
        assert!(
            !a.disk_confirmed,
            "agreeing to erase one disk is not agreeing to erase another"
        );
    }

    #[test]
    fn the_passphrase_fields_appear_only_when_encrypting() {
        let mut a = answers();
        choose(Step::Disk, 0, &mut a);
        let fields = |a: &Answers| {
            rows(Step::Encryption, a)
                .iter()
                .filter(|r| r.text_target().is_some())
                .count()
        };
        assert_eq!(fields(&a), 2);
        assert!(
            rows(Step::Encryption, &a)
                .iter()
                .all(|r| r.text_target().is_none_or(super::TextTarget::is_secret))
        );
        choose(Step::Encryption, 0, &mut a);
        assert_eq!(fields(&a), 0);
    }

    #[test]
    fn the_erase_confirmation_toggles() {
        let mut a = answers();
        choose(Step::Disk, 0, &mut a);
        let confirm_row = crate::disks::sample().len() + 1;
        choose(Step::Disk, confirm_row, &mut a);
        assert!(a.disk_confirmed);
        choose(Step::Disk, confirm_row, &mut a);
        assert!(!a.disk_confirmed, "it must be possible to take it back");
    }

    #[test]
    fn choosing_a_tier_records_it_as_an_override() {
        let mut a = Answers {
            detected_tier: Some(Tier::Potato),
            ..answers()
        };
        let full = TIERS.iter().position(|(t, _)| *t == Tier::Full).unwrap();
        choose(Step::Desktop, full + 2, &mut a); // two preamble rows
        assert_eq!(a.tier_override, Some(Tier::Full));
        assert_eq!(a.effective_tier(), Some(Tier::Full));
    }

    /// A heading is not a choice; landing on one must change nothing.
    #[test]
    fn choosing_an_unselectable_row_does_nothing() {
        let mut a = Answers {
            detected_tier: Some(Tier::Lite),
            ..answers()
        };
        let before = a.clone();
        choose(Step::Desktop, 0, &mut a); // the "reports:" heading
        choose(Step::Desktop, 1, &mut a); // the gap
        assert_eq!(a, before);
    }

    #[test]
    fn choosing_past_the_end_of_a_screen_does_nothing() {
        let mut a = answers();
        let before = a.clone();
        choose(Step::Region, 999, &mut a);
        assert_eq!(a, before);
    }

    #[test]
    fn network_rows_map_to_the_right_configuration() {
        for (index, expect_dhcp, expect_offline) in
            [(0, true, false), (1, false, false), (2, false, true)]
        {
            let mut a = answers();
            choose(Step::Network, index, &mut a);
            assert_eq!(
                matches!(a.network, Some(Network::Dhcp)),
                expect_dhcp,
                "row {index}"
            );
            assert_eq!(
                matches!(a.network, Some(Network::Offline)),
                expect_offline,
                "row {index}"
            );
        }
    }

    /// A fully answered wizard, so every screen shows its longest form.
    fn full() -> Answers {
        Answers {
            keyboard: Some("no".into()),
            timezone: Some("Europe/Oslo".into()),
            network: Some(Network::Dhcp),
            disk: Some(DiskPlan::WholeDisk {
                device: "/dev/sda".into(),
                encrypt: true,
            }),
            disk_confirmed: true,
            username: "andre".into(),
            full_name: "André Biseth".into(),
            password: "secret".into(),
            password_confirm: "secret".into(),
            passphrase: "secret".into(),
            passphrase_confirm: "secret".into(),
            hostname: "alpymist".into(),
            detected_tier: Some(Tier::Lite),
            ..answers().with_defaults()
        }
    }

    /// The panel is smaller than it used to be; no screen may overflow it at
    /// any resolution we claim to support, including a 1024x600 netbook.
    #[test]
    fn every_screen_fits_inside_the_panel_at_every_supported_size() {
        use alpymist_ui::chrome::Chrome;
        let a = full();
        for (w, h) in [
            (640, 480),
            (800, 600),
            (1024, 600),
            (1024, 768),
            (1280, 800),
            (1366, 768),
            (1920, 1080),
            (2560, 1440),
        ] {
            let room = usize::try_from(Chrome::for_screen(w, h).body_rows()).unwrap();
            for step in Step::ALL {
                let needed = rows(step, &a).len();
                assert!(
                    needed <= room,
                    "{w}x{h}: {step:?} needs {needed} rows, panel has {room}"
                );
            }
        }
    }

    /// Catches a body line restating the subtitle above it, as the Welcome
    /// screen did: two sentences sharing nearly all their words.
    #[test]
    fn no_screen_repeats_its_subtitle_in_the_body() {
        let words = |s: &str| -> std::collections::BTreeSet<String> {
            s.split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(str::to_lowercase)
                .collect()
        };
        let a = full();
        for step in Step::ALL {
            let sub = words(step.subtitle());
            for row in rows(step, &a) {
                let body = words(&row.text);
                if body.is_empty() || sub.is_empty() {
                    continue;
                }
                let shared = sub.intersection(&body).count();
                let union = sub.union(&body).count();
                assert!(
                    shared * 10 < union * 6,
                    "{step:?}: {:?} repeats the subtitle {:?}",
                    row.text,
                    step.subtitle()
                );
            }
        }
    }

    #[test]
    fn the_password_is_never_shown_in_the_rows() {
        let a = Answers {
            password: "hunter2".into(),
            password_confirm: "hunter2".into(),
            ..answers()
        };
        for row in rows(Step::Account, &a) {
            assert!(
                !row.text.contains("hunter2"),
                "password leaked into {:?}",
                row.text
            );
        }
    }
}
