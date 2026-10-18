use std::{
    ffi::OsStr,
    fmt, io,
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
};

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

#[derive(Debug)]
pub enum Error {
    /// `diff` could not be started.
    Spawn(io::Error),
    /// `diff` exited with trouble, explained by its standard error when it wrote one.
    Trouble { status: ExitStatus, stderr: String },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "diff: {error}"),
            Self::Trouble { status, stderr } => match stderr.trim_end() {
                "" => write!(formatter, "diff failed: {status}"),
                explanation => formatter.write_str(explanation),
            },
        }
    }
}

impl std::error::Error for Error {}

impl CompareFiles<'_> {
    pub fn run(self, _context: &mut CompareFilesContext) -> Result<Vec<u8>, Error> {
        let [before, after] = &self.comparison.operands;
        let recursion: &[&str] = match self.comparison.depth {
            Depth::TopLevel => &[],
            Depth::Recursive => &["-r"],
        };

        // `-N` renders added and removed files as diffs; `--` keeps dash-leading paths
        // operands; `diff` reads the inherited stdin for `-`.
        let output = Command::new("diff")
            .args(["-u", "-N"])
            .args(recursion)
            .arg("--")
            .arg(before.argument())
            .arg(after.argument())
            .stdin(Stdio::inherit())
            .output()
            .map_err(Error::Spawn)?;

        match DiffStatus::of_exit(output.status) {
            DiffStatus::Same | DiffStatus::Different => Ok(output.stdout),
            DiffStatus::Trouble => Err(Error::Trouble {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            }),
        }
    }
}
