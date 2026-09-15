//! The store's state, and what every key and click does to it. No pixels.
//!
//! The window shows one of four lists — new and updated applications,
//! what is installed, what has updates, a category — narrowed by the search
//! field and by a source, or one entry's page. Typing always goes to the
//! search, as in the menu: there is no field to click first.
//!
//! Anything that changes the system is a [`Command`] handed back to the
//! caller, which runs it on the source's worker and posts [`Event`]s back.
//! Several can be queued; each source carries out one at a time.

use crate::catalog::{At, Catalog, Category, Entry, Installed};
use crate::search::{self, Query};
use crate::source::Op;
use alpymist_widget::Colour;
use std::time::{Duration, Instant};

/// How a source is shown.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    /// Its name on the command line.
    pub id: String,
    /// Its name in the window.
    pub label: String,
    /// A glyph from the icon font.
    pub icon: String,
    /// Its badge's colour.
    pub colour: Colour,
    /// Whether its installed entries can be started from the store.
    pub launches: bool,
    /// Whether its entries are applications, with categories and icons,
    /// rather than packages.
    pub apps: bool,
}

/// Which list is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// New and updated applications.
    Discover,
    /// What was installed.
    Installed,
    /// What has updates.
    Updates,
    /// Applications in a category.
    Category(Category),
}

impl View {
    /// Every view, in the sidebar's order.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let mut views = vec![Self::Discover, Self::Installed, Self::Updates];
        views.extend(Category::ALL.iter().map(|&c| Self::Category(c)));
        views
    }

    /// Its title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Discover => "New & updated",
            Self::Installed => "Installed",
            Self::Updates => "Updates",
            Self::Category(c) => c.label(),
        }
    }
}

/// A button on an entry's page, or on its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Install it.
    Install,
    /// Start it.
    Open,
    /// Update it.
    Update,
    /// Remove it; pressed twice.
    Remove,
}

/// Something the pointer can be over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The search field.
    Search,
    /// A view in the sidebar.
    Nav(View),
    /// A source filter; `None` is every source.
    Chip(Option<u16>),
    /// A result row, by its position in the results.
    Row(usize),
    /// A result row's button.
    RowAction(usize, Action),
    /// Back from an entry's page.
    Back,
    /// A button on an entry's page.
    Button(Action),
    /// The entry's website.
    Link,
    /// Update everything.
    UpdateAll,
    /// Fetch every catalogue again.
    Refresh,
}

/// A key, as far as the store cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Home.
    Home,
    /// End.
    End,
    /// Tab.
    Tab,
    /// Shift+Tab.
    BackTab,
    /// Enter.
    Enter,
    /// Space.
    Space,
    /// Escape.
    Escape,
    /// Backspace.
    Backspace,
    /// Ctrl+U or Ctrl+Backspace: clear the search.
    Clear,
    /// Delete: remove the selected entry.
    Delete,
    /// Ctrl+R: refresh.
    Refresh,
    /// Ctrl+Q or Ctrl+W: close.
    Quit,
    /// Ctrl+0 to Ctrl+9: every source, or one.
    Source(u8),
}

/// Something for the caller to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Carry out an operation on a source.
    Run {
        /// Which source.
        source: u16,
        /// What to do.
        op: Op,
    },
    /// Start an installed entry.
    Launch {
        /// Which source.
        source: u16,
        /// Its id.
        id: String,
    },
    /// Open a web page.
    OpenUrl(String),
}

/// What the window should do after the store handled something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing visible changed.
    Unchanged,
    /// Paint again.
    Redraw,
    /// Do these, and paint again.
    Run(Vec<Command>),
    /// Close the window.
    Close,
}

/// What the workers send.
#[derive(Debug)]
pub enum Event {
    /// A source is doing something before it can be shown.
    Status {
        /// Which source.
        source: u16,
        /// What it is doing.
        text: String,
    },
    /// A source's catalogue was read.
    Loaded {
        /// Which source.
        source: u16,
        /// Its entries, or why there are none.
        result: Result<Vec<Entry>, String>,
    },
    /// A source said what is installed.
    Installed {
        /// Which source.
        source: u16,
        /// What is installed, or why that is not known.
        result: Result<Vec<Installed>, String>,
    },
    /// A running operation said something.
    Progress {
        /// Which source.
        source: u16,
        /// What it said.
        line: String,
    },
    /// A running operation ended.
    Finished {
        /// Which source.
        source: u16,
        /// What it was.
        op: Op,
        /// Whether it worked.
        result: Result<(), String>,
    },
}

/// Where a source's catalogue is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loading {
    /// On its way, doing this.
    Busy(String),
    /// Read.
    Ready,
    /// Not to be had, for this reason.
    Failed(String),
}

