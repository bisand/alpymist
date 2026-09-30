//! The fingerprint window: the view, the daemon client on its thread, and
//! Alpymist's password agent registered for this process, so polkit asks for
//! the password in `alpymist-auth`'s dialog when a finger is added or removed.

use alpymist_fingerprint::enrol::{Ask, Enrol, FINGERS};
use alpymist_fingerprint::fprint::{self, Event, Order};
use alpymist_fingerprint::view::{self, Layout, Target};
use alpymist_widget::draw::Fonts;
use alpymist_widget::host;
use alpymist_widget::window::{self, App, Key, Mods};
use alpymist_widget::{Appearance, Outcome};
use denise::Frame;
use denise::geom::{Point, Size};
use std::path::PathBuf;
use std::sync::mpsc;

/// The app id, and the one-at-a-time socket's name.
const NAME: &str = "alpymist-fingerprint";

struct Window {
    state: Enrol,
    stages: u32,
    appearance: Appearance,
    fonts: Fonts,
    layout: Option<Layout>,
    size: Size,
    scale: u32,
    hover: Option<Target>,
    orders: mpsc::Sender<Order>,
}

/// Open the window, or bring the one already open forward.
pub fn run() -> Result<(), String> {
    alpymist_widget::instance::toggle(NAME, |listener| {
        // Kept for as long as the window is open: polkit asks it for the
        // password while a finger is added or removed.
        let agent = alpymist_auth::agent::register(auth_program());
        if let Err(e) = &agent {
            eprintln!("alpymist-fingerprint: no password agent: {e}");
        }
        let (sender, events) = host::events();
        let (orders, received) = mpsc::channel();
        std::thread::spawn(move || {
            let send = move |event: Event| sender.send(event).is_ok();
            fprint::run(&received, &send);
        });
        let appearance = alpymist_widget::appearance();
        let mut fonts = Fonts::load(&appearance);
        let state = Enrol::new(None, Vec::new());
        let height = Layout::height(
            &appearance,
            &mut fonts,
            &Enrol::new(Some(String::new()), vec![String::new()]),
        );
        let width = u32::try_from(view::WIDTH).unwrap_or(560);
        let app = Window {
            state,
            stages: 5,
            appearance,
            fonts,
            layout: None,
            size: Size::new(width, height),
            scale: 1,
            hover: None,
            orders,
        };
        let options = window::Options {
            app_id: NAME.into(),
            min_size: (width, height),
            max_size: Some((width, height)),
        };
        window::run(app, &options, events, listener)?;
        drop(agent);
        Ok(false)
    })
}

/// Alpymist's password dialog, or another for trying a build.
fn auth_program() -> PathBuf {
    std::env::var_os("ALPYMIST_AUTH")
        .filter(|p| !p.is_empty())
        .map_or_else(|| PathBuf::from("/usr/bin/alpymist-auth"), PathBuf::from)
}

impl Window {
    fn relayout(&mut self) {
        self.layout = Some(Layout::new(
            &self.appearance,
            &mut self.fonts,
            &self.state,
            self.size,
            self.scale,
        ));
    }

    /// Hand what the state asks for to the daemon's thread.
    fn ask(&mut self, ask: Ask) -> Outcome {
        let order = match ask {
            Ask::Nothing => return Outcome::Unchanged,
            Ask::Enrol(finger) => Order::Enrol(finger),
            Ask::Test => Order::Test,
            Ask::Stop => Order::Stop,
            Ask::RemoveAll => Order::RemoveAll,
        };
        if self.orders.send(order).is_err() {
            self.state.refused("The fingerprint service is gone.");
        }
        self.relayout();
        Outcome::Redraw
    }

    fn act(&mut self, target: Target) -> Outcome {
        match target {
            Target::Finger(i) => {
                if self.state.choose(i) {
                    self.relayout();
                    Outcome::Redraw
                } else {
                    Outcome::Unchanged
                }
            }
            Target::Add => {
                let ask = self.state.enrol(self.stages);
                self.ask(ask)
            }
            Target::Test => {
                let ask = self.state.test();
                self.ask(ask)
            }
            Target::Remove => {
                let ask = self.state.remove();
                self.ask(ask)
            }
            Target::Cancel => {
                let ask = self.state.cancel();
                self.ask(ask)
            }
        }
    }
}

impl App for Window {
    type Event = Event;

    fn title(&self) -> String {
        "Fingerprints".into()
    }

    fn preferred_size(&self) -> (u32, u32) {
        (self.size.width, self.size.height)
    }

    fn resize(&mut self, size: Size, scale: u32) {
        self.size = size;
        self.scale = scale;
        self.relayout();
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        if self.layout.is_none() {
            self.relayout();
        }
        if let Some(layout) = &self.layout {
            view::paint(
                frame,
                layout,
                &self.appearance,
                &mut self.fonts,
                &self.state,
                self.hover,
            );
        }
    }

    fn key(&mut self, key: Key, _: Mods) -> Outcome {
        match key {
            Key::Left => {
                let i = (self.state.chosen + FINGERS.len() - 1) % FINGERS.len();
                self.act(Target::Finger(i))
            }
            Key::Right => {
                let i = (self.state.chosen + 1) % FINGERS.len();
                self.act(Target::Finger(i))
            }
            Key::Enter => self.act(Target::Add),
            Key::Chord('t') => self.act(Target::Test),
            Key::Escape => {
                let ask = self.state.cancel();
                if ask == Ask::Nothing {
                    return Outcome::Close;
                }
                self.ask(ask)
            }
            _ => Outcome::Unchanged,
        }
    }

    fn text(&mut self, _: char) -> Outcome {
        Outcome::Unchanged
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let target = at.and_then(|p| self.layout.as_ref().and_then(|l| l.hit(p)));
        if target == self.hover {
            return Outcome::Unchanged;
        }
        self.hover = target;
        Outcome::Redraw
    }

    fn press(&mut self, at: Point) -> Outcome {
        match self.layout.as_ref().and_then(|l| l.hit(at)) {
            Some(target) => self.act(target),
            None => Outcome::Unchanged,
        }
    }

    fn scroll(&mut self, _: i32) -> Outcome {
        Outcome::Unchanged
    }

    fn event(&mut self, event: Event) -> Outcome {
        match event {
            Event::Ready {
                reader,
                enrolled,
                stages,
                why,
            } => {
                self.state = Enrol::new(reader, enrolled);
                self.stages = stages;
                if let Some(why) = why {
                    self.state.refused(&why);
                }
            }
            Event::Enroll(result, done) => self.state.enroll_status(&result, done),
            Event::Verify(result, done) => self.state.verify_status(&result, done),
            Event::Removed(result) => self.state.removed(result),
            Event::Refused(why) => self.state.refused(&why),
        }
        self.relayout();
        Outcome::Redraw
    }
}
