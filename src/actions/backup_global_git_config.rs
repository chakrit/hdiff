use std::fs;

use super::{Error, global_git_config::GlobalGitConfigContext};
use crate::shell_words;

/// Copies the global Git config file `config` to `copy`. Both paths are UTF-8 so the
/// copy has a shell spelling.
#[derive(Debug)]
pub struct BackupGlobalGitConfig {
    pub config: String,
    pub copy: String,
}

impl BackupGlobalGitConfig {
    /// The `cp` command that performs exactly this copy.
    pub fn shell_command(&self) -> String {
        shell_words::command_line(&["cp", &self.config, &self.copy])
    }

    pub fn run(self, _context: &mut GlobalGitConfigContext) -> Result<(), Error> {
        match fs::copy(&self.config, &self.copy) {
            Ok(_) => Ok(()),
            Err(source) => Err(Error::Backup {
                backup: self,
                source,
            }),
        }
    }
}