/// An operation asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// Which source.
    pub source: u16,
    /// What to do.
    pub op: Op,
    /// The entry's name, for the status line.
    pub name: String,
    /// The last thing it said.
    pub progress: String,
}

/// A line for the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// What it says.
    pub text: String,
    /// Whether something went wrong.
    pub warn: bool,
}

/// Which part of the window has the keyboard, beside the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The results.
    List,
    /// A button on an entry's page, by its position.
    Button(usize),
}

/// How long a first press of Remove waits for the second.
pub const CONFIRM: Duration = Duration::from_secs(4);

/// The whole window's state.
pub struct Store {
    /// The sources, as shown.
    pub sources: Vec<SourceInfo>,
    /// Their entries.
    pub catalog: Catalog,
    /// Where each source's catalogue is.
    pub loading: Vec<Loading>,
    /// The list showing.
    pub view: View,
    /// The entry whose page is open.
    pub detail: Option<At>,
    /// The search text.
    pub query: String,
    /// The source the results are narrowed to.
    pub filter: Option<u16>,
    /// The results, best or newest first.
    pub results: Vec<At>,
    /// How many results each source has, before narrowing to one.
    pub counts: Vec<usize>,
    /// The selected result.
    pub selected: usize,
    /// The first result on screen.
    pub scroll: usize,
    /// How many results fit, as the view last laid out.
    pub page: usize,
    /// How far down an entry's page is scrolled, in lines.
    pub detail_scroll: usize,
    /// What the pointer is over.
    pub hover: Option<Target>,
    /// Where the keyboard is.
    pub focus: Focus,
    /// Whether the focus is shown: after the keyboard moved it, not after a
    /// click.
    pub focus_visible: bool,
    /// Operations running and waiting, oldest first; each source's first is
    /// the one running.
    pub jobs: Vec<Job>,
    /// The status line.
    pub notice: Option<Notice>,
    /// Problems with the configuration, shown until something replaces them.
    pub problems: Vec<String>,
    /// The entry whose Remove was pressed once, and when.
    pub confirm: Option<(At, Instant)>,
    /// Animation frames, while something is busy.
    pub frame: u32,
    /// The results before narrowing to a source, for counting and for
    /// narrowing the next keystroke's search.
    unfiltered: Vec<At>,
    last: Option<(View, Query)>,
}

impl Store {
    /// A store over `sources`, every catalogue still loading.
    #[must_use]
    pub fn new(sources: Vec<SourceInfo>) -> Self {
        let n = sources.len();
        Self {
            catalog: Catalog::new(n),
            loading: vec![Loading::Busy("Reading the catalogue".into()); n],
            counts: vec![0; n],
            sources,
            view: View::Discover,
            detail: None,
            query: String::new(),
            filter: None,
            results: Vec::new(),
            selected: 0,
            scroll: 0,
            page: 8,
            detail_scroll: 0,
            hover: None,
            focus: Focus::List,
            focus_visible: false,
            jobs: Vec::new(),
            notice: None,
            problems: Vec::new(),
            confirm: None,
            frame: 0,
            unfiltered: Vec::new(),
            last: None,
        }
    }

    /// The selected entry, if there are results.
    #[must_use]
    pub fn selected_entry(&self) -> Option<(At, &Entry)> {
        let at = *self.results.get(self.selected)?;
        Some((at, self.catalog.get(at)?))
    }

    /// Whether anything is moving: an operation, or a catalogue loading.
    #[must_use]
    pub fn animating(&self) -> bool {
        !self.jobs.is_empty() || self.loading.iter().any(|l| matches!(l, Loading::Busy(_)))
    }

    /// The operation running on an entry, if there is one.
    #[must_use]
    pub fn job_for(&self, at: At) -> Option<&Job> {
        let entry = self.catalog.get(at)?;
        self.jobs
            .iter()
            .find(|j| j.source == at.source && j.op.target() == Some(entry.id.as_str()))
    }

    /// Whether the job is the one running on its source.
    #[must_use]
    pub fn is_running(&self, job: &Job) -> bool {
        self.jobs.iter().find(|j| j.source == job.source) == Some(job)
    }

    /// How many entries have updates.
    #[must_use]
    pub fn update_count(&self) -> usize {
        self.catalog
            .iter()
            .filter(|(_, e)| e.state.has_update())
            .count()
    }

    /// The buttons an entry has, in order.
    #[must_use]
    pub fn actions(&self, at: At) -> Vec<Action> {
        let Some(entry) = self.catalog.get(at) else {
            return Vec::new();
        };
        let launches = self
            .sources
            .get(usize::from(at.source))
            .is_some_and(|s| s.launches);
        if !entry.state.installed() {
            return vec![Action::Install];
        }
        let mut actions = Vec::new();
        if entry.state.has_update() {
            actions.push(Action::Update);
        }
        if launches {
            actions.push(Action::Open);
        }
        if !entry.protected {
            actions.push(Action::Remove);
        }
        actions
    }

