use std::{
    fmt,
    process::{Command, ExitStatus, Output},
};

use super::Error;

/// Runs `command` to completion and returns its output when `accepts` admits its exit
/// status. `name` is the command as messages name it, such as `git config`.
pub fn run(
    name: &'static str,
    command: &mut Command,
    accepts: impl FnOnce(ExitStatus) -> bool,
) -> Result<Output, Error> {
    let output = command
        .output()
        .map_err(|source| Error::Spawn { name, source })?;

    match accepts(output.status) {
        true => Ok(output),
        false => Err(Error::ProcessFailed(ProcessFailure::of_output(
            name, &output,
        ))),
    }
}

/// A child process that exited with a rejected status, explained by its standard error
/// when it wrote one and otherwise by its exit status.
#[derive(Debug)]
pub struct ProcessFailure {
    /// The command as messages name it.
    name: &'static str,
    status: ExitStatus,
    stderr: String,
}

impl ProcessFailure {
    /// The failure `output` shows for the command `name` names.
    fn of_output(name: &'static str, output: &Output) -> Self {
        Self {
            name,
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

impl fmt::Display for ProcessFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.stderr.trim_end() {
            "" => write!(formatter, "{} failed: {}", self.name, self.status),
            explanation => formatter.write_str(explanation),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::run;

    #[test]
    fn a_silent_failure_names_its_command_and_exit_status() {
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
    fn a_missing_program_is_reported_by_its_command_name() {
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
