//! The parsed document: surrounding text and file diffs in input order. Contract:
//! docs/spec/architecture.md#core-model.

use crate::{
    compact::Count,
    file_identity::FileIdentity,
    headers::{FileHeader, GitHeader},
    source_text::SourceLine,
};

/// Sections in input order, with no two text sections adjacent; only the parser builds
/// one, which keeps that invariant.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffDocument {
    sections: Box<[Section]>,
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
    /// A document of `sections`, which must not hold two adjacent text sections.
    pub(crate) fn new(sections: Box<[Section]>) -> Self {
        Self { sections }
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

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

impl DiffFile {
    /// One value for each of the file's records, built hunk by hunk from `values`; a
    /// file without hunks maps no records.
    ///
    /// Panics when `values` gives a hunk a different number of values than it has
    /// records.
    pub fn record_map<T>(
        &self,
        mut values: impl FnMut(&UnifiedBody, &Hunk) -> Box<[T]>,
    ) -> RecordMap<T> {
        let FileBody::Unified(body) = &self.body else {
            return RecordMap::empty();
        };

        let hunks = body
            .hunks
            .iter()
            .map(|hunk| {
                let hunk_values = values(body, hunk);
                assert_eq!(
                    hunk_values.len(),
                    hunk.records.len(),
                    "one value for each record of the hunk"
                );
                hunk_values
            })
            .collect();

        RecordMap { hunks }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FileBody {
    /// A Git section without unified headers, such as a rename, mode change, or binary
    /// change.
    Metadata(GitHeader),
    Unified(Box<UnifiedBody>),
}

/// A section with `---` / `+++` headers and hunks, after its Git header when it has one.
#[derive(Debug, PartialEq, Eq)]
pub struct UnifiedBody {
    pub git: Option<GitHeader>,
    pub old: FileHeader,
    pub new: FileHeader,
    pub hunks: Box<[Hunk]>,
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

/// One value for each side of a change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BySide<T> {
    pub old: T,
    pub new: T,
}

impl<T> BySide<T> {
    pub fn get(&self, side: Side) -> &T {
        match side {
            Side::Old => &self.old,
            Side::New => &self.new,
        }
    }

    pub fn get_mut(&mut self, side: Side) -> &mut T {
        match side {
            Side::Old => &mut self.old,
            Side::New => &mut self.new,
        }
    }

    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> BySide<U> {
        BySide {
            old: f(self.old),
            new: f(self.new),
        }
    }

    /// Each side's value paired with `other`'s value for that side.
    pub fn zip<U>(self, other: BySide<U>) -> BySide<(T, U)> {
        BySide {
            old: (self.old, other.old),
            new: (self.new, other.new),
        }
    }

    /// Each side with its value, old side first.
    pub fn into_sides(self) -> [(Side, T); 2] {
        [(Side::Old, self.old), (Side::New, self.new)]
    }
}

/// A record's place in its file: its hunk, then its position among the hunk's records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordAddress {
    hunk: Count,
    record: Count,
}

impl RecordAddress {
    /// The record at position `record` of hunk `hunk`.
    ///
    /// Panics when either position does not fit in a [`Count`].
    pub fn new(hunk: usize, record: usize) -> Self {
        Self {
            hunk: Count::of(hunk),
            record: Count::of(record),
        }
    }

    /// Records between this record and `other` when both are in one hunk; records of
    /// different hunks have no distance.
    pub fn records_from(self, other: Self) -> Option<Count> {
        (self.hunk == other.hunk).then(|| self.record.abs_diff(other.record))
    }
}

/// One value for each record of a file, looked up by [`RecordAddress`]. Only
/// [`DiffFile::record_map`] builds a map with records, so a map always matches its file.
#[derive(Debug, PartialEq, Eq)]
pub struct RecordMap<T> {
    hunks: Box<[Box<[T]>]>,
}

impl<T> RecordMap<T> {
    /// The map of a page without records, such as surrounding text.
    pub fn empty() -> Self {
        Self {
            hunks: Box::new([]),
        }
    }

    /// The value of the record at `address`.
    ///
    /// Panics when `address` names no record of the map's file.
    pub fn get(&self, address: RecordAddress) -> &T {
        &self.hunks[address.hunk.get()][address.record.get()]
    }
}
