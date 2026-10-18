//! The controlling-terminal session. Contracts: docs/spec/architecture.md#terminal-lifecycle
//! and docs/spec/architecture.md#input-and-terminal-boundary.

use std::{fs::File, io};

use crossterm::{
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::{TtyEvents, Tui};
use crate::{prepared::PreparedDocument, theme::ColorDepth};

const CONTROLLING_TERMINAL: &str = "/dev/tty";

/// Opens the controlling terminal for drawing; `None` when the process has none.
pub fn open_controlling_terminal() -> Option<File> {
    File::options()
        .read(true)
        .write(true)
        .open(CONTROLLING_TERMINAL)
        .ok()
}

/// Runs the interactive loop on the controlling terminal, restoring it on every return path.
pub fn run_interactive(controlling_terminal: File, document: PreparedDocument) -> io::Result<()> {
    let mut session = TerminalSession::start(controlling_terminal)?;

    let result = run_loop(session.terminal_mut(), document);

    combine(result, session.finish())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<File>>,
    document: PreparedDocument,
) -> io::Result<()> {
    let area = terminal.size()?;
    let color_depth = ColorDepth::from_color_count(crossterm::style::available_color_count());

    Tui::new(document, area, color_depth).run(terminal, TtyEvents::default())
}

enum Stage {
    RawMode,
    AlternateScreen,
}

/// Raw mode and the alternate screen on the controlling terminal, unwound in reverse.
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<File>>,
    stages: Vec<Stage>,
}

impl TerminalSession {
    fn start(controlling_terminal: File) -> io::Result<Self> {
        let terminal = Terminal::new(CrosstermBackend::new(controlling_terminal))?;
        let mut session = Self {
            terminal,
            stages: Vec::new(),
        };
        restore_on_panic();

        terminal::enable_raw_mode()?;
        session.stages.push(Stage::RawMode);

        let entered = execute!(session.terminal.backend_mut(), EnterAlternateScreen);
        if let Err(error) = entered {
            return Err(with_cleanup(error, session.finish()));
        }
        session.stages.push(Stage::AlternateScreen);

        Ok(session)
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<File>> {
        &mut self.terminal
    }

    /// Restores every completed stage, reporting all failures after attempting each one.
    fn finish(mut self) -> io::Result<()> {
        let cursor = self.terminal.show_cursor();
        let stages = self.stages.iter().rev().map(|stage| match stage {
            Stage::AlternateScreen => execute!(self.terminal.backend_mut(), LeaveAlternateScreen),
            Stage::RawMode => terminal::disable_raw_mode(),
        });
        let errors = std::iter::once(cursor)
            .chain(stages.collect::<Vec<_>>())
            .filter_map(Result::err)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();

        match errors.is_empty() {
            true => Ok(()),
            false => Err(io::Error::other(format!(
                "terminal cleanup failed: {}",
                errors.join("; ")
            ))),
        }
    }
}

/// Joins a session result with its cleanup result so neither failure is lost.
fn combine(result: io::Result<()>, cleanup: io::Result<()>) -> io::Result<()> {
    match result {
        Ok(()) => cleanup,
        Err(error) => Err(with_cleanup(error, cleanup)),
    }
}

fn with_cleanup(error: io::Error, cleanup: io::Result<()>) -> io::Error {
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => io::Error::other(format!("{error}; {cleanup_error}")),
    }
}

/// Best effort: restores the terminal before the panic message prints, so it stays visible.
fn restore_on_panic() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let restored = open_controlling_terminal().map(|mut controlling_terminal| {
            let cursor = execute!(controlling_terminal, crossterm::cursor::Show);
            let screen = execute!(controlling_terminal, LeaveAlternateScreen);
            [cursor, screen, terminal::disable_raw_mode()]
        });
        if let Some(results) = restored {
            results
                .into_iter()
                .filter_map(Result::err)
                .for_each(|error| eprintln!("terminal restore failed: {error}"));
        }
        hook(info);
    }));
}
