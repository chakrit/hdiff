use std::fs;

use super::{Error, global_git_config::GlobalGitConfigContext};
use crate::{shell_words, utf8_path::Utf8Path};

/// Copies the global Git config file `config` to `copy`. Both paths are UTF-8 so the
/// copy has a shell spelling.
#[derive(Debug)]
pub struct BackupGlobalGitConfig {
    pub config: Utf8Path,
    pub copy: Utf8Path,
}

impl BackupGlobalGitConfig {
    /// The `cp` command that performs exactly this copy.
    pub fn shell_command(&self) -> String {
        shell_words::command_line(&["cp", self.config.as_str(), self.copy.as_str()])
    }

    pub fn run(self, _context: &mut GlobalGitConfigContext) -> Result<(), Error> {
        match fs::copy(self.config.as_path(), self.copy.as_path()) {
            Ok(_) => Ok(()),
            Err(source) => Err(Error::Backup {
                backup: self,
                source,
            }),
        }
    }
}
