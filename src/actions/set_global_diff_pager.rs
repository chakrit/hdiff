use std::{ffi::OsStr, fmt, io, process::Command};

use super::install_git_diff_pager::InstallGitDiffPagerContext;

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
    pub fn run(self, context: &mut InstallGitDiffPagerContext) -> Result<(), Error> {
        let environment = &context.environment;
        let mut git = Command::new("git");
        git.args(["config", "--global", "pager.diff", self.command]);
        set_or_remove(
            &mut git,
            "GIT_CONFIG_GLOBAL",
            environment.git_config_global.as_deref(),
        );
        set_or_remove(&mut git, "HOME", environment.home.as_deref());
        set_or_remove(
            &mut git,
            "XDG_CONFIG_HOME",
            environment.xdg_config_home.as_deref(),
        );

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
