use std::{
    io::{self, IsTerminal},
    iter, mem,
    sync::mpsc::{Receiver, Sender},
    thread,
    time::Duration,
};

use crossterm::event::{self, Event as TerminalEvent};
use ratatui::{Terminal, backend::Backend, layout::Size};

use crate::{
    file_tree::Arrival,
    loading::{Loaded, Message},
    navigation::{Navigation, Step},
    preferences::ViewPreferences,
    prepared::{Arrived, PreparedDocument, PreparedPage},
    theme::{ColorDepth, Theme},
};

mod bindings;
mod chrome;
mod keymap_popup;
mod row_style;
mod row_writer;
mod rows;
mod session;
mod view;

pub(crate) use bindings::character_for;
pub use session::{
    Error, RestoreFailures, SessionStage, open_controlling_terminal, run_interactive,
};

/// How the viewer writes its output. Rules:
/// docs/spec/architecture.md#input-and-terminal-boundary.
pub enum OutputMode {
    Finite,
    Interactive,
}

impl OutputMode {
    /// Interactive when standard output is a terminal, finite otherwise.
    pub fn of_stdout() -> Self {
        match io::stdout().is_terminal() {
            true => Self::Interactive,
            false => Self::Finite,
        }
    }
}

/// The interactive loop. Rules: docs/spec/architecture.md#interactive-loop.
pub struct Tui {
    navigation: Navigation,
    theme: &'static Theme,
}

impl Tui {
    /// The first page of `document` in the initial preferences on a `screen`-sized
    /// terminal, drawn in the palette for `color_depth`.
    pub fn start(document: PreparedDocument, screen: Size, color_depth: ColorDepth) -> Self {
        Self {
            navigation: Navigation::start(document, ViewPreferences::INITIAL, screen),
            theme: Theme::of_depth(color_depth),
        }
    }

    /// Applies `arrived` updates to the files shown so far; the next draw shows them.
    pub fn load(&mut self, arrived: Vec<Arrived>) {
        self.navigation.load(arrived);
    }

    pub fn preferences(&self) -> ViewPreferences {
        self.navigation.interaction().preferences()
    }

    /// Draws the first frame, then answers `messages` until an event exits or every
    /// sender is gone. Each batch of waiting messages is answered in order and drawn
    /// once. Rules: docs/spec/performance.md#preparation-boundary.
    pub fn run<B: Backend<Error = io::Error>>(
        &mut self,
        terminal: &mut Terminal<B>,
        messages: &Receiver<Message>,
    ) -> io::Result<SessionEnd> {
        self.run_observed(terminal, messages, &mut Unwatched)
    }

    /// [`run`](Self::run), telling `observer` of each message it answers and each frame
    /// it draws, for the measurement drivers.
    pub fn run_observed<B: Backend<Error = io::Error>>(
        &mut self,
        terminal: &mut Terminal<B>,
        messages: &Receiver<Message>,
        observer: &mut impl RunObserver,
    ) -> io::Result<SessionEnd> {
        let mut input_failure = None;
        self.draw(terminal)?;
        observer.drew();

        while let Ok(first) = messages.recv() {
            let mut arrived = Vec::new();
            for message in iter::once(first).chain(messages.try_iter()) {
                match message {
                    Message::Terminal(event) => {
                        self.load(mem::take(&mut arrived));
                        observer.answered(Answered::Terminal);
                        if self.respond(terminal, event?)? == Step::Exit {
                            return Ok(SessionEnd::of(input_failure));
                        }
                    }
                    Message::Loaded(Loaded::Arrived(update)) => {
                        observer.answered(Answered::Arrived(update.arrival()));
                        arrived.push(update);
                    }
                    Message::Loaded(Loaded::Failed(error)) => {
                        arrived.push(Arrived::Ended(PreparedPage::empty()));
                        input_failure = Some(error);
                    }
                    Message::Loaded(Loaded::NoFiles(_)) => {
                        arrived.push(Arrived::Ended(PreparedPage::empty()));
                    }
                }
            }
            self.load(arrived);
            self.draw(terminal)?;
            observer.drew();
        }

        Ok(SessionEnd::of(input_failure))
    }

