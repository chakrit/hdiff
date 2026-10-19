#![deny(warnings)]

use std::{env, fmt, io, process::ExitCode};

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
    document::DiffDocument,
    exit_status::DiffStatus,
    finite_output,
    input::{self, StdinState},
    parser,
    prepared::PreparedDocument,
    terminal,
};

/// Runs hdiff; a failure is reported on standard error and exits with the trouble
/// status. Rules: docs/spec/architecture.md#error-output.
fn main() -> ExitCode {
    let Err(error) = run() else {
        return ExitCode::SUCCESS;
    };

    let trouble = ExitCode::from(DiffStatus::Trouble.code());
    match report(&error) {
        Ok(()) => trouble,
        // Standard error was the last channel left; the exit status still reports
        // trouble.
        Err(_unreported) => trouble,
    }
}

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
    let clock = bench::InputClock::start();
    let content = source.read().map_err(RunError::Input)?;
    let clock = clock.input_read(&content);
    let document = content.parse().map_err(RunError::Parse)?;
    let clock = clock.parsed();

    let report = driver.run(clock, &document).map_err(RunError::Bench)?;
    write_stdout(report.as_bytes())
}

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

fn view(source: input::Source) -> Result<(), RunError> {
    let content = source.read().map_err(RunError::Input)?;
    if content.bytes().is_empty() {
        return Ok(());
    }

    let document = content.parse().map_err(RunError::Parse)?;
    render_document(document)
}

/// Draws `document`, releasing it once prepared: the prepared document keeps every
/// line it draws. Whatever keeps `/dev/tty` from opening, the output falls back to
/// finite text. Rules: docs/spec/architecture.md#input-and-terminal-boundary.
fn render_document(document: DiffDocument) -> Result<(), RunError> {
    let controlling_terminal = match terminal::OutputMode::of_stdout() {
        terminal::OutputMode::Finite => None,
        terminal::OutputMode::Interactive => terminal::open_controlling_terminal().ok(),
    };

    match controlling_terminal {
        None => write_stdout(&finite_output::of_document(&document)),
        Some(controlling_terminal) => {
            let prepared = PreparedDocument::prepare(&document);
            drop(document);
            terminal::run_interactive(controlling_terminal, prepared).map_err(RunError::Terminal)
        }
    }
}

fn write_stdout(bytes: &[u8]) -> Result<(), RunError> {
    WriteOutput { bytes }
        .run(&mut OutputContext {
            writer: &mut io::stdout(),
        })
        .map_err(RunError::Output)
}

/// Writes `error` to standard error with every line naming the program.
fn report(error: &RunError) -> Result<(), actions::Error> {
    ReportError { error }.run()
}

#[derive(Debug)]
enum RunError {
    Usage(cli::Error),
    Input(input::Error),
    Parse(parser::Error),
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
