#![deny(warnings)]

use std::{env, fmt, fs::File, io, process::ExitCode};

use hdiff::{
    actions::{
        self,
        global_git_config::{GitConfigEnvironment, GlobalGitConfigContext},
        install_git_diff_pager::InstallGitDiffPager,
        report_error::ReportError,
        write_output::{OutputContext, WriteOutput},
    },
    bench::{self, Driver},
    cli::{self, Invocation},
    exit_status::DiffStatus,
    finite_output,
    input::{self, Opened, Origin, StdinState},
    loading::{self, Channel, Loaded, Message},
    measurement::Unobserved,
    parser,
    prepared::{Arrived, PreparedDocument, PreparedFiles, PreparedPage},
    terminal::{self, OutputMode, SessionEnd},
};

/// Runs hdiff; a failure is reported on standard error and exits with the trouble
/// status. Rules: docs/spec/architecture.md#error-output.
fn main() -> ExitCode {
    let Err(error) = run() else {
        return ExitCode::SUCCESS;
    };

    let trouble = ExitCode::from(DiffStatus::Trouble.code());
    let reported = ReportError { error: &error }.run(&mut OutputContext {
        writer: &mut io::stderr(),
    });
    match reported {
        Ok(()) => trouble,
        // Standard error was the last channel left; the exit status still reports
        // trouble.
        Err(_unreported) => trouble,
    }
}

/// Does what the command line asks.
fn run() -> Result<(), RunError> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let stdin = StdinState::inspect().map_err(RunError::Input)?;
    let invocation = cli::select_invocation(&arguments, stdin).map_err(RunError::Usage)?;

    match invocation {
        Invocation::Help => write_stdout(cli::help().as_bytes()),
        Invocation::Install => install(),
        Invocation::Version => write_stdout(cli::VERSION.as_bytes()),
        Invocation::Bench { driver, source } => bench(driver, source),
        Invocation::View { source } => view(source),
    }
}

/// Reads and parses the input like [`view`], but parses empty input too so the driver
/// reports zero counts, timing each phase for the driver's report. Rules:
/// docs/spec/performance.md#benchmark-drivers.
fn bench(driver: Driver, source: input::Source) -> Result<(), RunError> {
    let input_clock = bench::InputClock::start();
    let content = source.read().map_err(RunError::Input)?;
    let parse_clock = input_clock.input_read(&content);
    let document = content.parse().map_err(RunError::Parse)?;
    let preparation_clock = parse_clock.parsed();

    let report = driver
        .run(preparation_clock, &document)
        .map_err(RunError::Bench)?;
    write_stdout(report.as_bytes())
}

/// Registers this executable as the global Git diff pager and prints the commands it
/// applied.
fn install() -> Result<(), RunError> {
    let executable = env::current_exe().map_err(RunError::Executable)?;

    let report = InstallGitDiffPager { executable }
        .run(&mut GlobalGitConfigContext {
            environment: GitConfigEnvironment::of_process(),
        })
        .map_err(RunError::Install)?;

    let output = format!("{}\n", report.completed_commands.join("\n"));
    write_stdout(output.as_bytes())
}

/// Shows `source` on the controlling terminal as it streams in, or as finite text when
/// output is not a terminal; empty input shows nothing. Whatever keeps `/dev/tty` from
/// opening, the output falls back to finite text.
/// Rules: docs/spec/architecture.md#input-and-terminal-boundary.
fn view(source: input::Source) -> Result<(), RunError> {
    let controlling_terminal = match OutputMode::of_stdout() {
        OutputMode::Finite => None,
        OutputMode::Interactive => terminal::open_controlling_terminal().ok(),
    };

    match controlling_terminal {
        None => write_finite(source),
        Some(controlling_terminal) => stream(source, controlling_terminal),
    }
}

/// Reads and parses all of `source`, then writes it as finite text.
fn write_finite(source: input::Source) -> Result<(), RunError> {
    let content = source.read().map_err(RunError::Input)?;
    if content.bytes().is_empty() {
        return Ok(());
    }

    let document = content.parse().map_err(RunError::Parse)?;
    write_stdout(&finite_output::of_document(&document))
}

/// Reads `source` on a worker thread and opens the screen once its first file has
/// arrived; input that ends without a file never opens it. Rules:
/// docs/spec/performance.md#preparation-boundary.
fn stream(source: input::Source, controlling_terminal: File) -> Result<(), RunError> {
    let Opened { input, origin } = source.open().map_err(RunError::Input)?;
    let channel = loading::start(input);

    let document = match first_loaded(&channel) {
        Loaded::Arrived(Arrived::Opened(first) | Arrived::Whole(first)) => {
            PreparedDocument::Files(PreparedFiles::loading(first))
        }
        Loaded::Arrived(Arrived::Grew(_) | Arrived::Closed { .. } | Arrived::Ended(_)) => {
            unreachable!("the worker opens a file before it grows, closes or ends one")
        }
        Loaded::NoFiles(text) if text.is_empty() => return Ok(()),
        Loaded::NoFiles(text) => match origin {
            Origin::Stdin | Origin::PatchFile(_) => return Err(RunError::Parse(parser::NoDiff)),
            Origin::Comparison => {
                PreparedDocument::Text(PreparedPage::of_text(&text, &mut Unobserved))
            }
        },
        Loaded::Failed(error) => return Err(RunError::Input(origin.read_failure(error))),
    };

    match terminal::run_interactive(controlling_terminal, document, channel)
        .map_err(RunError::Terminal)?
    {
        SessionEnd::Quit => Ok(()),
        SessionEnd::InputFailed(error) => Err(RunError::Input(origin.read_failure(error))),
    }
}

/// What the worker read first; before the screen opens, only the worker sends.
fn first_loaded(channel: &Channel) -> Loaded {
    loop {
        match channel.receiver.recv() {
            Ok(Message::Loaded(loaded)) => return loaded,
            Ok(Message::Terminal(_)) => {}
            Err(_) => unreachable!("the channel holds a sender until the session ends"),
        }
    }
}

/// Writes `bytes` to standard output.
fn write_stdout(bytes: &[u8]) -> Result<(), RunError> {
    WriteOutput { bytes }
        .run(&mut OutputContext {
            writer: &mut io::stdout(),
        })
        .map_err(RunError::Output)
}

/// Why a run failed, from whichever step failed.
enum RunError {
    Usage(cli::Error),
    Input(input::Error),
    Parse(parser::NoDiff),
    Executable(io::Error),
    Install(actions::Error),
    Bench(io::Error),
    Output(actions::Error),
    Terminal(terminal::Error),
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(error) => write!(formatter, "{error}"),
            Self::Input(error) => write!(formatter, "{error}"),
            Self::Parse(error) => write!(formatter, "{error}"),
            Self::Executable(error) => {
                write!(formatter, "cannot locate the hdiff executable: {error}")
            }
            Self::Install(error) => write!(formatter, "--install: {error}"),
            Self::Bench(error) => write!(formatter, "--bench: {error}"),
            Self::Output(error) => write!(formatter, "standard output: {error}"),
            Self::Terminal(error) => write!(formatter, "terminal: {error}"),
        }
    }
}
