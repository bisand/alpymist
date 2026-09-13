//! Turning answers into the exact commands that will be run.
//!
//! The plan is pure data. Nothing here touches a disk — it decides *what would*
//! happen, which means the whole thing can be printed, reviewed, tested, and
//! shown to the user before a single byte is written.
//!
//! # What is ours and what is Alpine's
//!
//! Bootloader, kernel, initramfs and base system are Alpine's `setup-disk`,
//! run against a root this plan has already mounted. That is the part with the
//! most hardware to get wrong, and Alpine's has been run on far more machines
//! than ours ever will be.
//!
//! Partitioning, LUKS and filesystems are done here instead, because
//! `setup-disk` only takes an encryption passphrase interactively, typed twice
//! at a terminal. Feeding that from a pipe means guessing how it reads, and
//! guessing wrong produces a disk nobody can open. `cryptsetup --key-file=-`
//! reads exactly the bytes it is given. The layout mirrors `setup-disk`'s own
//! for an encrypted system: a small unencrypted boot partition holding the
//! kernel, and everything else in one encrypted root.
//!
//! Keyboard, time zone and hostname are set on the live system first, exactly
//! as `setup-alpine` does: `setup-disk` copies the live `/etc` into the new
//! root, and the initramfs needs the keymap in place before it is built, so the
//! passphrase prompt at boot types the way the user chose.

use crate::answers::{Answers, DiskPlan, Firmware, Network};

/// Bytes a step is given on standard input.
#[derive(Clone, PartialEq, Eq)]
pub enum Input {
    /// Ordinary text, safe to show.
    Text(String),
    /// A passphrase or password. Never displayed, logged or put in `Debug`.
    Secret(String),
}

impl Input {
    /// The bytes to write.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Text(s) | Self::Secret(s) => s.as_bytes(),
        }
    }
}

impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(s) => f.debug_tuple("Text").field(s).finish(),
            Self::Secret(_) => f.write_str("Secret(<hidden>)"),
        }
    }
}

/// One command in the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// What to show the user while this runs.
    pub title: String,
    /// Program and arguments. Never passed through a shell.
    pub argv: Vec<String>,
    /// Environment additions for this step only.
    pub env: Vec<(String, String)>,
    /// What to feed the command on standard input, if anything.
    ///
    /// The only way a secret reaches a command. Arguments and environment are
    /// readable by every process on the machine through `/proc`; a pipe is not.
    pub stdin: Option<Input>,
    /// Whether this step can destroy data.
    pub destructive: bool,
}

impl Step {
    fn new(title: &str, argv: &[&str]) -> Self {
        Self {
            title: title.to_string(),
            argv: argv.iter().map(|s| (*s).to_string()).collect(),
            env: Vec::new(),
            stdin: None,
            destructive: false,
        }
    }

    fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    fn with_input(mut self, input: Input) -> Self {
        self.stdin = Some(input);
        self
    }

    /// The step as it would be typed, for the log and the review screen.
    ///
    /// Standard input is described, never shown: a partition table is not
    /// secret, but one rule for all input is a rule nobody can get wrong.
    #[must_use]
    pub fn display(&self) -> String {
        let env = self
            .env
            .iter()
            .fold(String::new(), |mut out, (key, value)| {
                out.push_str(key);
                out.push('=');
                out.push_str(value);
                out.push(' ');
                out
            });
        let input = match self.stdin {
            None => "",
            Some(Input::Text(_)) => " < (script)",
            Some(Input::Secret(_)) => " < (hidden)",
        };
        format!("{env}{}{input}", self.argv.join(" "))
    }
}

/// Everything that will be done, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The steps, in the order they run.
    pub steps: Vec<Step>,
    /// The device that will be written to.
    pub target: String,
}

impl Plan {
    /// Whether any step can destroy data.
    #[must_use]
    pub fn is_destructive(&self) -> bool {
        self.steps.iter().any(|s| s.destructive)
    }

    /// The index of the first destructive step, if there is one.
    ///
    /// Everything before it can be run and undone; from here on it cannot.
    #[must_use]
    pub fn point_of_no_return(&self) -> Option<usize> {
        self.steps.iter().position(|s| s.destructive)
    }
}

/// Why a plan could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// No disk was chosen.
    NoDisk,
    /// Something required was never answered.
    Missing(&'static str),
    /// Asked for something not yet implemented.
    Unsupported(&'static str),
}

impl PlanError {
    /// What to tell the user.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NoDisk => "No disk was chosen.".into(),
            Self::Missing(what) => format!("{what} was not answered."),
            Self::Unsupported(what) => format!("{what} is not supported yet."),
        }
    }
}