    /// Answers one terminal event at the terminal's current size, drawing the result
    /// unless it exits; an event no binding names changes nothing.
    pub fn step<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
        event: TerminalEvent,
    ) -> Result<Step, B::Error> {
        let step = self.respond(terminal, event)?;
        match step {
            Step::Exit => {}
            Step::Continue => self.draw(terminal)?,
        }
        Ok(step)
    }

    /// Answers one terminal event at the terminal's current size without drawing; an
    /// event no binding names changes nothing.
    fn respond<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
        event: TerminalEvent,
    ) -> Result<Step, B::Error> {
        let Some(session_event) = bindings::event_for(event) else {
            return Ok(Step::Continue);
        };
        let screen = autoresize_to_frame_size(terminal)?;
        self.navigation.adopt_screen(screen);

        Ok(self.navigation.respond(session_event))
    }

    /// Draws the interaction, first adopting the size of the frame it draws, so bounds
    /// and drawing always read one screen size.
    pub fn draw<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), B::Error> {
        let Self { navigation, theme } = self;

        let mut selected_line = None;
        terminal.draw(|frame| {
            navigation.adopt_screen(frame.area().as_size());

            selected_line = view::render_prepared_frame(frame, navigation, theme);
        })?;

        navigation.drew_file_list(selected_line);
        Ok(())
    }
}

/// What a run tells its observer.
pub trait RunObserver {
    /// The run answered a message.
    fn answered(&mut self, answered: Answered);
    /// The run wrote a frame, which shows every message answered before it.
    fn drew(&mut self);
}

/// A message a run answered: a terminal event, or an update from the worker.
pub enum Answered {
    Terminal,
    Arrived(Arrival),
}

/// A run observer that records nothing, for interactive sessions.
struct Unwatched;

impl RunObserver for Unwatched {
    #[inline]
    fn answered(&mut self, _: Answered) {}

    #[inline]
    fn drew(&mut self) {}
}

/// How an interactive session ended: the user left, or the input failed while it ran.
#[derive(Debug)]
pub enum SessionEnd {
    Quit,
    InputFailed(io::Error),
}

impl SessionEnd {
    fn of(input_failure: Option<io::Error>) -> Self {
        input_failure.map_or(Self::Quit, Self::InputFailed)
    }
}

/// Reads terminal events on their own thread and sends them to the main thread, until it
/// stops receiving.
pub(crate) fn read_terminal_events(sender: Sender<Message>) {
    thread::spawn(move || {
        for event in TtyEvents::default() {
            if sender.send(Message::Terminal(event)).is_err() {
                break;
            }
        }
    });
}

/// Resizes `terminal` to its backend's current size, which may clear the screen, and
/// returns the size it draws its next frame at.
fn autoresize_to_frame_size<B: Backend>(terminal: &mut Terminal<B>) -> Result<Size, B::Error> {
    terminal.autoresize()?;
    Ok(terminal.get_frame().area().as_size())
}

/// Blocking terminal events with consecutive resizes coalesced into the latest one.
#[derive(Default)]
struct TtyEvents {
    pending: Option<TerminalEvent>,
}

impl Iterator for TtyEvents {
    type Item = io::Result<TerminalEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        let next_event = self.pending.take().map_or_else(event::read, Ok);

        Some(next_event.and_then(|received| self.coalesce_resize(received)))
    }
}

impl TtyEvents {
    /// `received`, or, when it is a resize, the last of the resizes already waiting
    /// behind it; the first other event waiting is kept for the next read.
    fn coalesce_resize(&mut self, received: TerminalEvent) -> io::Result<TerminalEvent> {
        let TerminalEvent::Resize(..) = received else {
            return Ok(received);
        };
        let mut latest_resize = received;

        while event::poll(Duration::ZERO)? {
            let waiting = event::read()?;
            if !matches!(waiting, TerminalEvent::Resize(..)) {
                self.pending = Some(waiting);
                break;
            }
            latest_resize = waiting;
        }

        Ok(latest_resize)
    }
}
