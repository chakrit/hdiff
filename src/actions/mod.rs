use std::{fmt, io, path::PathBuf};

use self::{
    backup_global_git_config::BackupGlobalGitConfig, child_process::ProcessFailure,
    install_git_diff_pager::PathRole,
};

pub mod backup_global_git_config;
pub mod child_process;
pub mod compare_files;
pub mod global_git_config;
pub mod install_git_diff_pager;
pub mod set_global_diff_pager;
pub mod write_output;

/// Why a unit of work failed.
#[derive(Debug)]
pub enum Error {
    /// A child process could not start; `name` is the command as messages name it.
    Spawn {
        name: &'static str,
        source: io::Error,
    },
    /// A child process exited with a status its caller rejects.
    ProcessFailed(ProcessFailure),
    /// Output could not be written.
    Write(io::Error),
    /// `--install` has no `HOME` to locate the global Git config from.
    MissingHome,
    /// `--install` found the global Git config disabled.
    GlobalConfigDisabled,
    /// `--install` cannot spell a path as a shell command.
    NonUtf8Path(PathRole),
    /// `--install` could not tell whether a config file exists.
    InspectConfig { path: PathBuf, source: io::Error },
    /// The global Git config could not be copied to its backup.
    Backup {
        backup: BackupGlobalGitConfig,
        source: io::Error,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { name, source } => write!(formatter, "cannot run {name}: {source}"),
            Self::ProcessFailed(failure) => write!(formatter, "{failure}"),
            Self::Write(source) => write!(formatter, "{source}"),
            Self::MissingHome => {
                formatter.write_str("HOME is required to locate the global Git config")
            }
            Self::GlobalConfigDisabled => {
                formatter.write_str("GIT_CONFIG_GLOBAL=/dev/null disables global Git configuration")
            }
            Self::NonUtf8Path(role) => write!(formatter, "{} is not valid UTF-8", role.label()),
            Self::InspectConfig { path, source } => {
                write!(formatter, "{}: {source}", path.display())
            }
            Self::Backup { backup, source } => write!(
                formatter,
                "backing up {} to {}: {source}",
                backup.config.as_str(),
                backup.copy.as_str()
            ),
        }
    }
}

impl std::error::Error for Error {}