/// Where the new system is mounted while it is being built.
const ROOT: &str = "/mnt";
/// The device-mapper name of the unlocked root, which `setup-disk` expects.
const CRYPT_NAME: &str = "root";
/// Boot partition size in MiB. Holds kernels and initramfs, so not the bare
/// minimum a FAT32 EFI partition needs; matches `setup-disk`'s encrypted layout.
const BOOT_MIB: u32 = 300;

/// The `n`th partition of a disk, as the kernel names it.
///
/// Disks whose names end in a digit — `nvme0n1`, `mmcblk0` — put a `p` before
/// the partition number so `nvme0n1p1` cannot be read as disk `nvme0n11`.
#[must_use]
pub fn partition(device: &str, n: u32) -> String {
    if device.ends_with(|c: char| c.is_ascii_digit()) {
        format!("{device}p{n}")
    } else {
        format!("{device}{n}")
    }
}

/// Build the plan for these answers.
///
/// # Errors
/// Returns the first reason the answers cannot be turned into a plan.
#[allow(clippy::too_many_lines)]
pub fn build(a: &Answers) -> Result<Plan, PlanError> {
    let Some(disk) = a.disk.as_ref() else {
        return Err(PlanError::NoDisk);
    };
    let keyboard = a
        .keyboard
        .as_deref()
        .ok_or(PlanError::Missing("The keyboard layout"))?;
    let timezone = a
        .timezone
        .as_deref()
        .ok_or(PlanError::Missing("The timezone"))?;
    let tier = a
        .effective_tier()
        .ok_or(PlanError::Missing("The desktop"))?;

    let (device, encrypt) = match disk {
        DiskPlan::WholeDisk { device, encrypt } => (device.clone(), *encrypt),
        DiskPlan::Manual { .. } => {
            return Err(PlanError::Unsupported(
                "Installing to an existing partition",
            ));
        }
    };
    if encrypt && a.passphrase.is_empty() {
        return Err(PlanError::Missing("The disk passphrase"));
    }

    let variant = a.keyboard_variant.as_deref().unwrap_or(keyboard);
    let boot = partition(&device, 1);
    let system = partition(&device, 2);
    let root = if encrypt {
        format!("/dev/mapper/{CRYPT_NAME}")
    } else {
        system.clone()
    };
    let boot_mount = format!("{ROOT}/boot");

    let (table, boot_fs, mkfs_boot): (String, &str, Vec<&str>) = match a.firmware {
        Firmware::Uefi => (
            format!("label: gpt\nsize={BOOT_MIB}MiB, type=U\ntype=L\n"),
            "vfat",
            vec!["mkfs.vfat", "-F", "32", &boot],
        ),
        Firmware::Bios => (
            format!("label: dos\nsize={BOOT_MIB}MiB, type=83, bootable\ntype=83\n"),
            "ext4",
            // pv-grub and older GRUB cannot read 64-bit ext4; setup-disk agrees.
            vec!["mkfs.ext4", "-q", "-O", "^64bit", &boot],
        ),
    };

    let mut steps = vec![
        // Nothing here touches the target disk.
        Step::new("Checking the disk", &["test", "-b", &device]),
        Step::new(
            "Setting the keyboard layout",
            &["setup-keymap", keyboard, variant],
        ),
        Step::new("Setting the time zone", &["setup-timezone", "-z", timezone]),
        Step::new("Setting the hostname", &["setup-hostname", &a.hostname]),
        Step::new(
            "Preparing the disk tools",
            &[
                "apk",
                "add",
                "--quiet",
                "--no-progress",
                "sfdisk",
                "e2fsprogs",
                "dosfstools",
                "cryptsetup",
            ],
        ),
        // From here on it does.
        Step::new(
            "Partitioning the disk",
            &[
                "sfdisk",
                "--quiet",
                "--wipe",
                "always",
                "--wipe-partitions",
                "always",
                &device,
            ],
        )
        .with_input(Input::Text(table))
        .destructive(),
        // mdev creates the new partition nodes; setup-disk does the same.
        Step::new("Waiting for the new partitions", &["mdev", "-s"]),
        Step::new("Formatting the boot partition", &mkfs_boot).destructive(),
    ];

    if encrypt {
        steps.push(
            Step::new(
                "Encrypting the disk",
                &[
                    "cryptsetup",
                    "luksFormat",
                    "--batch-mode",
                    "--type",
                    "luks2",
                    "--key-file=-",
                    &system,
                ],
            )
            .with_input(Input::Secret(a.passphrase.clone()))
            .destructive(),
        );
        steps.push(
            Step::new(
                "Unlocking the encrypted disk",
                &["cryptsetup", "open", "--key-file=-", &system, CRYPT_NAME],
            )
            .with_input(Input::Secret(a.passphrase.clone())),
        );
    }

    steps.extend([
        Step::new(
            "Formatting the system partition",
            &["mkfs.ext4", "-q", &root],
        )
        .destructive(),
        Step::new(
            "Mounting the new system",
            &["mount", "-t", "ext4", &root, ROOT],
        ),
        Step::new("Making room for the kernel", &["mkdir", "-p", &boot_mount]),
        Step::new(
            "Mounting the boot partition",
            &["mount", "-t", boot_fs, &boot, &boot_mount],
        ),
        Step::new(
            "Installing the base system and bootloader",
            &["setup-disk", "-m", "sys", ROOT],
        )
        .with_env("BOOTLOADER", "grub"),
        Step::new(
            "Creating your account",
            &[
                "chroot",
                ROOT,
                "adduser",
                "-D",
                "-g",
                &a.full_name,
                &a.username,
            ],
        ),
    ]);
    // Without a password the account stays locked, as `adduser -D` leaves it.
    // Feeding chpasswd an empty one would instead allow logging in with none.
    if !a.password.is_empty() {
        steps.push(
            Step::new(
                "Setting your password",
                &["chroot", ROOT, "chpasswd", "-c", "sha512"],
            )
            .with_input(Input::Secret(format!("{}:{}\n", a.username, a.password))),
        );
    }
    steps.push(Step::new(
        "Granting administrative access",
        &["chroot", ROOT, "adduser", &a.username, "wheel"],
    ));

    if matches!(a.network, Some(Network::Dhcp)) {
        steps.push(Step::new(
            "Enabling networking",
            &["chroot", ROOT, "rc-update", "add", "networking", "boot"],
        ));
    }

    steps.push(Step::new(
        "Installing the desktop",
        &[
            "apk",
            "add",
            "--root",
            ROOT,
            "--no-progress",
            tier.metapackage(),
        ],
    ));

    steps.push(Step::new(
        "Unmounting the boot partition",
        &["umount", &boot_mount],
    ));
    steps.push(Step::new("Unmounting the new system", &["umount", ROOT]));
    if encrypt {
        steps.push(Step::new(
            "Locking the encrypted disk",
            &["cryptsetup", "close", CRYPT_NAME],
        ));
    }

    Ok(Plan {
        steps,
        target: device,
    })
}

