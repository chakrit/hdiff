use std::process::{Command, Stdio};

use super::child_process::{self, ProcessError};
use crate::{
    comparison::{Comparison, Depth},
    exit_status::DiffStatus,
};

pub struct CompareFiles<'a> {
    pub comparison: &'a Comparison,
}

pub struct CompareFilesContext;

impl CompareFiles<'_> {
    pub fn run(self, _context: &mut CompareFilesContext) -> Result<Vec<u8>, ProcessError> {
        let [before, after] = &self.comparison.operands;
        let recursion: &[&str] = match self.comparison.depth {
            Depth::TopLevel => &[],
            Depth::Recursive => &["-r"],
        };

        // `-N` renders added and removed files as diffs; `--` keeps dash-leading paths
        // operands; `diff` reads the inherited stdin for `-`.
        let mut diff = Command::new("diff");
        diff.args(["-u", "-N"])
            .args(recursion)
            .arg("--")
            .arg(before.argument())
            .arg(after.argument())
            .stdin(Stdio::inherit());

        let output = child_process::run("diff", &mut diff, |status| {
            match DiffStatus::of_exit(status) {
                DiffStatus::Same | DiffStatus::Different => true,
                DiffStatus::Trouble => false,
            }
        })?;

        Ok(output.stdout)
    }
}