    /// The button a row shows: the first of its entry's, unless that is
    /// Remove, which is only offered on the entry's page.
    #[must_use]
    pub fn row_action(&self, at: At) -> Option<Action> {
        self.actions(at)
            .first()
            .copied()
            .filter(|a| *a != Action::Remove)
    }

    /// Whether Remove has been pressed once for `at`, recently enough to
    /// count.
    #[must_use]
    pub fn confirming(&self, at: At) -> bool {
        self.confirm
            .is_some_and(|(c, when)| c == at && when.elapsed() < CONFIRM)
    }

    // ---- Results -------------------------------------------------------

    /// Work the results out again, keeping the selection where it can.
    pub fn refresh_results(&mut self) {
        let keep = self.results.get(self.selected).copied();
        let query = Query::new(&self.query);
        let narrowing = self
            .last
            .as_ref()
            .is_some_and(|(view, last)| *view == self.view && query.narrows(last));

        let catalog = &self.catalog;
        let view = self.view;
        let in_view = |e: &Entry| match view {
            View::Discover => true,
            View::Installed => matches!(
                e.state,
                crate::catalog::State::Installed { explicit: true, .. }
            ),
            View::Updates => e.state.has_update(),
            View::Category(c) => e.categories & c.bit() != 0,
        };

        self.unfiltered = if query.is_empty() {
            let mut list: Vec<At> = catalog
                .iter()
                .filter(|(_, e)| in_view(e) && !e.hidden)
                // Discover without a search shows applications only: every
                // new build of every Alpine package is not news.
                .filter(|(_, e)| view != View::Discover || e.app)
                .map(|(at, _)| at)
                .collect();
            let name = |at: &At| catalog.get(*at).map_or("", |e| e.name_lc.as_str());
            if view == View::Discover {
                let released = |at: &At| catalog.get(*at).map_or(0, |e| e.released);
                list.sort_unstable_by(|a, b| {
                    released(b).cmp(&released(a)).then(name(a).cmp(name(b)))
                });
            } else {
                list.sort_unstable_by(|a, b| name(a).cmp(name(b)).then(a.cmp(b)));
            }
            list
        } else if narrowing {
            let mut candidates = std::mem::take(&mut self.unfiltered);
            // A hidden package named in full was not among the last
            // results, having not been named in full yet.
            for (at, e) in catalog.iter() {
                if e.hidden
                    && e.name_lc == query_whole(&self.query)
                    && in_view(e)
                    && !candidates.contains(&at)
                {
                    candidates.push(at);
                }
            }
            search::rank(catalog, &query, candidates.into_iter())
        } else {
            let candidates: Vec<At> = catalog
                .iter()
                .filter(|(_, e)| in_view(e))
                .map(|(at, _)| at)
                .collect();
            search::rank(catalog, &query, candidates.into_iter())
        };

        self.counts = vec![0; self.sources.len()];
        for at in &self.unfiltered {
            if let Some(c) = self.counts.get_mut(usize::from(at.source)) {
                *c += 1;
            }
        }
        self.results = match self.filter {
            None => self.unfiltered.clone(),
            Some(s) => self
                .unfiltered
                .iter()
                .copied()
                .filter(|at| at.source == s)
                .collect(),
        };
        self.last = (!query.is_empty()).then_some((self.view, query));

        self.selected = keep
            .and_then(|k| self.results.iter().position(|&at| at == k))
            .unwrap_or(0);
        self.scroll = self.scroll.min(self.results.len().saturating_sub(1));
        self.follow_selection();
    }

    fn follow_selection(&mut self) {
        let page = self.page.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + page {
            self.scroll = self.selected + 1 - page;
        }
        let most = self.results.len().saturating_sub(page);
        self.scroll = self.scroll.min(most);
    }

    /// Set how many rows fit, as the view laid out.
    pub fn set_page(&mut self, rows: usize) {
        if rows != self.page {
            self.page = rows.max(1);
            self.follow_selection();
        }
    }

    // ---- Events ----------------------------------------------------------

