use std::{fs, io, path::Path};

use super::install_git_diff_pager::InstallGitDiffPagerContext;

pub struct BackupGlobalGitConfig<'a> {
    pub source: &'a Path,
    pub destination: &'a Path,
}

impl BackupGlobalGitConfig<'_> {
    pub fn run(self, _context: &mut InstallGitDiffPagerContext) -> io::Result<()> {
        fs::copy(self.source, self.destination).map(|_| ())
    }
}
