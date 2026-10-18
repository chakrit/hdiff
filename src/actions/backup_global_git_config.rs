use std::{fs, io, path::Path};

use super::global_git_config::GlobalGitConfigContext;

pub struct BackupGlobalGitConfig<'a> {
    pub source: &'a Path,
    pub destination: &'a Path,
}

impl BackupGlobalGitConfig<'_> {
    pub fn run(self, _context: &mut GlobalGitConfigContext) -> io::Result<()> {
        fs::copy(self.source, self.destination).map(|_| ())
    }
}
