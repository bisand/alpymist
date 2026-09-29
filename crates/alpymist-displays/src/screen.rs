//! A screen as Hyprland reports it, and what it is called in a layout.
//!
//! A connector name is no name for a screen: the monitor on `DP-3` today is
//! on `DP-5` after a reboot, or on another port of the dock. Hyprland's
//! description is the screen's make, model and serial from its EDID, and its
//! monitor rules can match on it (`desc:`), so that is the screen's name —
//! except where it cannot tell two screens apart: two of the same model with
//! no serial, or a description with a comma or a semicolon, which a monitor
//! rule cannot hold. Those are named by description and connector, and follow the port.

use serde::Deserialize;

/// Built-in panels: laptop screens, named by how they are wired.
const INTERNAL: [&str; 3] = ["eDP", "LVDS", "DSI"];

/// One of `hyprctl -j monitors all`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Monitor {
    /// The connector: `eDP-1`, `DP-3`.
    pub name: String,
    /// Make, model and serial: `Samsung Electric Company S24E650 H4ZK500123`.
    pub description: String,
    /// The model alone: `S24E650`.
    pub model: String,
    /// Width in pixels, in the mode it is in.
    pub width: i32,
    /// Height in pixels.
    pub height: i32,
    /// Refresh rate in Hz.
    pub refresh_rate: f64,
    /// Where its top left corner is, in the layout's logical pixels.
    pub x: i32,
    /// See `x`.
    pub y: i32,
    /// How much larger everything is drawn.
    pub scale: f64,
    /// Rotation and flip, as `wl_output` numbers them: 0 is none, 1 is 90°.
    pub transform: u8,
    /// Whether it is off.
    pub disabled: bool,
    /// Its modes: `1920x1080@60.00Hz`.
    pub available_modes: Vec<String>,
}

impl Monitor {
    /// What it is called in a list: the laptop's own screen, or its model,
    /// or its connector when it gives no model.
    #[must_use]
    pub fn short(&self) -> String {
        if self.internal() {
            "Built-in screen".to_owned()
        } else if self.model.trim().is_empty() {
            self.name.clone()
        } else {
            self.model.trim().to_owned()
        }
    }

    /// Whether this is the machine's own panel.
    #[must_use]
    pub fn internal(&self) -> bool {
        INTERNAL.iter().any(|p| self.name.starts_with(p))
    }

    /// Its mode as a monitor rule writes it: `1920x1080@60.00`.
    #[must_use]
    pub fn mode(&self) -> String {
        format!("{}x{}@{:.2}", self.width, self.height, self.refresh_rate)
    }

    /// How wide it is in the layout: rotated a quarter turn, its height;
    /// scaled, less.
    #[must_use]
    pub fn logical_width(&self) -> i32 {
        let across = if self.transform % 2 == 1 {
            self.height
        } else {
            self.width
        };
        logical(across, self.scale)
    }

    /// How wide it will be when on: as it is, or where it is off, in the
    /// first mode it offers, which is the one it asks for.
    #[must_use]
    pub fn laid_width(&self) -> i32 {
        if self.width > 0 && !self.disabled {
            return self.logical_width();
        }
        let first = self.available_modes.first().map_or("", String::as_str);
        let size = first.split('@').next().unwrap_or("");
        let mut sides = size.split('x').filter_map(|n| n.parse::<i32>().ok());
        match (sides.next(), sides.next()) {
            (Some(w), Some(h)) => logical(
                if self.transform % 2 == 1 { h } else { w },
                self.scale.max(1.0),
            ),
            _ => 0,
        }
    }
}

/// `pixels` at `scale`, as Hyprland lays it out.
#[allow(clippy::cast_possible_truncation)] // a screen is not 2³¹ pixels wide
pub(crate) fn logical(pixels: i32, scale: f64) -> i32 {
    if scale > 0.0 {
        (f64::from(pixels) / scale).round() as i32
    } else {
        pixels
    }
}

/// What each of `monitors` is called in a layout, in the same order.
#[must_use]
pub fn names(monitors: &[Monitor]) -> Vec<String> {
    monitors
        .iter()
        .map(|m| {
            let shared = monitors
                .iter()
                .filter(|o| o.description == m.description)
                .count()
                > 1;
            if m.description.is_empty() || m.description.contains([',', ';']) || shared {
                format!("{} @ {}", m.description, m.name)
            } else {
                m.description.clone()
            }
        })
        .collect()
}

/// What a monitor rule matches a screen by: its description, or where the
/// name had to say its connector, the connector.
#[must_use]
pub fn target(name: &str, monitor: &Monitor) -> String {
    if name.ends_with(&format!(" @ {}", monitor.name)) {
        monitor.name.clone()
    } else {
        format!("desc:{}", monitor.description)
    }
}

/// `hyprctl -j monitors all`, read.
///
/// # Errors
/// It is not the list Hyprland prints.
pub fn parse(json: &[u8]) -> Result<Vec<Monitor>, String> {
    serde_json::from_slice(json).map_err(|e| format!("Hyprland's list of screens: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{Monitor, names, parse, target};

    fn m(name: &str, description: &str) -> Monitor {
        Monitor {
            name: name.into(),
            description: description.into(),
            ..Monitor::default()
        }
    }

    #[test]
    fn a_screen_is_named_by_its_edid_unless_that_cannot_tell_it_apart() {
        let screens = [
            m("eDP-1", "Chimei Innolux Corporation 0x14C9"),
            m("DP-3", "Dell Inc. U2419H"),
            m("DP-5", "Dell Inc. U2419H"),
            m("HDMI-A-1", "Maker, Inc. Screen"),
        ];
        assert_eq!(
            names(&screens),
            [
                "Chimei Innolux Corporation 0x14C9",
                "Dell Inc. U2419H @ DP-3",
                "Dell Inc. U2419H @ DP-5",
                "Maker, Inc. Screen @ HDMI-A-1",
            ]
        );
        assert_eq!(
            target(&names(&screens)[0], &screens[0]),
            "desc:Chimei Innolux Corporation 0x14C9"
        );
        assert_eq!(target(&names(&screens)[2], &screens[2]), "DP-5");
    }

    #[test]
    fn hyprlands_list_is_read() {
        let json = br#"[{"id":0,"name":"Virtual-1","description":"Red Hat Inc. QEMU Monitor",
            "make":"Red Hat, Inc.","model":"QEMU Monitor","serial":"","width":1280,"height":800,
            "refreshRate":74.994,"x":0,"y":0,"scale":1.00,"transform":0,"disabled":false,
            "availableModes":["1280x800@74.99Hz","1920x1080@60.00Hz"]}]"#;
        let list = parse(json).unwrap();
        assert_eq!(list[0].mode(), "1280x800@74.99");
        assert!(!list[0].internal());
        assert_eq!(list[0].logical_width(), 1280);
        let turned = Monitor {
            width: 2880,
            height: 1800,
            scale: 2.0,
            transform: 1,
            ..m("eDP-1", "Panel")
        };
        assert!(turned.internal());
        assert_eq!(turned.logical_width(), 900);
    }
}
