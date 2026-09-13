//! Turning answers into the exact commands that will be run.
//!
//! The plan is pure data. Nothing here touches a disk — it decides *what would*
//! happen, which means the whole thing can be printed, reviewed, tested, and
//! shown to the user before a single byte is written.
//!
//! The work itself is delegated to Alpine's own `setup-disk`, `setup-keymap`
//! and friends rather than reimplemented. Partitioning and bootloader
//! installation are the least forgiving code in any installer, and Alpine's
//! has been run on far more machines than ours ever will be.

use crate::answers::{Answers, DiskPlan, Network};

/// One command in the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// What to show the user while this runs.
    pub title: String,
    /// Program and arguments. Never passed through a shell.
    pub argv: Vec<String>,
    /// Environment additions for this step only.
    pub env: Vec<(String, String)>,
    /// Whether this step can destroy data.
    pub destructive: bool,
}

impl Step {
    fn new(title: &str, argv: &[&str]) -> Self {
        Self {
            title: title.to_string(),
            argv: argv.iter().map(|s| (*s).to_string()).collect(),
            env: Vec::new(),
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

    /// The step as it would be typed, for the log and the review screen.
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
        format!("{env}{}", self.argv.join(" "))
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

/// Build the plan for these answers.
///
/// # Errors
/// Returns the first reason the answers cannot be turned into a plan.
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

    let device = match disk {
        DiskPlan::WholeDisk { device, encrypt } => {
            if *encrypt {
                // setup-disk asks for the passphrase on the terminal and asks
                // again when opening the volume. Driving that by feeding stdin
                // is guesswork about call order, and guessing wrong produces an
                // encrypted disk nobody can open — a worse outcome than saying
                // so plainly here.
                return Err(PlanError::Unsupported("Disk encryption"));
            }
            device.clone()
        }
        DiskPlan::Manual { .. } => {
            return Err(PlanError::Unsupported(
                "Installing to an existing partition",
            ));
        }
    };

    let variant = a.keyboard_variant.as_deref().unwrap_or(keyboard);
    let mut steps = vec![
        // Everything before the install itself is reversible.
        Step::new("Checking the disk", &["blkid", &device]),
        // From here it is not.
        Step::new(
            "Partitioning and installing the base system",
            &["setup-disk", "-m", "sys", &device],
        )
        .with_env("ERASE_DISKS", &device)
        .with_env("BOOTLOADER", "grub")
        .destructive(),
        Step::new(
            "Setting the keyboard layout",
            &["chroot", ROOT, "setup-keymap", keyboard, variant],
        ),
        Step::new(
            "Setting the timezone",
            &["chroot", ROOT, "setup-timezone", "-z", timezone],
        ),
        Step::new(
            "Setting the hostname",
            &["chroot", ROOT, "setup-hostname", &a.hostname],
        ),
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
        Step::new(
            "Granting administrative access",
            &["chroot", ROOT, "adduser", &a.username, "wheel"],
        ),
        Step::new(
            "Installing the desktop",
            &[
                "chroot",
                ROOT,
                "apk",
                "add",
                "--no-progress",
                tier.metapackage(),
            ],
        ),
    ];

    if matches!(a.network, Some(Network::Dhcp)) {
        steps.push(Step::new(
            "Enabling networking",
            &["chroot", ROOT, "rc-update", "add", "networking", "boot"],
        ));
    }

    Ok(Plan {
        steps,
        target: device,
    })
}

#[cfg(test)]
mod tests {
    use super::{PlanError, build};
    use crate::answers::{Answers, DiskPlan, Network};
    use alpymist_core::Tier;

    fn answers() -> Answers {
        Answers {
            keyboard: Some("no".into()),
            keyboard_variant: Some("nodeadkeys".into()),
            timezone: Some("Europe/Oslo".into()),
            network: Some(Network::Dhcp),
            disk: Some(DiskPlan::WholeDisk {
                device: "/dev/sdb".into(),
                encrypt: false,
            }),
            disk_confirmed: true,
            username: "andre".into(),
            full_name: "André Biseth".into(),
            password: "a good passphrase".into(),
            password_confirm: "a good passphrase".into(),
            hostname: "alpymist".into(),
            disks: crate::disks::sample(),
            detected_tier: Some(Tier::Lite),
            tier_override: None,
        }
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
        for step in build(&answers()).unwrap().steps {
            assert!(!step.title.is_empty(), "a step with no title");
            assert!(!step.argv.is_empty(), "a step with no command");
            assert!(
                step.display().contains(&step.argv[0]),
                "display omits the program"
            );
        }
    }

    #[test]
    fn exactly_one_step_is_destructive_and_it_is_the_install_itself() {
        let plan = build(&answers()).unwrap();
        let destructive: Vec<_> = plan
            .steps
            .iter()
            .filter(|s| s.destructive)
            .map(|s| s.title.clone())
            .collect();
        assert_eq!(
            destructive.len(),
            1,
            "unexpected destructive steps: {destructive:?}"
        );
        assert!(destructive[0].contains("Partitioning"));
    }

    /// Anything before the first destructive step can still be aborted.
    #[test]
    fn nothing_destructive_happens_before_the_disk_has_been_inspected() {
        let plan = build(&answers()).unwrap();
        let point = plan.point_of_no_return().expect("there is one");
        assert!(
            point > 0,
            "the very first step must not be the destructive one"
        );
        assert!(plan.steps[..point].iter().all(|s| !s.destructive));
    }

    #[test]
    fn the_erase_is_aimed_at_the_chosen_disk_and_no_other() {
        let plan = build(&answers()).unwrap();
        let erase = plan.steps.iter().find(|s| s.destructive).unwrap();
        assert!(erase.argv.contains(&"/dev/sdb".to_string()));
        assert!(
            erase
                .env
                .iter()
                .any(|(k, v)| k == "ERASE_DISKS" && v == "/dev/sdb"),
            "ERASE_DISKS must name the target explicitly: {:?}",
            erase.env
        );
    }

    #[test]
    fn commands_are_argument_vectors_never_shell_strings() {
        // A name with a space in it must not be able to become two arguments.
        let mut a = answers();
        a.full_name = "André Biseth".into();
        let plan = build(&a).unwrap();
        let adduser = plan
            .steps
            .iter()
            .find(|s| s.title.contains("Creating"))
            .unwrap();
        assert!(
            adduser.argv.contains(&"André Biseth".to_string()),
            "the full name must be one argument: {:?}",
            adduser.argv
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
        let plan = build(&a).unwrap();
        assert!(!plan.steps.iter().any(|s| s.title.contains("networking")));
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
        ] {
            let mut a = answers();
            mutate(&mut a);
            assert_eq!(build(&a).unwrap_err(), expected);
        }
    }

    /// Better to refuse than to produce an encrypted disk nobody can open.
    #[test]
    fn encryption_is_refused_rather_than_half_done() {
        let mut a = answers();
        a.disk = Some(DiskPlan::WholeDisk {
            device: "/dev/sdb".into(),
            encrypt: true,
        });
        assert_eq!(
            build(&a).unwrap_err(),
            PlanError::Unsupported("Disk encryption")
        );
    }

    #[test]
    fn installing_onto_an_existing_partition_is_refused_for_now() {
        let mut a = answers();
        a.disk = Some(DiskPlan::Manual {
            root: "/dev/sdb3".into(),
        });
        assert!(matches!(build(&a).unwrap_err(), PlanError::Unsupported(_)));
    }

    #[test]
    fn a_plan_never_mentions_the_password() {
        let plan = build(&answers()).unwrap();
        for step in plan.steps {
            let shown = step.display();
            assert!(
                !shown.contains("a good passphrase"),
                "password leaked into {shown:?}"
            );
        }
    }
}
