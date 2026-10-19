use std::{
    fmt, io,
    process::{Command, ExitStatus, Output},
};

/// Runs `command`, named `program` in messages, to completion and returns its output when
/// `accepts` admits its exit status.
pub fn run(
    program: &'static str,
    command: &mut Command,
    accepts: impl FnOnce(ExitStatus) -> bool,
) -> Result<Output, ProcessError> {
    let output = command
        .output()
        .map_err(|source| ProcessError::Spawn { program, source })?;

    match accepts(output.status) {
        true => Ok(output),
        false => Err(ProcessError::Failed(ProcessFailure::of_output(
            program, &output,
        ))),
    }
}

/// A child process that could not start or exited with a status its caller rejects.
#[derive(Debug)]
pub enum ProcessError {
    Spawn {
        program: &'static str,
        source: io::Error,
    },
    Failed(ProcessFailure),
}

impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { program, source } => write!(formatter, "cannot run {program}: {source}"),
            Self::Failed(failure) => write!(formatter, "{failure}"),
        }
    }
}

impl std::error::Error for ProcessError {}

/// A child process that exited with a rejected status, explained by its standard error
/// when it wrote one and otherwise by its exit status.
#[derive(Debug)]
pub struct ProcessFailure {
    program: &'static str,
    status: ExitStatus,
    stderr: String,
}

impl ProcessFailure {
    fn of_output(program: &'static str, output: &Output) -> Self {
        Self {
            program,
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

impl fmt::Display for ProcessFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.stderr.trim_end() {
            "" => write!(formatter, "{} failed: {}", self.program, self.status),
            explanation => formatter.write_str(explanation),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::run;

    #[test]
    fn a_silent_failure_names_its_program_and_exit_status() {
        let error = run("sh", Command::new("sh").args(["-c", "exit 3"]), |status| {
            status.success()
        })
        .expect_err("exit 3 is rejected");

        assert_eq!(error.to_string(), "sh failed: exit status: 3");
    }

    #[test]
    fn a_failure_is_explained_by_its_standard_error() {
        let error = run(
            "sh",
            Command::new("sh").args(["-c", "echo 'error: could not lock' >&2; exit 1"]),
            |status| status.success(),
        )
        .expect_err("exit 1 is rejected");

        assert_eq!(error.to_string(), "error: could not lock");
    }

    #[test]
    fn a_missing_program_names_the_program_it_could_not_run() {
        let error = run(
            "hdiff-missing",
            &mut Command::new("hdiff-definitely-missing-program"),
            |status| status.success(),
        )
        .expect_err("a missing program cannot start");

        assert!(
            error.to_string().starts_with("cannot run hdiff-missing: "),
            "{error}"
        );
    }

    #[test]
    fn an_accepted_status_returns_the_output() {
        let output = run(
            "sh",
            Command::new("sh").args(["-c", "echo out; exit 1"]),
            |status| status.code() == Some(1),
        )
        .expect("exit 1 is accepted");

        assert_eq!(output.stdout, b"out\n");
    }
}
