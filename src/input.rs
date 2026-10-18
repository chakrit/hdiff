//! Diff input acquisition. Contract: docs/spec/architecture.md#input-contract.

use std::{
    fmt,
    fs::{self, File},
    io::{self, Read},
    os::{fd::AsFd, unix::fs::FileTypeExt},
    path::PathBuf,
};

use crate::{
    actions::compare_files::{self, CompareFiles, CompareFilesContext, Comparison, Operand},
    document::DiffDocument,
    parser::{self, ParseError},
};

#[derive(Debug, PartialEq, Eq)]
pub enum Source {
    Stdin,
    PatchFile(PathBuf),
    Comparison(Comparison),
}

impl Source {
    pub fn reads_stdin(&self) -> bool {
        match self {
            Self::Stdin => true,
            Self::PatchFile(_) => false,
            Self::Comparison(comparison) => comparison.operands.contains(&Operand::Stdin),
        }
    }

    pub fn read(self) -> Result<Content, Error> {
        match self {
            Self::Stdin => {
                let mut bytes = Vec::new();
                io::stdin().read_to_end(&mut bytes).map_err(Error::Stdin)?;
                Ok(Content::Patch(bytes))
            }
            Self::PatchFile(path) => match fs::read(&path) {
                Ok(bytes) => Ok(Content::Patch(bytes)),
                Err(source) => Err(Error::PatchFile { path, source }),
            },
            Self::Comparison(comparison) => CompareFiles {
                comparison: &comparison,
            }
            .run(&mut CompareFilesContext)
            .map(Content::Comparison)
            .map_err(Error::Comparison),
        }
    }
}

/// Whether standard input is a pipe or socket, as when another process feeds hdiff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinState {
    Piped,
    Unpiped,
}

impl StdinState {
    pub fn inspect() -> Result<Self, Error> {
        let file_type = io::stdin()
            .as_fd()
            .try_clone_to_owned()
            .map(File::from)
            .and_then(|stdin| stdin.metadata())
            .map_err(Error::Stdin)?
            .file_type();

        match file_type.is_fifo() || file_type.is_socket() {
            true => Ok(Self::Piped),
            false => Ok(Self::Unpiped),
        }
    }
}

/// Bytes read from a source, tagged by whether they must hold a file diff.
#[derive(Debug, PartialEq, Eq)]
pub enum Content {
    /// Viewer input: stdin or a patch file.
    Patch(Vec<u8>),
    /// `diff` output, which may hold only surrounding text.
    Comparison(Vec<u8>),
}

impl Content {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Patch(bytes) | Self::Comparison(bytes) => bytes,
        }
    }

    /// Parses the content, releasing its buffer: the document keeps every line it needs.
    pub fn parse(self) -> Result<DiffDocument, ParseError> {
        match self {
            Self::Patch(bytes) => parser::parse_patch(&bytes),
            Self::Comparison(bytes) => parser::parse_comparison(&bytes),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Stdin(io::Error),
    PatchFile { path: PathBuf, source: io::Error },
    Comparison(compare_files::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdin(error) => write!(formatter, "standard input: {error}"),
            Self::PatchFile { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Comparison(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for Error {}
