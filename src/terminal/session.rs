//! The controlling-terminal session. Rules: docs/spec/architecture.md#terminal-lifecycle
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
use crate::{
    actions::report_error::ReportError, non_empty::NonEmpty, prepared::PreparedDocument,
    theme::ColorDepth,
};

const CONTROLLING_TERMINAL: &str = "/dev/tty";

/// Opens the controlling terminal for drawing.
pub fn open_controlling_terminal() -> io::Result<File> {
    File::options()
        .read(true)
        .write(true)
        .open(CONTROLLING_TERMINAL)
}

/// Runs the interactive loop on the controlling terminal, restoring it on every return
/// path.
pub fn run_interactive(
    controlling_terminal: File,
    document: PreparedDocument,
) -> Result<(), Error> {
    let mut session = TerminalSession::start(controlling_terminal)?;

    let result = run_loop(session.terminal_mut(), document);

    match (result, session.finish()) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(failures)) => Err(Error::Restore(failures)),
        (Err(error), restored) => Err(Error::Failed {
            stage: SessionStage::Run,
            error,
            restored,
        }),
    }
}

/// Runs the interactive loop on `terminal` at its size and color depth.
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
pub enum Error {
    /// A stage of the session failed.
    Failed {
        stage: SessionStage,
        error: io::Error,
        restored: Result<(), RestoreFailures>,
    },
    /// The loop finished, but the terminal was not fully restored.
    Restore(RestoreFailures),
}

impl fmt::Display for Error {
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
            Self::Restore(failures) => write!(formatter, "{failures}"),
        }
    }
}

impl std::error::Error for Error {}

/// The part of a session that failed.
#[derive(Debug)]
pub enum SessionStage {
    /// The terminal could not be set up: its size, raw mode, or the alternate screen.
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
            Err(failures) => write!(formatter, "; {failures}"),
        }
    }
}

/// The restore steps that failed, at least one, in the order they were attempted. Shown
/// as `cleanup failed: ` and each failed step with its error.
#[derive(Debug)]
pub struct RestoreFailures(NonEmpty<RestoreFailure>);

impl fmt::Display for RestoreFailures {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let failures = self
            .0
            .iter()
            .map(|failure| format!("{}: {}", failure.step.label(), failure.error))
            .collect::<Vec<_>>();

        write!(formatter, "cleanup failed: {}", failures.join("; "))
    }
}

/// One restore step that failed, with its error.
#[derive(Debug)]
struct RestoreFailure {
    step: RestoreStep,
    error: io::Error,
}

impl RestoreFailure {
    /// The failure of `step`, when `result` is one.
    fn of(step: RestoreStep, result: io::Result<()>) -> Option<Self> {
        result.err().map(|error| Self { step, error })
    }
}

/// One step that returns the terminal to how the session found it.
#[derive(Debug, Clone, Copy)]
enum RestoreStep {
    Cursor,
    AlternateScreen,
    RawMode,
}

impl RestoreStep {
    /// The step's name in a restore failure message.
    fn label(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::AlternateScreen => "alternate screen",
            Self::RawMode => "raw mode",
        }
    }
}

/// How far a session got into the terminal modes it enters, each implying the ones
/// before.
enum Entered {
    RawMode,
    AlternateScreen,
}

/// Shows the cursor, leaves the alternate screen when it was entered, then leaves raw
/// mode, attempting each step whatever failed before it.
fn restore(terminal: &mut impl Write, entered: Entered) -> Result<(), RestoreFailures> {
    let cursor = execute!(terminal, cursor::Show);
    let left_screen = match entered {
        Entered::AlternateScreen => Some(execute!(terminal, LeaveAlternateScreen)),
        Entered::RawMode => None,
    };
    let raw_mode = terminal::disable_raw_mode();

    let failures = [
        RestoreFailure::of(RestoreStep::Cursor, cursor),
        left_screen.and_then(|result| RestoreFailure::of(RestoreStep::AlternateScreen, result)),
        RestoreFailure::of(RestoreStep::RawMode, raw_mode),
    ];
    match NonEmpty::of(failures.into_iter().flatten()) {
        None => Ok(()),
        Some(failures) => Err(RestoreFailures(failures)),
    }
}

/// Raw mode and the alternate screen on the controlling terminal, both entered, and the
/// panic hook that restores them.
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<File>>,
    panic_hook: RestoringPanicHook,
}

impl TerminalSession {
    /// Sets up the terminal at its size, enters raw mode, then the alternate screen,
    /// leaving raw mode again when the alternate screen fails.
    fn start(controlling_terminal: File) -> Result<Self, Error> {
        let mut terminal = Terminal::new(CrosstermBackend::new(controlling_terminal))
            .map_err(|error| start_failed(error, Ok(())))?;
        terminal::enable_raw_mode().map_err(|error| start_failed(error, Ok(())))?;
        let panic_hook = RestoringPanicHook::install();

        if let Err(error) = execute!(terminal.backend_mut(), EnterAlternateScreen) {
            let restored = restore(terminal.backend_mut(), Entered::RawMode);
            panic_hook.uninstall();
            return Err(start_failed(error, restored));
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

/// A session that failed to start on `error`, with whatever restoring the steps already
/// taken left behind.
fn start_failed(error: io::Error, restored: Result<(), RestoreFailures>) -> Error {
    Error::Failed {
        stage: SessionStage::Start,
        error,
        restored,
    }
}

/// A panic hook as the standard library holds one.
type PanicHook = dyn Fn(&PanicHookInfo<'_>) + Sync + Send;

/// A panic hook that restores the terminal before the panic message prints, so it stays
/// visible, then runs the hook it replaced.
struct RestoringPanicHook {
    previous: Arc<PanicHook>,
}

impl RestoringPanicHook {
    /// Replaces the current panic hook with one that restores the terminal first.
    fn install() -> Self {
        let previous: Arc<PanicHook> = Arc::from(panic::take_hook());
        let chained = Arc::clone(&previous);

        panic::set_hook(Box::new(move |info| {
            if let Err(failure) = restore_after_panic() {
                report_after_panic(&failure);
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

/// Best effort on a fresh handle, since the session's terminal is unreachable from a
/// panic.
fn restore_after_panic() -> Result<(), PanicRestoreFailure> {
    let mut controlling_terminal =
        open_controlling_terminal().map_err(PanicRestoreFailure::Reopen)?;

    restore(&mut controlling_terminal, Entered::AlternateScreen)
        .map_err(PanicRestoreFailure::Restore)
}

/// Writes `failure` to standard error as every error is written, before the panic
/// message. Rules: docs/spec/architecture.md#error-output.
fn report_after_panic(failure: &PanicRestoreFailure) {
    match (ReportError { error: failure }).run() {
        Ok(()) => {}
        // Standard error was the last channel left; the panic message still follows on
        // it, if it can.
        Err(_unreported) => {}
    }
}

/// Why a panic's restore left the terminal as the panic found it, in part or whole.
enum PanicRestoreFailure {
    /// The controlling terminal could not be reopened.
    Reopen(io::Error),
    /// Restore steps failed on the reopened terminal.
    Restore(RestoreFailures),
}

impl fmt::Display for PanicRestoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reopen(error) => write!(
                formatter,
                "cannot reopen {CONTROLLING_TERMINAL} to restore the terminal: {error}"
            ),
            Self::Restore(failures) => write!(formatter, "{failures}"),
        }
    }
}