    /// Something arrived from a worker.
    pub fn event(&mut self, event: Event) -> Outcome {
        match event {
            Event::Status { source, text } => {
                if let Some(l) = self.loading.get_mut(usize::from(source)) {
                    *l = Loading::Busy(text);
                }
            }
            Event::Loaded { source, result } => {
                let keep_detail = self.detail.and_then(|at| {
                    (at.source == source)
                        .then(|| self.catalog.get(at).map(|e| e.id.clone()))
                        .flatten()
                });
                let state = match result {
                    Ok(entries) => {
                        // What was known installed stays known across a
                        // reload, until the source says again.
                        let installed: Vec<Installed> = self
                            .catalog
                            .list(source)
                            .iter()
                            .filter_map(|e| match &e.state {
                                crate::catalog::State::Installed {
                                    version,
                                    explicit,
                                    update,
                                } => Some(Installed {
                                    id: e.id.clone(),
                                    version: version.clone(),
                                    explicit: *explicit,
                                    update: update.clone(),
                                }),
                                crate::catalog::State::Available => None,
                            })
                            .collect();
                        self.catalog.replace(source, entries);
                        if !installed.is_empty() {
                            self.catalog.set_installed(source, &installed);
                        }
                        Loading::Ready
                    }
                    Err(e) => Loading::Failed(e),
                };
                if let Some(l) = self.loading.get_mut(usize::from(source)) {
                    *l = state;
                }
                if let Some(id) = keep_detail {
                    self.detail = self.catalog.find(source, &id);
                }
                self.last = None;
                self.refresh_results();
            }
            Event::Installed { source, result } => match result {
                Ok(installed) => {
                    let before = self.detail.map(|at| self.actions(at));
                    self.catalog.set_installed(source, &installed);
                    // The page's buttons changed under the focus: Install
                    // became Open and Remove, and a second Enter must not
                    // land on Remove.
                    if before.is_some() && before != self.detail.map(|at| self.actions(at)) {
                        self.focus = Focus::Button(0);
                    }
                    self.last = None;
                    self.refresh_results();
                }
                Err(e) => {
                    let label = self.label(source);
                    self.notice = Some(Notice {
                        text: format!("{label}: {e}"),
                        warn: true,
                    });
                }
            },
            Event::Progress { source, line } => {
                if let Some(job) = self.jobs.iter_mut().find(|j| j.source == source) {
                    job.progress = line;
                }
            }
            Event::Finished { source, op, result } => {
                let at = self
                    .jobs
                    .iter()
                    .position(|j| j.source == source && j.op == op);
                let job = at.map(|i| self.jobs.remove(i));
                let name = job.map_or_else(|| op.target().unwrap_or("").to_owned(), |j| j.name);
                let label = self.label(source);
                self.notice = Some(match result {
                    Ok(()) => Notice {
                        text: op.done(&name, &label),
                        warn: false,
                    },
                    Err(e) => Notice {
                        text: format!("{}: {e}", op.doing(&name, &label)),
                        warn: true,
                    },
                });
            }
        }
        Outcome::Redraw
    }

    /// An animation frame passed.
    pub fn tick(&mut self) -> Outcome {
        self.frame = self.frame.wrapping_add(1);
        if let Some((_, when)) = self.confirm
            && when.elapsed() >= CONFIRM
        {
            self.confirm = None;
        }
        Outcome::redraw_if(self.animating())
    }

    fn label(&self, source: u16) -> String {
        self.sources
            .get(usize::from(source))
            .map_or_else(String::new, |s| s.label.clone())
    }

    // ---- Doing things -----------------------------------------------------

    /// Carry out `action` on `at`.
    pub fn act(&mut self, at: At, action: Action) -> Outcome {
        let Some(entry) = self.catalog.get(at) else {
            return Outcome::Unchanged;
        };
        let (id, name) = (entry.id.clone(), entry.name.clone());
        if !self.actions(at).contains(&action) {
            return Outcome::Unchanged;
        }
        let op = match action {
            Action::Open => {
                return Outcome::Run(vec![Command::Launch {
                    source: at.source,
                    id,
                }]);
            }
            Action::Install => Op::Install(id),
            Action::Update => Op::Update(Some(id)),
            Action::Remove => {
                if !self.confirming(at) {
                    self.confirm = Some((at, Instant::now()));
                    self.notice = Some(Notice {
                        text: format!("Press Remove again to remove {name}"),
                        warn: true,
                    });
                    return Outcome::Redraw;
                }
                self.confirm = None;
                Op::Remove(id)
            }
        };
        self.queue(at.source, op, name)
    }

    fn queue(&mut self, source: u16, op: Op, name: String) -> Outcome {
        if self.jobs.iter().any(|j| j.source == source && j.op == op) {
            return Outcome::Unchanged;
        }
        let label = self.label(source);
        self.jobs.push(Job {
            source,
            progress: format!("{}…", op.doing(&name, &label)),
            op: op.clone(),
            name,
        });
        self.notice = None;
        Outcome::Run(vec![Command::Run { source, op }])
    }

