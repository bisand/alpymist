//! What each screen offers, and what choosing a row does.
//!
//! Pure: rows in, changed [`Answers`] out. The interactive app and the snapshot
//! example both read from here, so what you see in a rendered screenshot is
//! what you get when you walk the wizard — they cannot drift apart.

use crate::answers::{Answers, DiskPlan, Network};
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

/// Which of the account answers a text row edits.
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
}

impl TextTarget {
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
            Self::PasswordConfirm => "Confirm",
            Self::Hostname => "Hostname",
        }
    }

    /// Whether this field should be masked.
    #[must_use]
    pub fn is_secret(self) -> bool {
        matches!(self, Self::Password | Self::PasswordConfirm)
    }

    /// The answer this field edits.
    pub fn value_mut(self, a: &mut Answers) -> &mut String {
        match self {
            Self::FullName => &mut a.full_name,
            Self::Username => &mut a.username,
            Self::Password => &mut a.password,
            Self::PasswordConfirm => &mut a.password_confirm,
            Self::Hostname => &mut a.hostname,
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

/// Keyboard layouts offered, as `(label, layout, variant)`.
pub const LAYOUTS: [(&str, &str, Option<&str>); 6] = [
    ("Norwegian", "no", None),
    ("Norwegian — no dead keys", "no", Some("nodeadkeys")),
    ("Swedish", "se", None),
    ("English (UK)", "gb", None),
    ("English (US)", "us", None),
    ("German", "de", None),
];

/// Timezones offered, as `(label, IANA name)`.
pub const TIMEZONES: [(&str, &str); 6] = [
    ("Oslo", "Europe/Oslo"),
    ("Stockholm", "Europe/Stockholm"),
    ("Copenhagen", "Europe/Copenhagen"),
    ("London", "Europe/London"),
    ("Berlin", "Europe/Berlin"),
    ("UTC", "UTC"),
];

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
        Step::Keyboard => LAYOUTS
            .iter()
            .map(|(label, layout, variant)| {
                let chosen = a.keyboard.as_deref() == Some(*layout)
                    && a.keyboard_variant.as_deref() == *variant;
                Row::radio(*label, chosen)
            })
            .collect(),
        Step::Region => TIMEZONES
            .iter()
            .map(|(label, tz)| Row::radio(*label, a.timezone.as_deref() == Some(*tz)))
            .collect(),
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
            let encrypt = matches!(&a.disk, Some(DiskPlan::WholeDisk { encrypt: true, .. }));
            rows.push(Row::toggle(
                format!("[{}] Encrypt the root filesystem (LUKS2)", mark(encrypt)),
                encrypt,
            ));
            rows.push(Row::toggle(
                format!(
                    "[{}] Yes, erase everything on this disk",
                    mark(a.disk_confirmed)
                ),
                a.disk_confirmed,
            ));
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
                a.keyboard.as_deref().unwrap_or("-")
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
        Step::Keyboard => {
            if let Some((_, layout, variant)) = LAYOUTS.get(index) {
                a.keyboard = Some((*layout).to_string());
                a.keyboard_variant = variant.map(ToString::to_string);
            }
        }
        Step::Region => {
            if let Some((_, tz)) = TIMEZONES.get(index) {
                a.timezone = Some((*tz).to_string());
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
                let encrypt = matches!(&a.disk, Some(DiskPlan::WholeDisk { encrypt: true, .. }));
                a.disk = Some(DiskPlan::WholeDisk {
                    device: a.disks[i].device.clone(),
                    encrypt,
                });
            }
            // The gap row follows the disks; the two toggles follow it.
            i if i == a.disks.len() + 1 => {
                if let Some(DiskPlan::WholeDisk { device, encrypt }) = &a.disk {
                    a.disk = Some(DiskPlan::WholeDisk {
                        device: device.clone(),
                        encrypt: !encrypt,
                    });
                }
            }
            i if i == a.disks.len() + 2 => a.disk_confirmed = !a.disk_confirmed,
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
    use super::{LAYOUTS, TIERS, TIMEZONES, choose, rows, selectable};
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

    #[test]
    fn choosing_a_layout_records_both_layout_and_variant() {
        let mut a = answers();
        let dead_keys = LAYOUTS
            .iter()
            .position(|(l, ..)| *l == "Norwegian — no dead keys")
            .unwrap();
        choose(Step::Keyboard, dead_keys, &mut a);
        assert_eq!(a.keyboard.as_deref(), Some("no"));
        assert_eq!(a.keyboard_variant.as_deref(), Some("nodeadkeys"));
    }

    #[test]
    fn choosing_a_plain_layout_clears_any_previous_variant() {
        let mut a = answers();
        choose(Step::Keyboard, 1, &mut a); // no + nodeadkeys
        choose(Step::Keyboard, 0, &mut a); // plain no
        assert_eq!(a.keyboard_variant, None, "a stale variant would survive");
    }

    #[test]
    fn the_chosen_row_is_the_one_marked_chosen() {
        let mut a = answers();
        choose(Step::Region, 2, &mut a);
        let marked: Vec<usize> = rows(Step::Region, &a)
            .iter()
            .enumerate()
            .filter(|(_, r)| r.chosen)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(marked, vec![2]);
        assert_eq!(a.timezone.as_deref(), Some(TIMEZONES[2].1));
    }

    #[test]
    fn choosing_a_disk_keeps_the_encryption_setting() {
        let mut a = answers();
        choose(Step::Disk, 0, &mut a);
        choose(Step::Disk, crate::disks::sample().len() + 1, &mut a); // toggle encryption on
        assert!(matches!(
            &a.disk,
            Some(DiskPlan::WholeDisk { encrypt: true, .. })
        ));
        choose(Step::Disk, 1, &mut a); // switch to the other disk
        assert!(
            matches!(&a.disk, Some(DiskPlan::WholeDisk { device, encrypt: true }) if device == "/dev/sdb"),
            "changing disk must not silently turn encryption off: {:?}",
            a.disk
        );
    }

    #[test]
    fn the_erase_confirmation_toggles() {
        let mut a = answers();
        choose(Step::Disk, 0, &mut a);
        let confirm_row = crate::disks::sample().len() + 2;
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
            hostname: "alpymist".into(),
            detected_tier: Some(Tier::Lite),
            ..answers()
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
