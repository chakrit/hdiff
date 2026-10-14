use std::{
    io,
    process::{Command, Stdio},
};

use crate::input::{Comparison, Depth};

pub struct CompareFiles<'a> {
    pub comparison: &'a Comparison,
}

pub struct CompareFilesContext;

impl CompareFiles<'_> {
    pub fn run(self, _context: &mut CompareFilesContext) -> io::Result<Vec<u8>> {
        let [before, after] = &self.comparison.operands;
        let recursion: &[&str] = match self.comparison.depth {
            Depth::TopLevel => &[],
            Depth::Recursive => &["-r"],
        };

        // `--` keeps dash-leading paths operands; `diff` reads the inherited stdin for `-`.
        let output = Command::new("diff")
            .arg("-u")
            .args(recursion)
            .arg("--")
            .arg(before.argument())
            .arg(after.argument())
            .stdin(Stdio::inherit())
            .output()?;

        match output.status.code() {
            Some(0 | 1) => Ok(output.stdout),
            _ => Err(io::Error::other(String::from_utf8_lossy(&output.stderr))),
        }
    }
}
