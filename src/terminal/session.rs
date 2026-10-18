//! The controlling-terminal session. Contracts: docs/spec/architecture.md#terminal-lifecycle
//! and docs/spec/architecture.md#input-and-terminal-boundary.

use std::{
    fmt,
    fs::File,
    io,
    panic::{self, PanicHookInfo},
    sync::Arc,
};

use crossterm::{
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::{TtyEvents, Tui};
use crate::{prepared::PreparedDocument, theme::ColorDepth};

const CONTROLLING_TERMINAL: &str = "/dev/tty";

/// Opens the controlling terminal for drawing.
pub fn open_controlling_terminal() -> io::Result<File> {
    File::options()
        .read(true)
        .write(true)
        .open(CONTROLLING_TERMINAL)
}

/// Runs the interactive loop on the controlling terminal, restoring it on every return path.
pub fn run_interactive(
    controlling_terminal: File,
    document: PreparedDocument,
) -> Result<(), SessionError> {
    let mut session = TerminalSession::start(controlling_terminal)?;

    let result = run_loop(session.terminal_mut(), document);

    match (result, session.finish()) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(failures)) => Err(SessionError::Restore(failures)),
        (Err(error), restored) => Err(SessionError::Run { error, restored }),
    }
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<File>>,
    document: PreparedDocument,
) -> io::Result<()> {
    let area = terminal.size()?;
    let color_depth = ColorDepth::from_color_count(crossterm::style::available_color_count());

    Tui::new(document, area, color_depth).run(terminal, TtyEvents::default())
}

/// Why an interactive session failed, with every terminal restore step that failed too.
#[derive(Debug)]
pub enum SessionError {
    /// Raw mode or the alternate screen could not be entered.
    Start {
        error: io::Error,
        restored: Result<(), RestoreFailures>,
    },
    /// The interactive loop failed.
    Run {
        error: io::Error,
        restored: Result<(), RestoreFailures>,
    },
    /// The loop finished, but the terminal was not fully restored.
    Restore(RestoreFailures),
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (error, restored) = match self {
            Self::Start { error, restored } => {
                write!(formatter, "cannot start the session: ")?;
                (error, restored)
            }
            Self::Run { error, restored } => (error, restored),
            Self::Restore(failures) => return write!(formatter, "cleanup failed: {failures}"),
        };

        write!(formatter, "{error}")?;
        match restored {
            Ok(()) => Ok(()),
            Err(failures) => write!(formatter, "; cleanup failed: {failures}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// The restore steps that failed, at least one, in the order they were attempted.
#[derive(Debug)]
pub struct RestoreFailures(Vec<(RestoreStep, io::Error)>);

impl fmt::Display for RestoreFailures {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let failures = self
            .0
            .iter()
            .map(|(step, error)| format!("{}: {error}", step.label()))
            .collect::<Vec<_>>();

        write!(formatter, "{}", failures.join("; "))
    }
}

/// One step that returns the terminal to how the session found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreStep {
    Cursor,
    AlternateScreen,
    RawMode,
}

impl RestoreStep {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::AlternateScreen => "alternate screen",
            Self::RawMode => "raw mode",
        }
    }
}

/// Success when every step succeeded; otherwise the failed steps.
fn restored(
    steps: impl IntoIterator<Item = (RestoreStep, io::Result<()>)>,
) -> Result<(), RestoreFailures> {
    let failures = steps
        .into_iter()
        .filter_map(|(step, result)| result.err().map(|error| (step, error)))
        .collect::<Vec<_>>();

    match failures.is_empty() {
        true => Ok(()),
        false => Err(RestoreFailures(failures)),
    }
}

/// Raw mode and the alternate screen on the controlling terminal, both entered, and the
/// panic hook that restores them.
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<File>>,
    panic_hook: RestoringPanicHook,
}

impl TerminalSession {
    /// Enters raw mode, then the alternate screen, leaving raw mode again when the alternate
    /// screen fails.
    fn start(controlling_terminal: File) -> Result<Self, SessionError> {
        let started = |error| SessionError::Start {
            error,
            restored: Ok(()),
        };
        let mut terminal =
            Terminal::new(CrosstermBackend::new(controlling_terminal)).map_err(started)?;
        terminal::enable_raw_mode().map_err(started)?;
        let panic_hook = RestoringPanicHook::install();

        if let Err(error) = execute!(terminal.backend_mut(), EnterAlternateScreen) {
            let restored = restored([
                (RestoreStep::Cursor, terminal.show_cursor()),
                (RestoreStep::RawMode, terminal::disable_raw_mode()),
            ]);
            panic_hook.uninstall();
            return Err(SessionError::Start { error, restored });
        }

        Ok(Self {
            terminal,
            panic_hook,
        })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<File>> {
        &mut self.terminal
    }

    /// Leaves the alternate screen, then raw mode, attempting each step whatever failed
    /// before it, and puts back the panic hook the session replaced.
    fn finish(mut self) -> Result<(), RestoreFailures> {
        let cursor = self.terminal.show_cursor();
        let screen = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let raw_mode = terminal::disable_raw_mode();
        self.panic_hook.uninstall();

        restored([
            (RestoreStep::Cursor, cursor),
            (RestoreStep::AlternateScreen, screen),
            (RestoreStep::RawMode, raw_mode),
        ])
    }
}

type PanicHook = dyn Fn(&PanicHookInfo<'_>) + Sync + Send;

/// A panic hook that restores the terminal before the panic message prints, so it stays
/// visible, then runs the hook it replaced.
struct RestoringPanicHook {
    previous: Arc<PanicHook>,
}

impl RestoringPanicHook {
    fn install() -> Self {
        let previous: Arc<PanicHook> = Arc::from(panic::take_hook());
        let chained = Arc::clone(&previous);

        panic::set_hook(Box::new(move |info| {
            if let Err(error) = restore_after_panic() {
                eprintln!("terminal restore failed: {error}");
            }
            chained(info);
        }));
        Self { previous }
    }

    /// Puts back the hook this one replaced.
    fn uninstall(self) {
        let previous = self.previous;
        panic::set_hook(Box::new(move |info| previous(info)));
    }
}

/// Best effort on a fresh handle, since the session's terminal is unreachable from a panic.
fn restore_after_panic() -> Result<(), PanicRestoreError> {
    let mut controlling_terminal = open_controlling_terminal().map_err(PanicRestoreError::Open)?;

    let restored = restored([
        (
            RestoreStep::Cursor,
            execute!(controlling_terminal, crossterm::cursor::Show),
        ),
        (
            RestoreStep::AlternateScreen,
            execute!(controlling_terminal, LeaveAlternateScreen),
        ),
        (RestoreStep::RawMode, terminal::disable_raw_mode()),
    ]);
    restored.map_err(PanicRestoreError::Restore)
}

enum PanicRestoreError {
    Open(io::Error),
    Restore(RestoreFailures),
}

impl fmt::Display for PanicRestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(error) => write!(formatter, "cannot open {CONTROLLING_TERMINAL}: {error}"),
            Self::Restore(failures) => write!(formatter, "{failures}"),
        }
    }
}
