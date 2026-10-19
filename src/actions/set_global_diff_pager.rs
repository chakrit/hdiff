use std::{ffi::OsStr, process::Command};

use super::{
    child_process::{self, ProcessError},
    global_git_config::GlobalGitConfigContext,
};
use crate::shell_words;

/// Sets `pager.diff` in the global Git config to `command`.
pub struct SetGlobalDiffPager<'a> {
    pub command: &'a str,
}

const GIT: &str = "git";

impl SetGlobalDiffPager<'_> {
    /// The `git config` command that performs exactly this write.
    pub fn shell_command(&self) -> String {
        let [config, global, key, value] = self.arguments();

        shell_words::command_line(&[GIT, config, global, key, value])
    }

    pub fn run(self, context: &mut GlobalGitConfigContext) -> Result<(), ProcessError> {
        let mut git = Command::new(GIT);
        git.args(self.arguments());
        for (name, value) in context.environment.variables() {
            set_or_remove(&mut git, name, value);
        }

        child_process::run("git config", &mut git, |status| status.success())?;

        Ok(())
    }

    fn arguments(&self) -> [&str; 4] {
        ["config", "--global", "pager.diff", self.command]
    }
}

/// Gives Git exactly the environment the config path was resolved from.
fn set_or_remove(git: &mut Command, key: &str, value: Option<&OsStr>) {
    match value {
        Some(value) => git.env(key, value),
        None => git.env_remove(key),
    };
}
