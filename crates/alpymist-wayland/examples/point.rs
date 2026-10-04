//! A pointer for a compositor that has none: what the headless tests move
//! and click with.
//!
//! `point WIDTH HEIGHT`, then lines on standard input: `at X Y`, `press`,
//! `release`, `wheel NOTCHES`, `scroll PIXELS`. It stays until its input
//! ends, because a virtual pointer goes with the client that made it, and a
//! seat whose only pointer has gone has no pointer.

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::BufRead;
    use wayland_client::protocol::{wl_pointer, wl_registry};
    use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
    use wayland_protocols_wlr::virtual_pointer::v1::client::{
        zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
        zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
    };

    struct Nothing;
    impl Dispatch<wl_registry::WlRegistry, wayland_client::globals::GlobalListContents> for Nothing {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &wayland_client::globals::GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
    delegate_noop!(Nothing: ignore ZwlrVirtualPointerManagerV1);
    delegate_noop!(Nothing: ignore ZwlrVirtualPointerV1);

    const BTN_LEFT: u32 = 0x110;
    let mut args = std::env::args().skip(1);
    let width: u32 = args.next().ok_or("point WIDTH HEIGHT")?.parse()?;
    let height: u32 = args.next().ok_or("point WIDTH HEIGHT")?.parse()?;

    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = wayland_client::globals::registry_queue_init::<Nothing>(&conn)?;
    let manager: ZwlrVirtualPointerManagerV1 = globals.bind(&queue.handle(), 1..=2, ())?;
    let pointer = manager.create_virtual_pointer(None, &queue.handle(), ());
    queue.roundtrip(&mut Nothing)?;

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["at", x, y] => pointer.motion_absolute(0, x.parse()?, y.parse()?, width, height),
            ["press"] => pointer.button(0, BTN_LEFT, wl_pointer::ButtonState::Pressed),
            ["release"] => pointer.button(0, BTN_LEFT, wl_pointer::ButtonState::Released),
            ["wheel", notches] => {
                let notches: i32 = notches.parse()?;
                pointer.axis_source(wl_pointer::AxisSource::Wheel);
                pointer.axis_discrete(
                    0,
                    wl_pointer::Axis::VerticalScroll,
                    f64::from(notches) * 15.0,
                    notches,
                );
            }
            ["scroll", pixels] => {
                pointer.axis_source(wl_pointer::AxisSource::Finger);
                pointer.axis(0, wl_pointer::Axis::VerticalScroll, pixels.parse()?);
            }
            _ => return Err(format!("not understood: {line}").into()),
        }
        pointer.frame();
        queue.roundtrip(&mut Nothing)?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {}
