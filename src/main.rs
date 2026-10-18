#![deny(warnings)]

use std::{
    env, fmt,
    io::{self, IsTerminal},
    process::ExitCode,
};

use hdiff::{
    actions::{
        git_config::{GitConfigEnvironment, InstallGitDiffPager, InstallGitDiffPagerContext},
        write_output::{OutputContext, WriteOutput},
    },
    bench,
    cli::{self, Command, UsageError},
    document::DiffDocument,
    input::{self, StdinState},
    parser::ParseError,
    prepared::PreparedDocument,
    render, terminal,
};

/// The exit status for every error, following `diff`'s status for trouble.
/// Contract: docs/spec/architecture.md#error-output.
const ERROR_STATUS: u8 = 2;

fn main() -> ExitCode {
    let Err(error) = run() else {
        return ExitCode::SUCCESS;
    };

    report(&error);
    ExitCode::from(ERROR_STATUS)
}

fn run() -> Result<(), RunError> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let stdin = StdinState::inspect()?;
    let command = cli::select_command(&arguments, stdin)?;

    match command {
        Command::Help => write_stdout(cli::HELP.as_bytes()),
        Command::Install => install(),
        Command::Version => write_stdout(cli::VERSION.as_bytes()),
        Command::Bench { driver, source } => {
            let report = driver.run(source).map_err(RunError::Bench)?;
            write_stdout(report.as_bytes())
        }
        Command::View { source } => view(source),
    }
}

fn install() -> Result<(), RunError> {
    let executable = env::current_exe().map_err(RunError::Executable)?;

    let report = InstallGitDiffPager { executable }
        .run(&mut InstallGitDiffPagerContext {
            environment: GitConfigEnvironment::of_process(),
        })
        .map_err(RunError::Install)?;

    let output = format!("{}\n", report.completed_commands.join("\n"));
    write_stdout(output.as_bytes())
}

fn view(source: input::Source) -> Result<(), RunError> {
    let content = source.read()?;
    if content.bytes().is_empty() {
        return Ok(());
    }

    let document = content.parse()?;
    render_document(&document)
}

fn render_document(document: &DiffDocument) -> Result<(), RunError> {
    let output_mode = terminal::mode_for_output(io::stdout().is_terminal());

    match output_mode {
        terminal::OutputMode::Finite => write_stdout(&render::render_unified(document)),
        terminal::OutputMode::Interactive => {
            // architecture.md (Input and terminal boundary): whatever keeps `/dev/tty` from
            // opening, the output falls back to finite text.
            let Ok(controlling_terminal) = terminal::open_controlling_terminal() else {
                return write_stdout(&render::render_unified(document));
            };
            let prepared = PreparedDocument::prepare(document);
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
fn report(error: &RunError) {
    let message = error
        .to_string()
        .lines()
        .map(|line| format!("hdiff: {line}\n"))
        .collect::<String>();

    let written = WriteOutput {
        bytes: message.as_bytes(),
    }
    .run(&mut OutputContext {
        writer: &mut io::stderr(),
    });

    // A failed error report has nowhere left to go; the exit status still reports failure.
    match written {
        Ok(()) | Err(_) => {}
    }
}

#[derive(Debug)]
enum RunError {
    Usage(UsageError),
    Input(input::Error),
    Parse(ParseError),
    Executable(io::Error),
    Install(io::Error),
    Bench(bench::Error),
    Output(io::Error),
    Terminal(io::Error),
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
            Self::Bench(error) => write!(formatter, "{error}"),
            Self::Output(error) => write!(formatter, "standard output: {error}"),
            Self::Terminal(error) => write!(formatter, "terminal: {error}"),
        }
    }
}

impl From<UsageError> for RunError {
    fn from(error: UsageError) -> Self {
        Self::Usage(error)
    }
}

impl From<input::Error> for RunError {
    fn from(error: input::Error) -> Self {
        Self::Input(error)
    }
}

impl From<ParseError> for RunError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

#[cfg(test)]
mod tests {
    use hdiff::{parser::parse_unified_diff, render::render_unified};

    #[test]
    fn turns_diff_input_into_finite_safe_output() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";

        let document = parse_unified_diff(input).expect("valid diff");
        let output = render_unified(&document);

        assert_eq!(output, input);
    }
}
