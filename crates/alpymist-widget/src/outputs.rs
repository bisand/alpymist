//! The compositor's outputs, by name.
//!
//! A surface can be asked for on a particular output, but the output's object
//! arrives before its name does: a `wl_output` announces its name (`DP-3`,
//! `eDP-1`) in an event of its own, from version 4 on. This binds them on an
//! event queue of its own and waits for the names, so a caller gets both
//! without its own state having to handle outputs first.

use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::wl_output;
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::{delegate_output, delegate_registry, registry_handlers};

struct Outputs {
    registry: RegistryState,
    outputs: OutputState,
}

/// An output, as a person would pick it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// What the compositor calls it: `eDP-1`, `DP-3`.
    pub name: String,
    /// What it is, where the compositor says: `Samsung Electric Company S24C750`.
    pub description: String,
}

/// Every named output on `conn`, in the order the compositor announced them,
/// which is the order it numbers them in: Hyprland's monitor 0 comes first.
///
/// The objects belong to a queue nobody dispatches after this returns, which is
/// fine for what they are wanted for: naming an output in a request.
pub(crate) fn on(conn: &Connection) -> Result<Vec<(Screen, wl_output::WlOutput)>, String> {
    let (globals, mut queue) =
        registry_queue_init::<Outputs>(conn).map_err(|e| format!("Wayland registry: {e}"))?;
    let qh = queue.handle();
    let mut state = Outputs {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
    };
    // The first round trip binds the outputs; the second brings their names.
    for _ in 0..2 {
        queue
            .roundtrip(&mut state)
            .map_err(|e| format!("Wayland: {e}"))?;
    }
    Ok(state
        .outputs
        .outputs()
        .filter_map(|output| {
            let info = state.outputs.info(&output)?;
            // Make and model: a compositor's own description tends to add the
            // serial number and the name again.
            let made = format!("{} {}", info.make, info.model).trim().to_owned();
            let description = if made.is_empty() {
                info.description.clone().unwrap_or_default()
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
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session: {e}"))?;
    Ok(on(&conn)?.into_iter().map(|(screen, _)| screen).collect())
}

impl OutputHandler for Outputs {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ProvidesRegistryState for Outputs {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

delegate_output!(Outputs);
delegate_registry!(Outputs);
