//! What the user tells the installer, and whether it makes sense.
//!
//! Pure data and pure validation: no rendering, no filesystem, no side effects.
//! Everything the wizard will eventually *do* is decided from this struct, so
//! this is the part that most deserves testing.

use alpymist_core::Tier;

/// A problem with what has been entered so far.
///
/// Carries a message written for the person at the keyboard, not for a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Which field the problem belongs to, for focusing it.
    pub field: Field,
    /// What to tell the user.
    pub message: String,
}

/// The fields a problem can attach to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Keyboard layout selection.
    Keyboard,
    /// Timezone selection.
    Timezone,
    /// Network configuration.
    Network,
    /// Disk selection.
    Disk,
    /// The confirmation that erasing the disk is intended.
    DiskConfirm,
    /// Account username.
    Username,
    /// Account full name.
    FullName,
    /// Account password.
    Password,
    /// Password confirmation.
    PasswordConfirm,
    /// System hostname.
    Hostname,
    /// Disk encryption passphrase.
    Passphrase,
    /// Passphrase confirmation.
    PassphraseConfirm,
}

/// How the machine starts, which decides the partition layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Firmware {
    /// UEFI: a GPT disk with an EFI system partition. Every aarch64 machine
    /// Alpymist runs on, and most x86 ones made this century.
    #[default]
    Uefi,
    /// Legacy BIOS: an MBR disk with a small boot partition.
    Bios,
}

impl Firmware {
    /// How the running machine booted, which is how the installed one will.
    #[must_use]
    pub fn detect() -> Self {
        if std::path::Path::new("/sys/firmware/efi").exists() {
            Self::Uefi
        } else if cfg!(target_arch = "x86_64") || cfg!(target_arch = "x86") {
            Self::Bios
        } else {
            // Everything else Alpymist runs on boots through UEFI, and a
            // missing sysfs entry there says more about sysfs than firmware.
            Self::Uefi
        }
    }
}

/// How the machine gets on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network {
    /// Ask a DHCP server. The default, and right almost always.
    Dhcp,
    /// A fixed address, for machines that need one.
    Static {
        /// Address in CIDR form, e.g. `192.168.1.10/24`.
        address: String,
        /// Default gateway.
        gateway: String,
        /// Name server.
        dns: String,
    },
    /// A Wi-Fi network, joined on the Network screen before anything is written.
    Wifi {
        /// The network's name.
        ssid: String,
    },
    /// Set the network up later; the installer does not need it.
    Offline,
}

/// What to do with the target disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskPlan {
    /// Erase the whole disk and lay out a standard scheme.
    WholeDisk {
        /// Device path, e.g. `/dev/sda`.
        device: String,
        /// Whether to put the root filesystem inside LUKS2.
        encrypt: bool,
    },
    /// The user will partition it themselves; the installer only formats.
    Manual {
        /// Device path of the partition to use as root.
        root: String,
    },
}

impl DiskPlan {
    /// The device this plan will write to.
    #[must_use]
    pub fn device(&self) -> &str {
        match self {
            Self::WholeDisk { device, .. } => device,
            Self::Manual { root } => root,
        }
    }

    /// Whether following this plan destroys existing data.
    #[must_use]
    pub fn is_destructive(&self) -> bool {
        matches!(self, Self::WholeDisk { .. })
    }
}

/// Everything the wizard has collected.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Answers {
    /// Console and desktop keyboard layout, e.g. `no`.
    pub keyboard: Option<String>,
    /// Keymap within the layout, e.g. `no-nodeadkeys`.
    pub keyboard_variant: Option<String>,
    /// What has been typed into the keyboard screen's search.
    pub keyboard_filter: String,
    /// IANA timezone, e.g. `Europe/Oslo`.
    pub timezone: Option<String>,
    /// What has been typed into the time zone screen's search.
    pub timezone_filter: String,
    /// Network configuration.
    pub network: Option<Network>,
    /// The Wi-Fi networks in reach, and whether one has been joined.
    pub wifi: crate::wifi::Wifi,
    /// The wired interface a cable would be plugged into, if there is one.
    pub wired_interface: Option<String>,
    /// What to do with the disk.
    pub disk: Option<DiskPlan>,
    /// Set when the user has acknowledged that the disk will be erased.
    pub disk_confirmed: bool,
    /// Passphrase for the encrypted disk.
    pub passphrase: String,
    /// Passphrase, again.
    pub passphrase_confirm: String,
    /// Login name.
    pub username: String,
    /// Display name, optional.
    pub full_name: String,
    /// Password.
    pub password: String,
    /// Password, again.
    pub password_confirm: String,
    /// System hostname.
    pub hostname: String,
    /// Disks found on this machine, offered on the disk screen.
    pub disks: Vec<crate::disks::Disk>,
    /// How this machine boots.
    pub firmware: Firmware,
    /// Whether typed characters come already translated by an operating system
    /// that follows the chosen layout, as in the desktop preview. When false,
    /// the installer translates keys itself and can only do so for the layouts
    /// Denise has tables for.
    pub typed_by_os: bool,
    /// Desktop tier the probe detected, if it ran.
    pub detected_tier: Option<Tier>,
    /// Tier the user chose instead, if they overrode the detection.
    pub tier_override: Option<Tier>,
}