    /// Update everything from every source that has updates.
    pub fn update_all(&mut self) -> Outcome {
        let sources: Vec<u16> = (0..self.catalog.sources())
            .filter(|&s| self.catalog.list(s).iter().any(|e| e.state.has_update()))
            .collect();
        let mut commands = Vec::new();
        for s in sources {
            if let Outcome::Run(mut c) = self.queue(s, Op::Update(None), String::new()) {
                commands.append(&mut c);
            }
        }
        if commands.is_empty() {
            Outcome::Unchanged
        } else {
            Outcome::Run(commands)
        }
    }

    /// Fetch every catalogue again.
    pub fn refresh(&mut self) -> Outcome {
        let mut commands = Vec::new();
        for s in 0..self.catalog.sources() {
            if let Outcome::Run(mut c) = self.queue(s, Op::Refresh, String::new()) {
                commands.append(&mut c);
            }
        }
        Outcome::Run(commands)
    }

    // ---- Navigation -------------------------------------------------------

    /// Show a view.
    pub fn show(&mut self, view: View) -> Outcome {
        let changed = self.view != view || self.detail.is_some();
        self.view = view;
        self.detail = None;
        self.focus = Focus::List;
        self.results.clear();
        self.selected = 0;
        self.scroll = 0;
        self.last = None;
        self.refresh_results();
        Outcome::redraw_if(changed)
    }

    /// Narrow to a source, or show every source.
    pub fn set_filter(&mut self, filter: Option<u16>) -> Outcome {
        if filter == self.filter || filter.is_some_and(|f| usize::from(f) >= self.sources.len()) {
            return Outcome::Unchanged;
        }
        self.filter = filter;
        self.refresh_results();
        Outcome::Redraw
    }

    /// Open an entry's page.
    pub fn open(&mut self, at: At) -> Outcome {
        if self.catalog.get(at).is_none() {
            return Outcome::Unchanged;
        }
        self.detail = Some(at);
        self.detail_scroll = 0;
        self.focus = Focus::Button(0);
        self.focus_visible = false;
        Outcome::Redraw
    }

    /// Open the page of the entry `id` in the source `source`, by the
    /// source's id: what `alpymist-store show flathub org.gimp.GIMP` asks.
    pub fn open_by_id(&mut self, source: &str, id: &str) -> Outcome {
        let Some(s) = self.sources.iter().position(|s| s.id == source) else {
            return Outcome::Unchanged;
        };
        let s = u16::try_from(s).unwrap_or(u16::MAX);
        if let Some(at) = self.catalog.find(s, id) {
            return self.open(at);
        }
        // Not loaded yet, or not there: search for it instead.
        id.clone_into(&mut self.query);
        self.detail = None;
        self.refresh_results();
        Outcome::Redraw
    }

    fn back(&mut self) -> Outcome {
        if self.detail.take().is_some() {
            self.focus = Focus::List;
            self.confirm = None;
            return Outcome::Redraw;
        }
        Outcome::Unchanged
    }

    fn move_selection(&mut self, by: isize) -> Outcome {
        if self.results.is_empty() {
            return Outcome::Unchanged;
        }
        let last = self.results.len() - 1;
        let to = self.selected.saturating_add_signed(by).min(last);
        if to == self.selected {
            return Outcome::Unchanged;
        }
        self.selected = to;
        self.follow_selection();
        Outcome::Redraw
    }

    fn cycle_view(&mut self, by: isize) -> Outcome {
        let views = View::all();
        let at = views.iter().position(|v| *v == self.view).unwrap_or(0);
        let n = views.len();
        let to = (at + n).saturating_add_signed(by) % n;
        self.show(views[to])
    }

    fn cycle_filter(&mut self, by: i32) -> Outcome {
        // None, 0, 1, … as 0, 1, 2, …
        let n = i32::try_from(self.sources.len()).unwrap_or(0) + 1;
        let now = self.filter.map_or(0, |f| i32::from(f) + 1);
        let to = (now + by).rem_euclid(n);
        self.set_filter(u16::try_from(to - 1).ok())
    }

    fn scroll_detail(&mut self, by: isize) -> Outcome {
        let to = self.detail_scroll.saturating_add_signed(by);
        if to == self.detail_scroll {
            return Outcome::Unchanged;
        }
        self.detail_scroll = to;
        Outcome::Redraw
    }

    // ---- Input ------------------------------------------------------------

