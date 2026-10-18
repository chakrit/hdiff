use std::{
    fmt,
    process::{ExitStatus, Output},
};

/// A child process that exited unsuccessfully, explained by its standard error when it
/// wrote one and otherwise by its exit status.
#[derive(Debug)]
pub struct ProcessFailure {
    /// The command as messages name it, such as `diff` or `git config`.
    pub program: &'static str,
    pub status: ExitStatus,
    pub stderr: String,
}

impl ProcessFailure {
    pub fn of_output(program: &'static str, output: &Output) -> Self {
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
    use std::{os::unix::process::ExitStatusExt, process::ExitStatus};

    use super::ProcessFailure;

    #[test]
    fn a_silent_failure_names_its_program_and_exit_status() {
        let failure = ProcessFailure {
            program: "git config",
            status: ExitStatus::from_raw(3 << 8),
            stderr: String::new(),
        };

        assert_eq!(failure.to_string(), "git config failed: exit status: 3");
    }

    #[test]
    fn a_failure_is_explained_by_its_standard_error() {
        let failure = ProcessFailure {
            program: "git config",
            status: ExitStatus::from_raw(1 << 8),
            stderr: "error: could not lock config file\n".to_owned(),
        };

        assert_eq!(failure.to_string(), "error: could not lock config file");
    }
}
