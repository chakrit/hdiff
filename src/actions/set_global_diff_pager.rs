use std::{ffi::OsStr, fmt, io, process::Command};

use super::{global_git_config::GlobalGitConfigContext, process_failure::ProcessFailure};
use crate::shell_words;

/// Sets `pager.diff` in the global Git config to `command`.
pub struct SetGlobalDiffPager<'a> {
    pub command: &'a str,
}

const GIT: &str = "git";

#[derive(Debug)]
pub enum Error {
    Spawn(io::Error),
    Failed(ProcessFailure),
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
            false => Err(Error::Failed(ProcessFailure::of_output(
                "git config",
                &output,
            ))),
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
            Self::Failed(failure) => write!(formatter, "{failure}"),
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
