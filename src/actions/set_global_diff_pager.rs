use std::{
    ffi::OsStr,
    fmt, io,
    process::{Command, ExitStatus},
};

use super::global_git_config::GlobalGitConfigContext;
use crate::shell_words;

/// Sets `pager.diff` in the global Git config to `command`.
pub struct SetGlobalDiffPager<'a> {
    pub command: &'a str,
}

const GIT: &str = "git";

#[derive(Debug)]
pub enum Error {
    Spawn(io::Error),
    /// `git config` exited unsuccessfully, explained by its standard error when it wrote one.
    Failed {
        status: ExitStatus,
        stderr: String,
    },
}

impl SetGlobalDiffPager<'_> {
    /// The `git config` command that performs exactly this write.
    pub fn shell_command(&self) -> String {
        let [config, global, key, value] = self.arguments();

        shell_words::command_line(&[GIT, config, global, key, value])
    }

    pub fn run(self, context: &mut GlobalGitConfigContext) -> Result<(), Error> {
        let mut git = Command::new(GIT);
        git.args(self.arguments());
        for (name, value) in context.environment.variables() {
            set_or_remove(&mut git, name, value);
        }

        let output = git.output().map_err(Error::Spawn)?;

        match output.status.success() {
            true => Ok(()),
            false => Err(Error::Failed {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            }),
        }
    }

    fn arguments(&self) -> [&str; 4] {
        ["config", "--global", "pager.diff", self.command]
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "cannot run git: {error}"),
            Self::Failed { status, stderr } => match stderr.trim_end() {
                "" => write!(formatter, "git config failed: {status}"),
                explanation => formatter.write_str(explanation),
            },
        }
    }
}

impl std::error::Error for Error {}

/// Gives Git exactly the environment the config path was resolved from.
fn set_or_remove(git: &mut Command, key: &str, value: Option<&OsStr>) {
    match value {
        Some(value) => git.env(key, value),
        None => git.env_remove(key),
    };
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    #[test]
    fn a_silent_git_failure_still_names_its_exit_status() {
        let error = Error::Failed {
            status: ExitStatus::from_raw(3 << 8),
            stderr: String::new(),
        };

        assert_eq!(error.to_string(), "git config failed: exit status: 3");
    }

    #[test]
    fn a_git_failure_is_explained_by_its_standard_error() {
        let error = Error::Failed {
            status: ExitStatus::from_raw(1 << 8),
            stderr: "error: could not lock config file\n".to_owned(),
        };

        assert_eq!(error.to_string(), "error: could not lock config file");
    }
}
