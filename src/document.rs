//! The parsed document: surrounding text and file diffs in input order. Contract:
//! docs/spec/architecture.md#core-model.

use crate::{
    compact::{narrow, widen},
    file_identity::{FileIdentity, GitLines, HeaderPath},
    source_text::SourceLine,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffDocument {
    pub sections: Box<[Section]>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Section {
    Text(Box<[SourceLine]>),
    File(DiffFile),
}

#[derive(Clone, Copy)]
pub struct FileView<'a> {
    pub file: &'a DiffFile,
    pub leading: &'a [SourceLine],
    pub trailing: &'a [SourceLine],
}

impl DiffDocument {
    pub fn files(&self) -> impl Iterator<Item = &DiffFile> {
        self.sections.iter().filter_map(|section| match section {
            Section::File(file) => Some(file),
            Section::Text(_) => None,
        })
    }

    /// Every surrounding-text line, in input order.
    pub fn text(&self) -> impl Iterator<Item = &SourceLine> {
        self.sections.iter().flat_map(|section| match section {
            Section::Text(lines) => lines.as_ref(),
            Section::File(_) => &[],
        })
    }

    pub fn file_views(&self) -> impl Iterator<Item = FileView<'_>> {
        self.sections
            .iter()
            .enumerate()
            .filter_map(|(index, section)| {
                let Section::File(file) = section else {
                    return None;
                };
                let leading = match index
                    .checked_sub(1)
                    .and_then(|previous| self.sections.get(previous))
                {
                    Some(Section::Text(lines)) => lines.as_ref(),
                    _ => &[],
                };
                let trailing = match self.sections.get(index + 1) {
                    Some(Section::Text(lines)) if index + 2 == self.sections.len() => {
                        lines.as_ref()
                    }
                    _ => &[],
                };

                Some(FileView {
                    file,
                    leading,
                    trailing,
                })
            })
    }
}

/// One file's diff: which file it names, and its lines.
#[derive(Debug, PartialEq, Eq)]
pub struct DiffFile {
    pub identity: FileIdentity,
    pub body: FileBody,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FileBody {
    /// A Git section without unified headers, such as a rename, mode change, or binary
    /// change.
    Metadata(GitHeader),
    Unified(Box<UnifiedBody>),
}

/// A `diff --git` line and the extended header lines after it.
#[derive(Debug, PartialEq, Eq)]
pub struct GitHeader {
    pub line: SourceLine,
    pub metadata: Box<[SourceLine]>,
}

impl GitHeader {
    pub fn lines(&self) -> GitLines<'_> {
        GitLines {
            header: &self.line,
            metadata: &self.metadata,
        }
    }
}

/// A section with `---` / `+++` headers and hunks, after its Git header when it has one.
#[derive(Debug, PartialEq, Eq)]
pub struct UnifiedBody {
    pub git: Option<GitHeader>,
    pub old: FileHeader,
    pub new: FileHeader,
    pub hunks: Box<[Hunk]>,
}

/// A `---` or `+++` line and the path it names.
#[derive(Debug, PartialEq, Eq)]
pub struct FileHeader {
    pub line: SourceLine,
    pub path: HeaderPath,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Hunk {
    pub header: SourceLine,
    pub records: Box<[Record]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub kind: RecordKind,
    pub payload: SourceLine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    Context,
    Addition,
    Deletion,
    Raw,
}

impl RecordKind {
    /// The kind a hunk line's leading marker gives, or `None` for an unmarked line.
    pub fn of_marker(marker: char) -> Option<Self> {
        match marker {
            ' ' => Some(Self::Context),
            '+' => Some(Self::Addition),
            '-' => Some(Self::Deletion),
            _ => None,
        }
    }

    /// The marker that precedes the payload; a raw line has none.
    pub fn marker(self) -> &'static str {
        match self {
            Self::Context => " ",
            Self::Addition => "+",
            Self::Deletion => "-",
            Self::Raw => "",
        }
    }

    /// The side a change removes from or adds to; context and raw lines change neither.
    pub fn changed_side(self) -> Option<Side> {
        Side::BOTH.into_iter().find(|side| side.change() == self)
    }

    /// Whether the record is a line of `side`'s file: context is a line of both sides, a
    /// change a line of the side it changes, and a raw line a line of neither.
    pub fn on_side(self, side: Side) -> bool {
        match self {
            Self::Context => true,
            Self::Addition | Self::Deletion => self.changed_side() == Some(side),
            Self::Raw => false,
        }
    }
}

/// One of the two files a diff compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Old,
    New,
}

impl Side {
    pub const BOTH: [Self; 2] = [Self::Old, Self::New];

    /// The record kind that changes this side; it decides which side each change kind
    /// changes.
    pub fn change(self) -> RecordKind {
        match self {
            Self::Old => RecordKind::Deletion,
            Self::New => RecordKind::Addition,
        }
    }
}

/// A record's place in its file: its hunk, then its position among the hunk's records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordAddress {
    hunk: u32,
    record: u32,
}

impl RecordAddress {
    /// The record at position `record` of hunk `hunk`.
    ///
    /// Panics when either position does not fit in `u32`.
    pub fn new(hunk: usize, record: usize) -> Self {
        Self {
            hunk: narrow(hunk),
            record: narrow(record),
        }
    }

    /// Records between this record and `other` when both are in one hunk; records of
    /// different hunks have no distance.
    pub fn records_from(self, other: Self) -> Option<u32> {
        (self.hunk == other.hunk).then(|| self.record.abs_diff(other.record))
    }
}

/// One value for each record of a file, looked up by [`RecordAddress`].
#[derive(Debug, PartialEq, Eq)]
pub struct RecordMap<T> {
    hunks: Box<[Box<[T]>]>,
}

impl<T> RecordMap<T> {
    /// A map from each hunk's values, in hunk order and then record order.
    pub fn from_hunks(hunks: impl IntoIterator<Item = Box<[T]>>) -> Self {
        Self {
            hunks: hunks.into_iter().collect(),
        }
    }

    /// The value of the record at `address`.
    ///
    /// Panics when `address` names no record of the map's file.
    pub fn get(&self, address: RecordAddress) -> &T {
        &self.hunks[widen(address.hunk)][widen(address.record)]
    }
}

/// The map of a file without records.
impl<T> Default for RecordMap<T> {
    fn default() -> Self {
        Self {
            hunks: Box::new([]),
        }
    }
}
