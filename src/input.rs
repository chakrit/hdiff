//! Diff input acquisition. Rules: docs/spec/architecture.md#input-contract.

use std::{
    fmt,
    fs::{self, File},
    io::{self, BufRead, BufReader, Read},
    os::{fd::AsFd, unix::fs::FileTypeExt},
    path::PathBuf,
};

use crate::{
    actions::{
        self,
        compare_files::{CompareFiles, CompareFilesContext},
    },
    comparison::Comparison,
    document::DiffDocument,
    parser,
};

/// Where hdiff's input comes from: a patch on standard input or in a file, or the
/// output of comparing two operands.
#[derive(Debug, PartialEq, Eq)]
pub enum Source {
    Stdin,
    PatchFile(PathBuf),
    Comparison(Comparison),
}

impl Source {
    /// Whether reading this source consumes standard input.
    pub fn reads_stdin(&self) -> bool {
        match self {
            Self::Stdin => true,
            Self::PatchFile(_) => false,
            Self::Comparison(comparison) => comparison.reads_stdin(),
        }
    }

    /// The source's bytes, running `diff` for a comparison.
    pub fn read(self) -> Result<Content, Error> {
        match self {
            Self::Stdin => read_stdin().map(Content::Patch).map_err(Error::Stdin),
            Self::PatchFile(path) => fs::read(&path)
                .map(Content::Patch)
                .map_err(|source| Error::PatchFile { path, source }),
            Self::Comparison(comparison) => CompareFiles {
                comparison: &comparison,
            }
            .run(&mut CompareFilesContext)
            .map(Content::Comparison)
            .map_err(Error::Comparison),
        }
    }

    /// The source opened to be read as its bytes arrive, running `diff` to its end for
    /// a comparison.
    pub fn open(self) -> Result<Opened, Error> {
        match self {
            Self::Stdin => Ok(Opened {
                input: Box::new(BufReader::new(io::stdin())),
                origin: Origin::Stdin,
            }),
            Self::PatchFile(path) => match File::open(&path) {
                Ok(file) => Ok(Opened {
                    input: Box::new(BufReader::new(file)),
                    origin: Origin::PatchFile(path),
                }),
                Err(source) => Err(Error::PatchFile { path, source }),
            },
            Self::Comparison(comparison) => CompareFiles {
                comparison: &comparison,
            }
            .run(&mut CompareFilesContext)
            .map(|output| Opened {
                input: Box::new(io::Cursor::new(output)),
                origin: Origin::Comparison,
            })
            .map_err(Error::Comparison),
        }
    }
}

/// A source opened to be read as its bytes arrive, with where they come from.
pub struct Opened {
    pub input: Box<dyn BufRead + Send>,
    pub origin: Origin,
}

/// Where opened input comes from, which decides whether it must hold a diff and how a
/// read failure is named.
pub enum Origin {
    Stdin,
    PatchFile(PathBuf),
    /// `diff` output, which may hold only surrounding text.
    Comparison,
}

impl Origin {
    /// The input error a failed read of this origin reports.
    pub fn read_failure(self, source: io::Error) -> Error {
        match self {
            Self::Stdin | Self::Comparison => Error::Stdin(source),
            Self::PatchFile(path) => Error::PatchFile { path, source },
        }
    }
}

/// Every byte standard input holds, read to its end.
fn read_stdin() -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Whether standard input is a pipe or socket, as when another process feeds hdiff.
pub enum StdinState {
    Piped,
    Unpiped,
}

impl StdinState {
    /// Standard input's state, read from its file type without consuming it.
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
pub enum Content {
    /// Viewer input: stdin or a patch file.
    Patch(Vec<u8>),
    /// `diff` output, which may hold only surrounding text.
    Comparison(Vec<u8>),
}

impl Content {
    /// The bytes read, whichever kind they are.
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Patch(bytes) | Self::Comparison(bytes) => bytes,
        }
    }

    /// Parses the content, releasing its buffer: the document keeps every line it needs.
    pub fn parse(self) -> Result<DiffDocument, parser::NoDiff> {
        match self {
            Self::Patch(bytes) => parser::parse_patch(&bytes),
            Self::Comparison(bytes) => Ok(parser::parse_comparison(&bytes)),
        }
    }
}

/// Why input could not be read.
#[derive(Debug)]
pub enum Error {
    Stdin(io::Error),
    PatchFile { path: PathBuf, source: io::Error },
    Comparison(actions::Error),
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
