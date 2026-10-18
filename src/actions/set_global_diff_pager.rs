use std::{ffi::OsStr, fmt, io, process::Command};

use super::global_git_config::GlobalGitConfigContext;

/// Sets `pager.diff` in the global Git config to `command`.
pub struct SetGlobalDiffPager<'a> {
    pub command: &'a str,
}

#[derive(Debug)]
pub enum Error {
    Spawn(io::Error),
    Failed { stderr: String },
}

impl SetGlobalDiffPager<'_> {
    pub fn run(self, context: &mut GlobalGitConfigContext) -> Result<(), Error> {
        let mut git = Command::new("git");
        git.args(["config", "--global", "pager.diff", self.command]);
        for (name, value) in context.environment.variables() {
            set_or_remove(&mut git, name, value);
        }

        let output = git.output().map_err(Error::Spawn)?;

        match output.status.success() {
            true => Ok(()),
            false => Err(Error::Failed {
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            }),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "cannot run git: {error}"),
            Self::Failed { stderr } => formatter.write_str(stderr.trim_end()),
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
