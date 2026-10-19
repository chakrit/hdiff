use std::process::{Command, Stdio};

use super::{Error, child_process};
use crate::{
    comparison::{Comparison, Depth},
    exit_status::DiffStatus,
};

/// Runs `diff -u` on a comparison's operands and returns its output.
pub struct CompareFiles<'a> {
    pub comparison: &'a Comparison,
}

/// The context `CompareFiles` runs in; `diff` needs nothing beyond the process.
pub struct CompareFilesContext;

impl CompareFiles<'_> {
    /// Runs `diff`; exit status 0 or 1 is a comparison and returns `diff`'s output, and
    /// any other status is the trouble `diff` reported.
    pub fn run(self, _context: &mut CompareFilesContext) -> Result<Vec<u8>, Error> {
        let operands = &self.comparison.operands;
        let recursion: &[&str] = match self.comparison.depth {
            Depth::TopLevel => &[],
            Depth::Recursive => &["-r"],
        };

        // `-N` renders added and removed files as diffs; `--` keeps dash-leading paths
        // as operands; `diff` reads the inherited stdin for `-`.
        let mut diff = Command::new("diff");
        diff.args(["-u", "-N"])
            .args(recursion)
            .arg("--")
            .arg(operands.old.argument())
            .arg(operands.new.argument())
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
