//! The overview on screen: a surface over the whole of the focused screen.

use alpymist_displays::{hypr, layout, screen};
use alpymist_overview::model::{Overview, Step};
use alpymist_overview::view::{self, Layout};
use alpymist_widget::draw::Fonts;
use alpymist_widget::host::{self, Placement};
use alpymist_widget::{Appearance, Key, Outcome, Widget, instance};
use denise::Frame;
use denise::geom::{Point, Rect, Size};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};

/// How long the mark takes from one workspace to the next.
const GLIDE: Duration = Duration::from_millis(140);

/// The mark on its way: where it left from, and when.
struct Moving {
    from: Rect,
    since: Instant,
}

struct App {
    overview: Overview,
    appearance: Appearance,
    fonts: Fonts,
    scale: u32,
    /// As last painted, for the pointer.
    layout: Option<Layout>,
    /// Where the mark was last painted, which is where it leaves from.
    mark: Option<Rect>,
    moving: Option<Moving>,
}

impl App {
    /// How far the mark has come, in thousandths, or that it has arrived.
    fn progress(&self) -> Option<i32> {
        let moving = self.moving.as_ref()?;
        let gone = moving.since.elapsed();
        (gone < GLIDE).then(|| {
            i32::try_from(gone.as_millis() * 1000 / GLIDE.as_millis().max(1)).unwrap_or(1000)
        })
    }

    fn act(&mut self, step: Step, glide: bool) -> Outcome {
        match step {
            Step::Nothing => Outcome::Unchanged,
            Step::Redraw => {
                if glide && let Some(from) = self.mark {
                    self.moving = Some(Moving {
                        from,
                        since: Instant::now(),
                    });
                }
                Outcome::Redraw
            }
            Step::Go(id) => {
                if let Err(e) = hypr::request(&format!("dispatch workspace {id}")) {
                    eprintln!("alpymist-overview: {e}");
                }
                Outcome::Close
            }
            Step::Close => Outcome::Close,
        }
    }
}

/// Super+Tab was pressed again.
struct Forward;

impl Widget for App {
    type Event = Forward;

    fn event(&mut self, _: Forward) -> Outcome {
        let step = self.overview.forward();
        self.act(step, true)
    }

    fn layout(&mut self, scale: u32) -> Size {
        self.scale = scale;
        // Not used: the surface is the screen.
        Size::new(1, 1)
    }

    fn paint(&mut self, frame: &mut Frame<'_>) {
        let progress = self.progress();
        if progress.is_none() {
            self.moving = None;
        }
        // Where it is going is only known once laid out, at this size.
        let from = self.moving.as_ref().map(|m| m.from);
        let size = frame.size();
        let target = Layout::new(&self.appearance, &self.overview, size, self.scale)
            .cells
            .get(self.overview.chosen)
            .copied();
        let mark = match (from, target, progress) {
            (Some(from), Some(to), Some(p)) => Some(view::between(from, to, p)),
            _ => target,
        };
        self.layout = Some(view::paint(
            frame,
            &self.appearance,
            &mut self.fonts,
            &self.overview,
            self.scale,
            mark,
        ));
        self.mark = mark;
    }

    fn key(&mut self, key: Key) -> Outcome {
        let step = self.overview.key(key);
        self.act(step, true)
    }

    fn text(&mut self, ch: char) -> Outcome {
        let step = self.overview.typed(ch);
        self.act(step, false)
    }

    fn pointer(&mut self, at: Option<Point>) -> Outcome {
        let over = at.and_then(|p| self.layout.as_ref().and_then(|l| l.cell_at(p)));
        let step = self.overview.hover(over);
        self.act(step, false)
    }

    fn press(&mut self, at: Point) -> Outcome {
        let over = self.layout.as_ref().and_then(|l| l.cell_at(at));
        let step = self.overview.press(over);
        self.act(step, false)
    }

    fn animating(&self) -> bool {
        self.moving.is_some()
    }

    fn frame_interval(&self) -> Duration {
        Duration::from_millis(16)
    }
}

/// What the surface is called, for a rule in Hyprland's configuration, and
/// what a second run finds the first by.
pub const NAME: &str = "alpymist-overview";

/// Open the overview of the screen with the focus; or, with one open, move
/// its mark on to the next workspace. Super+Tab runs this each time it is
/// pressed, and Hyprland takes that key before the overview sees it, so
/// holding Super and pressing Tab again goes through the workspaces as Tab
/// alone does.
///
/// # Errors
/// Hyprland could not be asked, or there is no Wayland session.
pub fn run() -> Result<(), String> {
    let socket = instance::socket_path(NAME);
    if let Some(path) = &socket
        && UnixStream::connect(path).is_ok()
    {
        // The one that is open heard that, and has moved on.
        return Ok(());
    }
    let listener = socket.as_ref().and_then(|path| {
        // Left by one that did not end well: nothing answered above.
        let _ = std::fs::remove_file(path);
        UnixListener::bind(path).ok()
    });

    let monitors = screen::parse(&hypr::request("j/monitors all")?)?;
    let layouts = layout::Layouts::load(&layout::path());
    let first = alpymist_displays::focused_id(1, &monitors, &layouts);
    let overview = Overview::new(&monitors, first, &hypr::request("j/clients")?)?;
    let appearance = alpymist_widget::appearance();
    let fonts = Fonts::load(&appearance);
    let options = host::Options {
        placement: Placement::FullScreen,
        output: Some(overview.screen.clone()),
        ..host::Options::new(NAME)
    };
    let app = App {
        overview,
        appearance,
        fonts,
        scale: 1,
        layout: None,
        mark: None,
        moving: None,
    };
    let (sender, events) = host::events();
    if let Some(listener) = listener {
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stream.is_err() || sender.send(Forward).is_err() {
                    break;
                }
            }
        });
    }
    // No listener of the host's: another run moves the mark, and does not
    // close what is open.
    let shown = host::run(app, &options, events, None);
    if let Some(path) = &socket {
        let _ = std::fs::remove_file(path);
    }
    shown.map(drop)
}
