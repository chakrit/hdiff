use std::{fs, io};

use super::global_git_config::GlobalGitConfigContext;
use crate::shell_words;

/// Copies the global Git config to its backup path. Both paths are UTF-8 so the copy
/// has a shell spelling.
pub struct BackupGlobalGitConfig<'a> {
    pub source: &'a str,
    pub destination: &'a str,
}

impl BackupGlobalGitConfig<'_> {
    /// The `cp` command that performs exactly this copy.
    pub fn shell_command(&self) -> String {
        shell_words::command_line(&["cp", self.source, self.destination])
    }

    pub fn run(self, _context: &mut GlobalGitConfigContext) -> io::Result<()> {
        fs::copy(self.source, self.destination).map(|_| ())
    }
}
