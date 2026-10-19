use std::{
    io::{self, IsTerminal},
    time::Duration,
};

use crossterm::event::{self, Event as TerminalEvent};
use ratatui::{Terminal, backend::Backend, layout::Size};

use crate::{
    navigation::{Navigation, Step},
    preferences::ViewPreferences,
    prepared::PreparedDocument,
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
    RestoreFailures, RestoreStep, SessionError, SessionStage, open_controlling_terminal,
    run_interactive,
};

/// How the viewer writes its output. Rules:
/// docs/spec/architecture.md#input-and-terminal-boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub fn new(document: PreparedDocument, screen: Size, color_depth: ColorDepth) -> Self {
        Self {
            navigation: Navigation::start(document, ViewPreferences::INITIAL, screen),
            theme: Theme::for_depth(color_depth),
        }
    }

    pub fn preferences(&self) -> ViewPreferences {
        self.navigation.interaction().preferences()
    }

    pub fn run<B: Backend<Error = io::Error>>(
        &mut self,
        terminal: &mut Terminal<B>,
        events: impl Iterator<Item = io::Result<TerminalEvent>>,
    ) -> io::Result<()> {
        self.draw(terminal)?;

        for event in events {
            if self.step(terminal, event?)? == Step::Exit {
                break;
            }
        }
        Ok(())
    }

    pub fn step<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
        event: TerminalEvent,
    ) -> Result<Step, B::Error> {
        let Some(event) = bindings::event_for(event) else {
            return Ok(Step::Continue);
        };
        let screen = autoresize_to_frame_size(terminal)?;
        self.navigation.adopt_screen(screen);

        match self.navigation.respond(event) {
            Step::Exit => Ok(Step::Exit),
            Step::Continue => {
                self.draw(terminal)?;
                Ok(Step::Continue)
            }
        }
    }

    /// Draws the interaction, first adopting the size of the frame it draws, so bounds
    /// and drawing always read one screen size.
    pub fn draw<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), B::Error> {
        let Self { navigation, theme } = self;

        terminal
            .draw(|frame| {
                navigation.adopt_screen(frame.area().as_size());

                view::render_prepared_frame(frame, navigation, theme)
            })
            .map(|_| ())
    }
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
        let event = self.pending.take().map_or_else(event::read, Ok);

        Some(event.and_then(|event| self.coalesce_resize(event)))
    }
}

impl TtyEvents {
    fn coalesce_resize(&mut self, event: TerminalEvent) -> io::Result<TerminalEvent> {
        let TerminalEvent::Resize(..) = event else {
            return Ok(event);
        };
        let mut latest_resize = event;

        while event::poll(Duration::ZERO)? {
            let next_event = event::read()?;
            if !matches!(next_event, TerminalEvent::Resize(..)) {
                self.pending = Some(next_event);
                break;
            }
            latest_resize = next_event;
        }
        Ok(latest_resize)
    }
}
