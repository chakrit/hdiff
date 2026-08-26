use std::{io, path::Path, process::Command};

pub struct CompareFiles<'a> {
    pub before: &'a Path,
    pub after: &'a Path,
}

pub struct CompareFilesContext;

impl CompareFiles<'_> {
    pub fn run(self, _context: &mut CompareFilesContext) -> io::Result<Vec<u8>> {
        let output = Command::new("diff")
            .args(["-u"])
            .arg(self.before)
            .arg(self.after)
            .output()?;

        match output.status.code() {
            Some(0 | 1) => Ok(output.stdout),
            _ => Err(io::Error::other(String::from_utf8_lossy(&output.stderr))),
        }
    }
}