/// The longest a Linux login name may be.
const MAX_USERNAME: usize = 32;
/// The shortest password we will accept without remarking on it.
///
/// Not enforced: see `Wizard::advisories`.
pub const MIN_PASSWORD: usize = 8;
/// The longest a single hostname label may be, per RFC 1123.
const MAX_HOSTNAME_LABEL: usize = 63;

impl std::fmt::Debug for Answers {
    /// Everything except the secrets, which a log has no business holding.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let masked = |s: &str| if s.is_empty() { "" } else { "<set>" };
        f.debug_struct("Answers")
            .field("keyboard", &self.keyboard)
            .field("keyboard_variant", &self.keyboard_variant)
            .field("timezone", &self.timezone)
            .field("network", &self.network)
            .field("wifi", &self.wifi)
            .field("wired_interface", &self.wired_interface)
            .field("disk", &self.disk)
            .field("disk_confirmed", &self.disk_confirmed)
            .field("passphrase", &masked(&self.passphrase))
            .field("username", &self.username)
            .field("full_name", &self.full_name)
            .field("password", &masked(&self.password))
            .field("hostname", &self.hostname)
            .field("firmware", &self.firmware)
            .field("detected_tier", &self.detected_tier)
            .field("tier_override", &self.tier_override)
            .finish_non_exhaustive()
    }
}

impl Answers {
    /// Fill every unanswered question that has a sensible answer.
    ///
    /// The point is that someone who agrees with the defaults can press Enter
    /// through the wizard, stopping only where a default cannot exist — their
    /// name, their password — or must not — agreeing to erase a disk.
    #[must_use]
    pub fn with_defaults(mut self) -> Self {
        if self.keyboard.is_none() {
            let (layout, variant) = crate::catalog::DEFAULT_KEYMAP;
            self.keyboard = Some(layout.into());
            self.keyboard_variant = Some(variant.into());
        }
        if self.timezone.is_none() {
            self.timezone = Some(crate::catalog::DEFAULT_ZONE.into());
        }
        if self.network.is_none() {
            self.network = Some(Network::Dhcp);
        }
        if self.disk.is_none()
            && let Some(free) = self.disks.iter().find(|d| d.mounted_at.is_none())
        {
            // Encrypted unless the user says otherwise: this is the distro
            // that is supposed to get security right by default.
            self.disk = Some(DiskPlan::WholeDisk {
                device: free.device.clone(),
                encrypt: true,
            });
        }
        if self.hostname.is_empty() {
            self.hostname = "alpymist".into();
        }
        self
    }

    /// The tier this machine will actually be set up for.
    ///
    /// An explicit choice always wins over the probe: the user may know
    /// something the probe cannot see, and being unable to overrule it would be
    /// worse than occasionally choosing wrong.
    #[must_use]
    pub fn effective_tier(&self) -> Option<Tier> {
        self.tier_override.or(self.detected_tier)
    }

    /// Whether the user overrode what the probe detected.
    #[must_use]
    pub fn tier_was_overridden(&self) -> bool {
        matches!((self.tier_override, self.detected_tier),
            (Some(chosen), Some(detected)) if chosen != detected)
    }
}

/// Validate a Linux login name.
///
/// Deliberately stricter than Linux strictly requires: portable names avoid a
/// long tail of breakage in scripts, mail addresses and file permissions that
/// is tedious to diagnose long after installation.
///
/// # Errors
/// Returns a message describing the first problem found.
pub fn validate_username(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Choose a username.".into());
    }
    if name.len() > MAX_USERNAME {
        return Err(format!(
            "Usernames can be at most {MAX_USERNAME} characters."
        ));
    }
    let first = name.chars().next().unwrap_or('0');
    if !(first.is_ascii_lowercase() || first == '_') {
        return Err("Usernames must start with a lowercase letter.".into());
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-'))
    {
        return Err(format!(
            "{bad:?} is not allowed. Use lowercase letters, digits, - and _."
        ));
    }
    Ok(())
}

