use std::{
    io::{self, IsTerminal},
    time::Duration,
};

use crossterm::event::{self, Event as TerminalEvent};
use ratatui::{Terminal, backend::Backend, layout::Size};

use crate::{
    interaction::{Interaction, Outcome},
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
    pub fn new(document: PreparedDocument, screen: Size, color_depth: ColorDepth) -> Self {
        let interaction = Interaction::start(ViewPreferences::INITIAL, screen);
        let navigation = Navigation::new(&document, &interaction);

        Self {
            document,
            interaction,
            navigation,
            theme: Theme::for_depth(color_depth),
        }
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
        let screen = next_frame_size(terminal)?;
        adopt_screen(
            &self.document,
            &mut self.interaction,
            &mut self.navigation,
            screen,
        );

        match self.interaction.respond(event, self.document.page_count()) {
            Outcome::Exit => Ok(Step::Exit),
            Outcome::Effect(effect) => {
                self.interaction = self
                    .navigation
                    .step(&self.document, &self.interaction, effect);
                self.draw(terminal)?;
                Ok(Step::Continue)
            }
        }
    }

    /// Draws the interaction, first adopting the size of the frame it draws, so bounds and
    /// drawing always read one screen size.
    pub fn draw<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), B::Error> {
        let Self {
            document,
            interaction,
            navigation,
            theme,
        } = self;

        terminal
            .draw(|frame| {
                adopt_screen(document, interaction, navigation, frame.area().as_size());

                view::render_prepared_frame(
                    frame,
                    document.file_tree(),
                    document.page(interaction.page()),
                    navigation,
                    interaction,
                    theme,
                )
            })
            .map(|_| ())
    }
}

/// The size `terminal` draws its next frame at.
fn next_frame_size<B: Backend>(terminal: &mut Terminal<B>) -> Result<Size, B::Error> {
    terminal.autoresize()?;
    Ok(terminal.get_frame().area().as_size())
}

/// Resizes `interaction` to `screen` when the screen it shows has another size.
fn adopt_screen(
    document: &PreparedDocument,
    interaction: &mut Interaction,
    navigation: &mut Navigation,
    screen: Size,
) {
    if screen == interaction.screen() {
        return;
    }
    let resized = navigation.step(document, interaction, interaction.resize(screen));

    *interaction = resized;
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