#[cfg(test)]
mod tests {
    use super::{Input, PlanError, build, partition};
    use crate::answers::{Answers, DiskPlan, Firmware, Network};
    use alpymist_core::Tier;

    const PASSWORD: &str = "a good password";
    const PASSPHRASE: &str = "correct horse battery staple";

    fn answers() -> Answers {
        Answers {
            keyboard: Some("no".into()),
            keyboard_variant: Some("no-nodeadkeys".into()),
            timezone: Some("Europe/Oslo".into()),
            network: Some(Network::Dhcp),
            disk: Some(DiskPlan::WholeDisk {
                device: "/dev/sdb".into(),
                encrypt: false,
            }),
            disk_confirmed: true,
            username: "andre".into(),
            full_name: "André Biseth".into(),
            password: PASSWORD.into(),
            password_confirm: PASSWORD.into(),
            hostname: "alpymist".into(),
            disks: crate::disks::sample(),
            detected_tier: Some(Tier::Lite),
            ..Answers::default()
        }
    }

    fn encrypted() -> Answers {
        let mut a = answers();
        a.disk = Some(DiskPlan::WholeDisk {
            device: "/dev/sdb".into(),
            encrypt: true,
        });
        a.passphrase = PASSPHRASE.into();
        a.passphrase_confirm = PASSPHRASE.into();
        a
    }

    fn titles(a: &Answers) -> Vec<String> {
        build(a)
            .unwrap()
            .steps
            .into_iter()
            .map(|s| s.title)
            .collect()
    }

