use std::process::Command;

use super::{
    Error, child_process,
    global_git_config::{EnvironmentVariable, GlobalGitConfigContext},
};
use crate::shell_words;

/// Sets `pager.diff` in the global Git config to `command`.
pub struct SetGlobalDiffPager<'a> {
    pub command: &'a str,
}

/// The program that writes Git config, found on `PATH`.
const GIT: &str = "git";

impl SetGlobalDiffPager<'_> {
    /// The `git config` command that performs exactly this write.
    pub fn shell_command(&self) -> String {
        let [config, global, key, value] = self.arguments();

        shell_words::command_line(&[GIT, config, global, key, value])
    }

    /// Runs the write with exactly the context's Git config environment.
    pub fn run(self, context: &mut GlobalGitConfigContext) -> Result<(), Error> {
        let mut git = Command::new(GIT);
        git.args(self.arguments());
        for variable in context.environment.variables() {
            set_or_remove(&mut git, variable);
        }

        child_process::run("git config", &mut git, |status| status.success())?;

        Ok(())
    }

    /// The `git` arguments both the write and its shell spelling use.
    fn arguments(&self) -> [&str; 4] {
        ["config", "--global", "pager.diff", self.command]
    }
}

/// Gives Git exactly the environment the config path was resolved from.
fn set_or_remove(git: &mut Command, EnvironmentVariable { name, value }: EnvironmentVariable) {
    match value {
        Some(value) => git.env(name, value),
        None => git.env_remove(name),
    };
}
