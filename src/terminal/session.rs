//! The controlling-terminal session. Contracts: docs/spec/architecture.md#terminal-lifecycle
//! and docs/spec/architecture.md#input-and-terminal-boundary.

use std::{
    fmt,
    fs::File,
    io::{self, Write},
    panic::{self, PanicHookInfo},
    sync::Arc,
};

use crossterm::{
    cursor, execute,
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
        (Err(error), restored) => Err(SessionError::Failed {
            stage: SessionStage::Run,
            error,
            restored,
        }),
    }
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<File>>,
    document: PreparedDocument,
) -> io::Result<()> {
    let screen = terminal.size()?;
    let color_depth = ColorDepth::from_color_count(crossterm::style::available_color_count());

    Tui::new(document, screen, color_depth).run(terminal, TtyEvents::default())
}

/// Why an interactive session failed, with every terminal restore step that failed too.
#[derive(Debug)]
pub enum SessionError {
    /// A stage of the session failed.
    Failed {
        stage: SessionStage,
        error: io::Error,
        restored: Result<(), RestoreFailures>,
    },
    /// The loop finished, but the terminal was not fully restored.
    Restore(RestoreFailures),
    /// A panic's restore could not reopen the controlling terminal.
    Reopen(io::Error),
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed {
                stage: SessionStage::Start,
                error,
                restored,
            } => write!(
                formatter,
                "cannot start the session: {error}{}",
                CleanupSuffix(restored)
            ),
            Self::Failed {
                stage: SessionStage::Run,
                error,
                restored,
            } => write!(formatter, "{error}{}", CleanupSuffix(restored)),
            Self::Restore(failures) => write!(formatter, "cleanup failed: {failures}"),
            Self::Reopen(error) => write!(
                formatter,
                "cannot reopen {CONTROLLING_TERMINAL} to restore the terminal: {error}"
            ),
        }
    }
}

impl std::error::Error for SessionError {}

/// The part of a session that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStage {
    /// Raw mode or the alternate screen could not be entered.
    Start,
    /// The interactive loop failed.
    Run,
}

/// The restore failures a failed stage appends to its error, if any.
struct CleanupSuffix<'a>(&'a Result<(), RestoreFailures>);

impl fmt::Display for CleanupSuffix<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Ok(()) => Ok(()),
            Err(failures) => write!(formatter, "; cleanup failed: {failures}"),
        }
    }
}

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
    fn label(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::AlternateScreen => "alternate screen",
            Self::RawMode => "raw mode",
        }
    }
}

/// How far a session got into the terminal modes it enters, each implying the ones before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entered {
    RawMode,
    AlternateScreen,
}

/// Shows the cursor, leaves the alternate screen when it was entered, then leaves raw mode,
/// attempting each step whatever failed before it.
fn restore(terminal: &mut impl Write, entered: Entered) -> Result<(), RestoreFailures> {
    let cursor = execute!(terminal, cursor::Show);
    let screen = match entered {
        Entered::AlternateScreen => Some(execute!(terminal, LeaveAlternateScreen)),
        Entered::RawMode => None,
    };
    let raw_mode = terminal::disable_raw_mode();

    let steps = [(RestoreStep::Cursor, cursor)]
        .into_iter()
        .chain(screen.map(|screen| (RestoreStep::AlternateScreen, screen)))
        .chain([(RestoreStep::RawMode, raw_mode)]);
    let failures = steps
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
        let started = |error| SessionError::Failed {
            stage: SessionStage::Start,
            error,
            restored: Ok(()),
        };
        let mut terminal =
            Terminal::new(CrosstermBackend::new(controlling_terminal)).map_err(started)?;
        terminal::enable_raw_mode().map_err(started)?;
        let panic_hook = RestoringPanicHook::install();

        if let Err(error) = execute!(terminal.backend_mut(), EnterAlternateScreen) {
            let restored = restore(terminal.backend_mut(), Entered::RawMode);
            panic_hook.uninstall();
            return Err(SessionError::Failed {
                stage: SessionStage::Start,
                error,
                restored,
            });
        }

        Ok(Self {
            terminal,
            panic_hook,
        })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<File>> {
        &mut self.terminal
    }

    /// Restores the terminal and puts back the panic hook the session replaced.
    fn finish(mut self) -> Result<(), RestoreFailures> {
        let restored = restore(self.terminal.backend_mut(), Entered::AlternateScreen);
        self.panic_hook.uninstall();
        restored
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
                eprintln!("hdiff: {error}");
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
fn restore_after_panic() -> Result<(), SessionError> {
    let mut controlling_terminal = open_controlling_terminal().map_err(SessionError::Reopen)?;

    restore(&mut controlling_terminal, Entered::AlternateScreen).map_err(SessionError::Restore)
}