    fn step<'a>(plan: &'a super::Plan, title: &str) -> &'a super::Step {
        plan.steps
            .iter()
            .find(|s| s.title.contains(title))
            .unwrap_or_else(|| panic!("no step titled {title:?}"))
    }

    #[test]
    fn a_complete_set_of_answers_produces_a_plan() {
        let plan = build(&answers()).expect("should plan");
        assert_eq!(plan.target, "/dev/sdb");
        assert!(plan.steps.len() >= 8);
    }

    /// The user must be able to see exactly what will run before it runs.
    #[test]
    fn every_step_says_what_it_is_and_what_it_runs() {
        for a in [answers(), encrypted()] {
            for step in build(&a).unwrap().steps {
                assert!(!step.title.is_empty(), "a step with no title");
                assert!(!step.argv.is_empty(), "a step with no command");
                assert!(
                    step.display().contains(&step.argv[0]),
                    "display omits the program"
                );
            }
        }
    }

    /// The safety re-check runs before the first destructive step, so that step
    /// has to be the first thing that touches the disk.
    #[test]
    fn partitioning_is_the_first_destructive_step_and_comes_after_inspection() {
        for a in [answers(), encrypted()] {
            let plan = build(&a).unwrap();
            let point = plan.point_of_no_return().expect("there is one");
            assert!(point > 0, "the very first step must not be destructive");
            assert!(plan.steps[point].title.contains("Partitioning"));
            assert!(plan.steps[..point].iter().all(|s| !s.destructive));
        }
    }

    /// Every step that writes to a disk writes to the chosen one.
    #[test]
    fn every_destructive_step_is_aimed_at_the_chosen_disk() {
        for a in [answers(), encrypted()] {
            let plan = build(&a).unwrap();
            for s in plan.steps.iter().filter(|s| s.destructive) {
                let target = s.argv.last().unwrap();
                assert!(
                    target.starts_with("/dev/sdb") || target == "/dev/mapper/root",
                    "{} writes to {target}",
                    s.title
                );
            }
        }
    }

    #[test]
    fn keyboard_and_time_are_set_before_the_system_is_copied() {
        let t = titles(&encrypted());
        let at = |name: &str| t.iter().position(|s| s.contains(name)).unwrap();
        // setup-disk copies the live /etc and builds the initramfs, which must
        // already carry the keymap for the passphrase prompt at boot.
        assert!(at("keyboard") < at("base system"));
        assert!(at("time zone") < at("base system"));
        assert!(at("hostname") < at("base system"));
    }

    #[test]
    fn an_encrypted_install_formats_the_unlocked_volume_not_the_partition() {
        let plan = build(&encrypted()).unwrap();
        assert_eq!(step(&plan, "Encrypting").argv.last().unwrap(), "/dev/sdb2");
        assert_eq!(
            step(&plan, "Formatting the system").argv.last().unwrap(),
            "/dev/mapper/root"
        );
        assert!(
            step(&plan, "Mounting the new system")
                .argv
                .contains(&"/dev/mapper/root".into())
        );
        let t = titles(&encrypted());
        assert!(
            t.iter().position(|s| s.contains("Unlocking"))
                < t.iter().position(|s| s.contains("Formatting the system"))
        );
        assert_eq!(t.last().unwrap(), "Locking the encrypted disk");
    }

    #[test]
    fn an_unencrypted_install_never_runs_cryptsetup() {
        let plan = build(&answers()).unwrap();
        assert!(!plan.steps.iter().any(|s| s.argv[0] == "cryptsetup"));
        assert_eq!(
            step(&plan, "Formatting the system").argv.last().unwrap(),
            "/dev/sdb2"
        );
    }

    /// Arguments and environment are world-readable through /proc; stdin is not.
    #[test]
    fn secrets_only_ever_travel_on_standard_input() {
        let plan = build(&encrypted()).unwrap();
        for s in &plan.steps {
            let visible = format!("{} {:?} {:?}", s.display(), s.argv, s.env);
            for secret in [PASSWORD, PASSPHRASE] {
                assert!(
                    !visible.contains(secret),
                    "{secret:?} leaked in {}",
                    s.title
                );
            }
            assert!(
                !format!("{s:?}").contains(PASSPHRASE),
                "Debug leaked the passphrase"
            );
            assert!(
                !format!("{s:?}").contains(PASSWORD),
                "Debug leaked the password"
            );
        }
        let fed: Vec<&Input> = plan.steps.iter().filter_map(|s| s.stdin.as_ref()).collect();
        assert!(
            fed.iter()
                .any(|i| matches!(i, Input::Secret(p) if p == PASSPHRASE))
        );
        assert!(
            fed.iter()
                .any(|i| matches!(i, Input::Secret(p) if p.contains(PASSWORD)))
        );
        assert!(!format!("{:?}", encrypted()).contains(PASSPHRASE));
    }

    #[test]
    fn the_passphrase_is_exactly_what_was_typed() {
        // No trailing newline: at boot the prompt reads up to Enter, and a key
        // file with a newline in it is a different passphrase.
        let plan = build(&encrypted()).unwrap();
        for s in plan
            .steps
            .iter()
            .filter(|s| s.argv[0] == "cryptsetup" && s.stdin.is_some())
        {
            assert_eq!(s.stdin.as_ref().unwrap().bytes(), PASSPHRASE.as_bytes());
        }
    }

    #[test]
    fn the_partition_table_follows_the_firmware() {
        let mut a = answers();
        a.firmware = Firmware::Uefi;
        let plan = build(&a).unwrap();
        let Some(Input::Text(table)) = &step(&plan, "Partitioning").stdin else {
            panic!("no table");
        };
        assert!(table.contains("label: gpt") && table.contains("type=U"));
        assert!(
            step(&plan, "boot partition")
                .argv
                .contains(&"mkfs.vfat".into())
        );

        a.firmware = Firmware::Bios;
        let plan = build(&a).unwrap();
        let Some(Input::Text(table)) = &step(&plan, "Partitioning").stdin else {
            panic!("no table");
        };
        assert!(table.contains("label: dos") && table.contains("bootable"));
    }

    #[test]
    fn partitions_are_named_the_way_the_kernel_names_them() {
        assert_eq!(partition("/dev/sda", 2), "/dev/sda2");
        assert_eq!(partition("/dev/vda", 1), "/dev/vda1");
        assert_eq!(partition("/dev/nvme0n1", 2), "/dev/nvme0n1p2");
        assert_eq!(partition("/dev/mmcblk0", 1), "/dev/mmcblk0p1");
    }

    #[test]
    fn commands_are_argument_vectors_never_shell_strings() {
        // A name with a space in it must not be able to become two arguments.
        let plan = build(&answers()).unwrap();
        assert!(
            step(&plan, "Creating")
                .argv
                .contains(&"André Biseth".to_string()),
        );
    }

    #[test]
    fn the_chosen_desktop_tier_is_the_package_installed() {
        for (tier, package) in [
            (Tier::Full, "alpymist-desktop-full"),
            (Tier::Potato, "alpymist-desktop-lite"),
            (Tier::Legacy, "alpymist-desktop-legacy"),
        ] {
            let mut a = answers();
            a.tier_override = Some(tier);
            let plan = build(&a).unwrap();
            assert!(
                plan.steps
                    .iter()
                    .any(|s| s.argv.contains(&package.to_string())),
                "{tier:?} should install {package}"
            );
        }
    }

    #[test]
    fn an_offline_install_does_not_enable_networking() {
        let mut a = answers();
        a.network = Some(Network::Offline);
        assert!(!titles(&a).iter().any(|s| s.contains("networking")));
    }

    #[test]
    fn missing_answers_are_reported_rather_than_guessed() {
        for (mutate, expected) in [
            (
                Box::new(|a: &mut Answers| a.disk = None) as Box<dyn Fn(&mut Answers)>,
                PlanError::NoDisk,
            ),
            (
                Box::new(|a: &mut Answers| a.keyboard = None),
                PlanError::Missing("The keyboard layout"),
            ),
            (
                Box::new(|a: &mut Answers| a.timezone = None),
                PlanError::Missing("The timezone"),
            ),
            (
                Box::new(|a: &mut Answers| a.passphrase.clear()),
                PlanError::Missing("The disk passphrase"),
            ),
        ] {
            let mut a = encrypted();
            mutate(&mut a);
            assert_eq!(build(&a).unwrap_err(), expected);
        }
    }

    /// An empty password must leave the account locked, not open.
    #[test]
    fn no_password_means_no_chpasswd() {
        let mut a = answers();
        a.password.clear();
        assert!(!titles(&a).iter().any(|t| t.contains("password")));
        assert!(titles(&answers()).iter().any(|t| t.contains("password")));
    }

    #[test]
    fn installing_onto_an_existing_partition_is_refused_for_now() {
        let mut a = answers();
        a.disk = Some(DiskPlan::Manual {
            root: "/dev/sdb3".into(),
        });
        assert!(matches!(build(&a).unwrap_err(), PlanError::Unsupported(_)));
    }
}