    /// A key was pressed.
    pub fn key(&mut self, key: Key) -> Outcome {
        match key {
            Key::Quit => return Outcome::Close,
            Key::Refresh => return self.refresh(),
            Key::Source(n) => {
                return self.set_filter(n.checked_sub(1).map(u16::from));
            }
            Key::Clear => return self.set_query(String::new()),
            _ => {}
        }
        if let Some(at) = self.detail {
            self.focus_visible = true;
            return self.detail_key(at, key);
        }
        let page = isize::try_from(self.page.max(1)).unwrap_or(1);
        match key {
            Key::Up => self.move_selection(-1),
            Key::Down => self.move_selection(1),
            Key::PageUp => self.move_selection(-page),
            Key::PageDown => self.move_selection(page),
            Key::Home => self.move_selection(isize::MIN / 2),
            Key::End => self.move_selection(isize::MAX / 2),
            Key::Left => self.cycle_filter(-1),
            Key::Right => self.cycle_filter(1),
            Key::Tab => self.cycle_view(1),
            Key::BackTab => self.cycle_view(-1),
            Key::Enter => match self.results.get(self.selected).copied() {
                Some(at) => {
                    let outcome = self.open(at);
                    self.focus_visible = true;
                    outcome
                }
                None => Outcome::Unchanged,
            },
            Key::Delete => match self.selected_entry() {
                Some((at, _)) => self.act(at, Action::Remove),
                None => Outcome::Unchanged,
            },
            Key::Backspace => {
                let mut q = self.query.clone();
                if q.pop().is_some() {
                    self.set_query(q)
                } else {
                    Outcome::Unchanged
                }
            }
            Key::Escape => {
                if !self.query.is_empty() {
                    self.set_query(String::new())
                } else if self.view != View::Discover {
                    self.show(View::Discover)
                } else {
                    Outcome::Unchanged
                }
            }
            Key::Space => self.text(' '),
            _ => Outcome::Unchanged,
        }
    }

    fn detail_key(&mut self, at: At, key: Key) -> Outcome {
        let actions = self.actions(at);
        let focused = match self.focus {
            Focus::Button(i) => i.min(actions.len().saturating_sub(1)),
            Focus::List => 0,
        };
        match key {
            Key::Escape => self.back(),
            Key::Backspace => {
                if self.query.is_empty() {
                    self.back()
                } else {
                    let mut q = self.query.clone();
                    q.pop();
                    self.set_query(q)
                }
            }
            Key::Tab | Key::Right if !actions.is_empty() => {
                self.focus = Focus::Button((focused + 1) % actions.len());
                Outcome::Redraw
            }
            Key::BackTab | Key::Left if !actions.is_empty() => {
                self.focus = Focus::Button((focused + actions.len() - 1) % actions.len());
                Outcome::Redraw
            }
            Key::Enter | Key::Space => match actions.get(focused) {
                Some(&action) => self.act(at, action),
                None => Outcome::Unchanged,
            },
            Key::Delete => self.act(at, Action::Remove),
            Key::Up => self.scroll_detail(-1),
            Key::Down => self.scroll_detail(1),
            Key::PageUp => self.scroll_detail(-10),
            Key::PageDown => self.scroll_detail(10),
            Key::Home => self.scroll_detail(isize::MIN / 2),
            _ => Outcome::Unchanged,
        }
    }

    fn set_query(&mut self, query: String) -> Outcome {
        if query == self.query && self.detail.is_none() {
            return Outcome::Unchanged;
        }
        self.query = query;
        self.detail = None;
        self.focus = Focus::List;
        // A new search starts at its best result, not at wherever the last
        // selection landed in it.
        self.results.clear();
        self.selected = 0;
        self.scroll = 0;
        self.refresh_results();
        Outcome::Redraw
    }

    /// A character was typed.
    pub fn text(&mut self, ch: char) -> Outcome {
        if ch.is_control() {
            return Outcome::Unchanged;
        }
        // A space starting a search types nothing; one on an entry's page is
        // a button press, which `key` has already had.
        if ch == ' ' && self.query.is_empty() {
            return Outcome::Unchanged;
        }
        let mut q = self.query.clone();
        q.push(ch);
        self.set_query(q)
    }

    /// The pointer is over `target`, or nothing.
    pub fn hover_over(&mut self, target: Option<Target>) -> Outcome {
        let changed = target != self.hover;
        self.hover = target;
        Outcome::redraw_if(changed)
    }

    /// `target` was clicked.
    pub fn click(&mut self, target: Target) -> Outcome {
        self.focus_visible = false;
        match target {
            Target::Search => Outcome::Unchanged,
            Target::Nav(view) => self.show(view),
            Target::Chip(filter) => self.set_filter(filter),
            Target::Row(i) => {
                self.selected = i.min(self.results.len().saturating_sub(1));
                match self.results.get(self.selected).copied() {
                    Some(at) => self.open(at),
                    None => Outcome::Unchanged,
                }
            }
            Target::RowAction(i, action) => match self.results.get(i).copied() {
                Some(at) => {
                    self.selected = i;
                    self.act(at, action)
                }
                None => Outcome::Unchanged,
            },
            Target::Back => self.back(),
            Target::Button(action) => match self.detail {
                Some(at) => {
                    if let Some(i) = self.actions(at).iter().position(|a| *a == action) {
                        self.focus = Focus::Button(i);
                    }
                    self.act(at, action)
                }
                None => Outcome::Unchanged,
            },
            Target::Link => self
                .detail
                .and_then(|at| self.catalog.get(at))
                .filter(|e| !e.homepage.is_empty())
                .map_or(Outcome::Unchanged, |e| {
                    Outcome::Run(vec![Command::OpenUrl(e.homepage.clone())])
                }),
            Target::UpdateAll => self.update_all(),
            Target::Refresh => self.refresh(),
        }
    }

