//! The compositor's outputs, by name.
//!
//! A surface can be asked for on a particular output, but the output's object
//! arrives before its name does: a `wl_output` announces its name (`DP-3`,
//! `eDP-1`) in an event of its own, from version 4 on. This waits for the
//! names, so a caller gets both.

use alpymist_wayland::{Output, Wayland};

/// An output, as a person would pick it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// What the compositor calls it: `eDP-1`, `DP-3`.
    pub name: String,
    /// What it is, where the compositor says: `Samsung Electric Company S24C750`.
    pub description: String,
}

/// Every named output, in the order the compositor announced them, which is
/// the order it numbers them in: Hyprland's monitor 0 comes first.
///
/// What else the compositor said while it was asked stays queued for the
/// caller's own loop.
pub(crate) fn on(wayland: &mut Wayland) -> Result<Vec<(Screen, Output)>, String> {
    // The outputs were bound on connecting; one round trip brings their names.
    wayland.roundtrip()?;
    Ok(wayland
        .outputs()
        .into_iter()
        .filter_map(|output| {
            let info = wayland.info(&output)?;
            // Make and model: a compositor's own description tends to add the
            // serial number and the name again.
            let made = format!("{} {}", info.make, info.model).trim().to_owned();
            let description = if made.is_empty() {
                info.description.unwrap_or_default()
            } else {
                made
            };
            Some((
                Screen {
                    name: info.name?,
                    description,
                },
                output,
            ))
        })
        .collect())
}

/// The compositor's outputs, first first.
///
/// Empty where the compositor does not name them, which is a `wl_output` older
/// than version 4.
///
/// # Errors
/// No Wayland session.
pub fn screens() -> Result<Vec<Screen>, String> {
    let mut wayland = Wayland::connect()?;
    Ok(on(&mut wayland)?
        .into_iter()
        .map(|(screen, _)| screen)
        .collect())
}
