use std::{fs, io};

use super::global_git_config::GlobalGitConfigContext;
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

    pub fn run(&self, _context: &mut GlobalGitConfigContext) -> io::Result<()> {
        fs::copy(&self.config, &self.copy).map(|_| ())
    }
}
