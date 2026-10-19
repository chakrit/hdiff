use std::{
    ffi::OsStr,
    path::PathBuf,
    process::{Command, Stdio},
};

use super::child_process::{self, ProcessError};
use crate::exit_status::DiffStatus;

/// Two operands for `diff -u`. Contract: docs/spec/architecture.md#input-contract.
#[derive(Debug, PartialEq, Eq)]
pub struct Comparison {
    pub operands: [Operand; 2],
    pub depth: Depth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Stdin,
    Path(PathBuf),
}

impl Operand {
    /// The operand as `diff` reads it, where `-` names standard input.
    pub fn argument(&self) -> &OsStr {
        match self {
            Self::Stdin => OsStr::new("-"),
            Self::Path(path) => path.as_os_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    TopLevel,
    Recursive,
}

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
