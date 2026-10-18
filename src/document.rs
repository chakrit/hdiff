//! The parsed document: surrounding text and file diffs in input order. Contract:
//! docs/spec/architecture.md#core-model.

use std::marker::PhantomData;

use crate::{
    compact::Count,
    file_identity::FileIdentity,
    headers::{FileHeader, GitHeader},
    side::{BySide, Side},
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
    /// The values `entries` gives each of the file's records, gathered hunk by hunk: a
    /// record no entry names maps to no values, and a file without hunks maps no records.
    ///
    /// Each call of `entries` receives one hunk's [`HunkRecords`] under a fresh brand, so
    /// the positions it returns can only name records of that hunk.
    pub fn record_map<E>(
        &self,
        mut entries: impl for<'h> FnMut(
            &BySide<FileHeader>,
            HunkRecords<'_, 'h>,
        ) -> Vec<(RecordPosition<'h>, E)>,
    ) -> RecordMap<Vec<E>> {
        let FileBody::Unified(body) = &self.body else {
            return RecordMap::empty();
        };

        let hunks = body
            .hunks
            .iter()
            .map(|hunk| {
                hunk.with_records(|records| {
                    let mut values = hunk
                        .records
                        .iter()
                        .map(|_| Vec::new())
                        .collect::<Box<[_]>>();
                    for (position, entry) in entries(&body.headers, records) {
                        values[position.index].push(entry);
                    }
                    values
                })
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
    pub headers: BySide<FileHeader>,
    pub hunks: Box<[Hunk]>,
}

impl UnifiedBody {
    /// Each hunk with the address of each of its records; the only source of
    /// [`RecordAddress`]es.
    pub fn addressed_hunks(&self) -> impl Iterator<Item = AddressedHunk<'_>> {
        self.hunks
            .iter()
            .enumerate()
            .map(|(index, hunk)| AddressedHunk { index, hunk })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Hunk {
    pub header: SourceLine,
    pub records: Box<[Record]>,
}

impl Hunk {
    /// `visit` applied to the hunk's records under a fresh brand, so the positions it
    /// sees can only name records of this hunk and cannot leave the call.
    pub fn with_records<T>(&self, visit: impl for<'h> FnOnce(HunkRecords<'_, 'h>) -> T) -> T {
        visit(HunkRecords::of(self))
    }
}

/// A hunk with its place among its file's hunks.
#[derive(Clone, Copy)]
pub struct AddressedHunk<'a> {
    index: usize,
    pub hunk: &'a Hunk,
}

impl<'a> AddressedHunk<'a> {
    /// The hunk's records, each with its address in the file.
    pub fn records(self) -> impl Iterator<Item = (RecordAddress, &'a Record)> {
        let hunk = self.index;
        self.hunk
            .records
            .iter()
            .enumerate()
            .map(move |(record, value)| (RecordAddress::new(hunk, record), value))
    }
}

/// One hunk's records under the brand `'h` of a single [`Hunk::with_records`] call.
#[derive(Clone, Copy)]
pub struct HunkRecords<'a, 'h> {
    hunk: &'a Hunk,
    brand: Brand<'h>,
}

impl<'a, 'h> HunkRecords<'a, 'h> {
    fn of(hunk: &'a Hunk) -> Self {
        Self {
            hunk,
            brand: Brand(PhantomData),
        }
    }

    /// The hunk's records, each with its branded position among them.
    pub fn positioned(self) -> impl Iterator<Item = (RecordPosition<'h>, &'a Record)> {
        self.hunk
            .records
            .iter()
            .enumerate()
            .map(move |(index, record)| {
                let position = RecordPosition {
                    index,
                    brand: self.brand,
                };
                (position, record)
            })
    }
}

/// A record's position among the records of the hunk whose [`HunkRecords`] carries the
/// brand `'h`; positions from another hunk carry another brand and do not type-check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordPosition<'h> {
    index: usize,
    brand: Brand<'h>,
}

/// An invariant lifetime marker: `'h` can neither grow nor shrink, so two brands unify
/// only when they are the same brand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Brand<'h>(PhantomData<fn(&'h ()) -> &'h ()>);

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

    /// The record kind that changes `side`; it decides which side each change kind
    /// changes.
    pub fn change_of(side: Side) -> Self {
        match side {
            Side::Old => Self::Deletion,
            Side::New => Self::Addition,
        }
    }

    /// The side a change removes from or adds to; context and raw lines change neither.
    pub fn changed_side(self) -> Option<Side> {
        Side::BOTH
            .into_iter()
            .find(|side| Self::change_of(*side) == self)
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
    fn new(hunk: usize, record: usize) -> Self {
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