/// Validate a hostname against RFC 1123.
///
/// # Errors
/// Returns a message describing the first problem found.
pub fn validate_hostname(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Choose a name for this machine.".into());
    }
    for label in name.split('.') {
        if label.is_empty() {
            return Err("Hostnames cannot contain an empty part.".into());
        }
        if label.len() > MAX_HOSTNAME_LABEL {
            return Err(format!(
                "Each part may be at most {MAX_HOSTNAME_LABEL} characters."
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err("Hostname parts cannot start or end with a hyphen.".into());
        }
        if let Some(bad) = label
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '-'))
        {
            return Err(format!("{bad:?} is not allowed in a hostname."));
        }
    }
    Ok(())
}

/// Validate an IPv4 address, optionally with a `/prefix`.
///
/// # Errors
/// Returns a message describing the problem.
pub fn validate_ipv4(text: &str, require_prefix: bool) -> Result<(), String> {
    let (addr, prefix) = match text.split_once('/') {
        Some((a, p)) => (a, Some(p)),
        None => (text, None),
    };
    if require_prefix && prefix.is_none() {
        return Err("Include the prefix length, for example 192.168.1.10/24.".into());
    }
    if !require_prefix && prefix.is_some() {
        return Err("Give a plain address here, with no /prefix.".into());
    }

    let parts: Vec<&str> = addr.split('.').collect();
    if parts.len() != 4 {
        return Err("An IPv4 address has four parts, for example 192.168.1.10.".into());
    }
    for part in parts {
        if part.is_empty() || part.len() > 3 || !part.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("{part:?} is not a number between 0 and 255."));
        }
        if part.parse::<u32>().unwrap_or(256) > 255 {
            return Err(format!("{part} is above 255."));
        }
    }
    if let Some(prefix) = prefix {
        match prefix.parse::<u32>() {
            Ok(p) if p <= 32 => {}
            _ => return Err("The prefix length must be between 0 and 32.".into()),
        }
    }
    Ok(())
}

/// Whether `gateway` is on the network `address` (with its prefix) is on.
///
/// A default route through a gateway outside the local network cannot be
/// added, so a system configured that way comes up with no network at all.
/// Both must already be valid; anything else answers `true`, leaving the
/// complaint to [`validate_ipv4`].
#[must_use]
pub fn gateway_is_local(address: &str, gateway: &str) -> bool {
    let parse = |text: &str| -> Option<u32> {
        let parts: Vec<u32> = text
            .split('.')
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        let [a, b, c, d] = parts.as_slice() else {
            return None;
        };
        Some((a << 24) | (b << 16) | (c << 8) | d)
    };
    let Some((host, prefix)) = address.split_once('/') else {
        return true;
    };
    let (Some(host), Some(gateway), Ok(prefix)) =
        (parse(host), parse(gateway), prefix.parse::<u32>())
    else {
        return true;
    };
    let mask = u32::MAX.checked_shl(32 - prefix.min(32)).unwrap_or(0);
    host & mask == gateway & mask
}

#[cfg(test)]
mod tests {
    use super::{
        Answers, DiskPlan, gateway_is_local, validate_hostname, validate_ipv4, validate_username,
    };

    #[test]
    fn a_gateway_must_be_on_the_addresss_own_network() {
        assert!(gateway_is_local("192.168.1.10/24", "192.168.1.1"));
        assert!(!gateway_is_local("192.168.1.10/24", "192.168.2.1"));
        assert!(gateway_is_local("10.0.5.9/8", "10.200.0.1"));
        assert!(!gateway_is_local("10.0.5.9/32", "10.0.5.1"));
        assert!(gateway_is_local("0.0.0.0/0", "8.8.8.8"));
        assert!(
            gateway_is_local("nonsense", "192.168.1.1"),
            "left to validate_ipv4"
        );
    }
    use alpymist_core::Tier;

    #[test]
    fn ordinary_usernames_are_accepted() {
        for name in ["andre", "bisand", "a", "user_1", "web-admin", "_svc"] {
            assert!(validate_username(name).is_ok(), "{name} should be valid");
        }
    }