    /// The wheel turned by `rows`, downwards positive.
    pub fn scroll_by(&mut self, rows: i32) -> Outcome {
        let by = isize::try_from(rows).unwrap_or(0);
        if self.detail.is_some() {
            return self.scroll_detail(by * 3);
        }
        let most = self.results.len().saturating_sub(self.page.max(1));
        let to = self.scroll.saturating_add_signed(by).min(most);
        if to == self.scroll {
            return Outcome::Unchanged;
        }
        self.scroll = to;
        // The selection stays on screen, as it would with the keyboard.
        let page = self.page.max(1);
        self.selected = self
            .selected
            .clamp(to, to + page - 1)
            .min(self.results.len().saturating_sub(1));
        Outcome::Redraw
    }
}

impl Outcome {
    /// `Redraw` when `changed`, otherwise `Unchanged`.
    #[must_use]
    pub fn redraw_if(changed: bool) -> Self {
        if changed {
            Self::Redraw
        } else {
            Self::Unchanged
        }
    }
}

fn query_whole(text: &str) -> String {
    text.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{Action, Command, Event, Key, Loading, Outcome, SourceInfo, Store, Target, View};
    use crate::catalog::{At, Category, Entry, Installed};
    use crate::source::Op;
    use alpymist_widget::Colour;

    fn source(id: &str, launches: bool) -> SourceInfo {
        SourceInfo {
            id: id.into(),
            label: id.into(),
            icon: String::new(),
            colour: Colour([0, 0, 0, 255]),
            launches,
            apps: launches,
        }
    }

    fn entry(id: &str, app: bool, released: u64, categories: u16) -> Entry {
        let mut e = Entry {
            id: id.into(),
            name: id.into(),
            summary: format!("about {id}"),
            app,
            released,
            categories,
            ..Entry::default()
        };
        e.index();
        e
    }

    fn store() -> Store {
        let mut s = Store::new(vec![source("flathub", true), source("alpine", false)]);
        s.event(Event::Loaded {
            source: 0,
            result: Ok(vec![
                entry("gimp", true, 10, Category::Graphics.bit()),
                entry("inkscape", true, 30, Category::Graphics.bit()),
                entry("firefox", true, 20, Category::Network.bit()),
            ]),
        });
        s.event(Event::Loaded {
            source: 1,
            result: Ok(vec![
                entry("zsh", false, 99, 0),
                entry("firefox-esr", false, 50, 0),
            ]),
        });
        s
    }

    fn ids(s: &Store) -> Vec<String> {
        s.results
            .iter()
            .map(|at| s.catalog.get(*at).unwrap().id.clone())
            .collect()
    }

    #[test]
    fn discover_lists_new_applications_and_no_packages() {
        let s = store();
        assert_eq!(ids(&s), ["inkscape", "firefox", "gimp"]);
        assert_eq!(s.loading, vec![Loading::Ready, Loading::Ready]);
    }

    #[test]
    fn typing_searches_every_source_and_chips_narrow_to_one() {
        let mut s = store();
        for ch in "fire".chars() {
            s.text(ch);
        }
        assert_eq!(ids(&s), ["firefox", "firefox-esr"]);
        assert_eq!(s.counts, [1, 1]);
        s.key(Key::Right);
        assert_eq!(ids(&s), ["firefox"]);
        s.key(Key::Source(2));
        assert_eq!(ids(&s), ["firefox-esr"]);
        s.key(Key::Source(0));
        s.key(Key::Down);
        s.text('f');
        assert_eq!(
            ids(&s),
            ["firefox", "firefox-esr"],
            "narrowed from the last search"
        );
        assert_eq!(s.selected, 0, "a new search starts at its best result");
        s.key(Key::Escape);
        assert!(s.query.is_empty());
    }

    #[test]
    fn categories_and_installed_are_views() {
        let mut s = store();
        s.show(View::Category(Category::Graphics));
        assert_eq!(ids(&s), ["gimp", "inkscape"]);
        s.event(Event::Installed {
            source: 1,
            result: Ok(vec![
                Installed {
                    id: "zsh".into(),
                    version: "5.9".into(),
                    explicit: true,
                    update: Some("5.9.1".into()),
                },
                Installed {
                    id: "firefox-esr".into(),
                    version: "1".into(),
                    explicit: false,
                    update: None,
                },
            ]),
        });
        s.show(View::Installed);
        assert_eq!(ids(&s), ["zsh"], "only what was asked for");
        s.show(View::Updates);
        assert_eq!(ids(&s), ["zsh"]);
        assert_eq!(s.update_count(), 1);
    }

    #[test]
    fn installing_queues_a_job_until_it_finishes() {
        let mut s = store();
        let gimp = s.catalog.find(0, "gimp").unwrap();
        let outcome = s.act(gimp, Action::Install);
        assert_eq!(
            outcome,
            Outcome::Run(vec![Command::Run {
                source: 0,
                op: Op::Install("gimp".into()),
            }])
        );
        assert!(s.animating());
        assert_eq!(
            s.act(gimp, Action::Install),
            Outcome::Unchanged,
            "not twice"
        );
        s.event(Event::Progress {
            source: 0,
            line: "Installing runtime".into(),
        });
        assert_eq!(s.job_for(gimp).unwrap().progress, "Installing runtime");
        s.event(Event::Finished {
            source: 0,
            op: Op::Install("gimp".into()),
            result: Err("no network".into()),
        });
        assert!(s.jobs.is_empty());
        let notice = s.notice.clone().unwrap();
        assert!(notice.warn && notice.text.contains("no network"));
    }

    #[test]
    fn remove_asks_twice_and_protected_packages_cannot_be_removed() {
        let mut s = store();
        s.event(Event::Installed {
            source: 0,
            result: Ok(vec![Installed {
                id: "gimp".into(),
                version: "3".into(),
                explicit: true,
                update: None,
            }]),
        });
        let gimp = s.catalog.find(0, "gimp").unwrap();
        assert_eq!(s.actions(gimp), [Action::Open, Action::Remove]);
        assert_eq!(s.row_action(gimp), Some(Action::Open));
        assert_eq!(s.act(gimp, Action::Remove), Outcome::Redraw);
        assert!(matches!(s.act(gimp, Action::Remove), Outcome::Run(_)));

        // Removed with the focus on Remove, then installed again: the focus
        // goes back to the first button, so Enter opens rather than removes.
        s.open(gimp);
        s.key(Key::Tab);
        assert_eq!(s.focus, super::Focus::Button(1));
        s.event(Event::Installed {
            source: 0,
            result: Ok(vec![]),
        });
        assert_eq!(s.focus, super::Focus::Button(0));

        let mut zsh = entry("zsh", false, 0, 0);
        zsh.protected = true;
        s.event(Event::Loaded {
            source: 1,
            result: Ok(vec![zsh]),
        });
        s.event(Event::Installed {
            source: 1,
            result: Ok(vec![Installed {
                id: "zsh".into(),
                version: "5".into(),
                explicit: true,
                update: None,
            }]),
        });
        let at = At {
            source: 1,
            index: 0,
        };
        assert!(s.actions(at).is_empty());
        assert_eq!(s.act(at, Action::Remove), Outcome::Unchanged);
    }

    #[test]
    fn an_entry_page_opens_and_closes_and_typing_leaves_it() {
        let mut s = store();
        s.key(Key::Down);
        s.key(Key::Enter);
        assert_eq!(s.detail, Some(s.results[1]));
        s.key(Key::Escape);
        assert_eq!(s.detail, None);
        s.click(Target::Row(0));
        assert!(s.detail.is_some());
        s.text('z');
        assert!(s.detail.is_none());
        assert_eq!(ids(&s), ["zsh"]);
    }

    #[test]
    fn a_reload_keeps_what_is_installed_and_the_open_page() {
        let mut s = store();
        s.event(Event::Installed {
            source: 0,
            result: Ok(vec![Installed {
                id: "firefox".into(),
                version: "1".into(),
                explicit: true,
                update: None,
            }]),
        });
        let firefox = s.catalog.find(0, "firefox").unwrap();
        s.open(firefox);
        s.event(Event::Loaded {
            source: 0,
            result: Ok(vec![entry("new", true, 1, 0), entry("firefox", true, 2, 0)]),
        });
        let moved = s.catalog.find(0, "firefox").unwrap();
        assert_eq!(s.detail, Some(moved));
        assert!(s.catalog.get(moved).unwrap().state.installed());
    }

    #[test]
    fn scrolling_keeps_the_selection_on_screen() {
        let mut s = store();
        s.set_page(2);
        assert_eq!(s.scroll_by(5), Outcome::Redraw);
        assert_eq!(s.scroll, 1);
        assert_eq!(s.selected, 1);
        s.key(Key::End);
        assert_eq!(s.selected, 2);
    }
}
