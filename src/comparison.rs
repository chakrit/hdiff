use std::{ffi::OsStr, path::PathBuf};

use crate::side::BySide;

/// Two operands for `diff -u`. Rules: docs/spec/architecture.md#input-contract.
#[derive(Debug, PartialEq, Eq)]
pub struct Comparison {
    pub operands: BySide<Operand>,
    pub depth: Depth,
}

impl Comparison {
    /// Whether either operand is standard input.
    pub fn reads_stdin(&self) -> bool {
        self.operands
            .iter()
            .any(|operand| *operand == Operand::Stdin)
    }
}

/// One side of a comparison: standard input or a path.
#[derive(Debug, PartialEq, Eq)]
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

/// Whether a comparison of directories descends into their subdirectories.
#[derive(Debug, PartialEq, Eq)]
pub enum Depth {
    TopLevel,
    Recursive,
}