    #[test]
    fn usernames_that_would_cause_trouble_later_are_refused() {
        for (name, why) in [
            ("", "empty"),
            ("Andre", "uppercase"),
            ("1andre", "leading digit"),
            ("-andre", "leading hyphen"),
            ("an dre", "space"),
            ("andré", "non-ascii"),
            ("root:x", "colon"),
        ] {
            assert!(
                validate_username(name).is_err(),
                "{name:?} ({why}) should be refused"
            );
        }
    }

    #[test]
    fn an_over_long_username_is_refused() {
        assert!(validate_username(&"a".repeat(32)).is_ok());
        assert!(validate_username(&"a".repeat(33)).is_err());
    }

    #[test]
    fn ordinary_hostnames_are_accepted() {
        for name in ["alpymist", "my-laptop", "box1", "a.b.c", "x"] {
            assert!(validate_hostname(name).is_ok(), "{name} should be valid");
        }
    }

    #[test]
    fn malformed_hostnames_are_refused() {
        for (name, why) in [
            ("", "empty"),
            ("-box", "leading hyphen"),
            ("box-", "trailing hyphen"),
            ("a..b", "empty label"),
            ("box_1", "underscore"),
            ("bø", "non-ascii"),
        ] {
            assert!(
                validate_hostname(name).is_err(),
                "{name:?} ({why}) should be refused"
            );
        }
    }

    #[test]
    fn a_label_longer_than_sixty_three_characters_is_refused() {
        assert!(validate_hostname(&"a".repeat(63)).is_ok());
        assert!(validate_hostname(&"a".repeat(64)).is_err());
    }

    #[test]
    fn addresses_with_a_prefix_are_accepted_where_one_is_required() {
        for addr in [
            "192.168.1.10/24",
            "10.0.0.1/8",
            "0.0.0.0/0",
            "255.255.255.255/32",
        ] {
            assert!(validate_ipv4(addr, true).is_ok(), "{addr} should be valid");
        }
    }

    #[test]
    fn a_missing_or_unwanted_prefix_is_reported() {
        assert!(
            validate_ipv4("192.168.1.10", true).is_err(),
            "prefix required"
        );
        assert!(
            validate_ipv4("192.168.1.1/24", false).is_err(),
            "prefix not wanted"
        );
        assert!(validate_ipv4("192.168.1.1", false).is_ok());
    }

    #[test]
    fn malformed_addresses_are_refused() {
        for bad in [
            "",
            "192.168.1",
            "192.168.1.1.1",
            "192.168.1.256",
            "a.b.c.d",
            "1.2.3.-1",
        ] {
            assert!(
                validate_ipv4(bad, false).is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn a_prefix_above_thirty_two_is_refused() {
        assert!(validate_ipv4("10.0.0.1/32", true).is_ok());
        assert!(validate_ipv4("10.0.0.1/33", true).is_err());
    }

    /// The user may know something the probe cannot see, so their choice wins.
    #[test]
    fn an_explicit_tier_choice_overrides_what_was_detected() {
        let a = Answers {
            detected_tier: Some(Tier::Potato),
            tier_override: Some(Tier::Full),
            ..Answers::default()
        };
        assert_eq!(a.effective_tier(), Some(Tier::Full));
        assert!(a.tier_was_overridden());
    }

    #[test]
    fn with_no_override_the_detected_tier_is_used() {
        let a = Answers {
            detected_tier: Some(Tier::Lite),
            ..Answers::default()
        };
        assert_eq!(a.effective_tier(), Some(Tier::Lite));
        assert!(
            !a.tier_was_overridden(),
            "agreeing with the probe is not an override"
        );
    }

    #[test]
    fn choosing_the_same_tier_the_probe_chose_is_not_an_override() {
        let a = Answers {
            detected_tier: Some(Tier::Lite),
            tier_override: Some(Tier::Lite),
            ..Answers::default()
        };
        assert!(!a.tier_was_overridden());
    }

    #[test]
    fn only_erasing_a_whole_disk_counts_as_destructive() {
        let whole = DiskPlan::WholeDisk {
            device: "/dev/sda".into(),
            encrypt: true,
        };
        let manual = DiskPlan::Manual {
            root: "/dev/sda3".into(),
        };
        assert!(whole.is_destructive());
        assert!(!manual.is_destructive());
        assert_eq!(whole.device(), "/dev/sda");
        assert_eq!(manual.device(), "/dev/sda3");
    }
}
