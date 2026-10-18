use std::{
    io::{self, IsTerminal},
    time::Duration,
};

use crossterm::event::{self, Event as TerminalEvent};
use ratatui::{Terminal, backend::Backend, layout::Size};

use crate::{
    interaction::{Interaction, Transition},
    navigation::Navigation,
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
pub use session::{SessionError, open_controlling_terminal, run_interactive};

/// How the viewer writes its output. Rules: `docs/spec/architecture.md` (Input and terminal
/// boundary).
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Exit,
}

/// The interactive loop. Contract: docs/spec/architecture.md#interactive-loop.
pub struct Tui {
    document: PreparedDocument,
    interaction: Interaction,
    navigation: Navigation,
    theme: &'static Theme,
}

impl Tui {
    pub fn new(document: PreparedDocument, area: Size, color_depth: ColorDepth) -> Self {
        let interaction = Interaction::start(ViewPreferences::INITIAL, area);
        let navigation = Navigation::new(&document, &interaction);

        Self {
            document,
            interaction,
            navigation,
            theme: Theme::for_depth(color_depth),
        }
    }

    /// Replaces the document, keeping the preferences and screen and starting at its first
    /// page.
    pub fn load(&mut self, document: PreparedDocument) {
        let screen = self.interaction.display().geometry().screen().as_size();

        self.interaction = Interaction::start(self.interaction.preferences(), screen);
        self.document = document;
        self.navigation = Navigation::new(&self.document, &self.interaction);
    }

    pub fn preferences(&self) -> ViewPreferences {
        self.interaction.preferences()
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
        let keymap = self.interaction.keymap();
        let Some(event) = bindings::event_for(event).and_then(|event| event.admitted(keymap))
        else {
            return Ok(Step::Continue);
        };

        match self
            .navigation
            .step(&self.document, &self.interaction, event)
        {
            Transition::Exit => Ok(Step::Exit),
            Transition::Redraw(next) => {
                self.interaction = next;
                self.draw(terminal)?;
                Ok(Step::Continue)
            }
        }
    }

    pub fn draw<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), B::Error> {
        let page = self.document.page(self.interaction.page());
        let tree = self.document.file_tree();

        terminal
            .draw(|frame| {
                view::render_prepared_frame(
                    frame,
                    tree,
                    page,
                    self.navigation.extent(),
                    &self.interaction,
                    self.theme,
                )
            })
            .map(|_| ())
    }
}

/// Blocking terminal events with consecutive resizes coalesced into the latest one.
#[derive(Default)]
struct TtyEvents {
    pending: Option<TerminalEvent>,
}

impl Iterator for TtyEvents {
    type Item = io::Result<TerminalEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = match self.pending.take() {
            Some(event) => event,
            None => match event::read() {
                Ok(event) => event,
                Err(error) => return Some(Err(error)),
            },
        };
        Some(self.coalesce_resize(event))
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
