//! The parsed document: file diffs in input order with their surrounding text. Rules:
//! docs/spec/architecture.md#core-model.

use std::{iter, marker::PhantomData};

use crate::{
    compact::Count,
    file_identity::FileIdentity,
    headers::{FileHeader, GitHeader},
    non_empty::NonEmpty,
    side::{BySide, Side},
    source_text::SourceLine,
};

/// A parsed document: file diffs with their surrounding text, or, when the input holds no
/// file, its surrounding text alone.
#[derive(Debug, PartialEq, Eq)]
pub enum DiffDocument {
    Files(DiffFiles),
    Text(Box<[SourceLine]>),
}

/// File diffs in input order, each with the surrounding text before it, then the text
/// after the last file.
#[derive(Debug, PartialEq, Eq)]
pub struct DiffFiles {
    sections: NonEmpty<FileSection>,
    trailing: Box<[SourceLine]>,
}

/// One file's diff with the surrounding text that leads up to it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FileSection {
    pub(crate) leading: Box<[SourceLine]>,
    pub(crate) file: DiffFile,
}

/// A file occurrence as a page shows it: the file with the text before and after it.
#[derive(Clone, Copy)]
pub struct FileView<'a> {
    pub file: &'a DiffFile,
    pub leading: &'a [SourceLine],
    pub trailing: &'a [SourceLine],
}

impl DiffDocument {
    /// The document `sections` and the `trailing` text after them form: their files, or
    /// the text alone when there are none.
    pub(crate) fn new(sections: Vec<FileSection>, trailing: Box<[SourceLine]>) -> Self {
        match NonEmpty::of(sections) {
            Some(sections) => Self::Files(DiffFiles { sections, trailing }),
            None => Self::Text(trailing),
        }
    }

    /// The document's files in input order; a text document has none.
    #[cfg(test)]
    pub fn files(&self) -> impl Iterator<Item = &DiffFile> {
        self.file_views().map(|view| view.file)
    }

    /// Each file with its surrounding text; a text document has none.
    #[cfg(test)]
    pub fn file_views(&self) -> impl Iterator<Item = FileView<'_>> {
        let views = match self {
            Self::Files(files) => Some(files.views()),
            Self::Text(_) => None,
        };

        views.into_iter().flatten()
    }
}

impl DiffFiles {
    /// Each file with the text leading up to it; the last file also carries the trailing
    /// text.
    pub fn views(&self) -> NonEmpty<FileView<'_>> {
        self.sections.map_with_last(
            |section| section.view(&[]),
            |section| section.view(&self.trailing),
        )
    }
}

impl FileSection {
    /// The section's file with its leading text and `trailing` text.
    fn view<'a>(&'a self, trailing: &'a [SourceLine]) -> FileView<'a> {
        FileView {
            file: &self.file,
            leading: &self.leading,
            trailing,
        }
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
    /// record no entry names maps to no values, and a metadata-only file maps no records.
    ///
    /// Each call of `entries` receives one hunk's [`HunkRecords`] under a fresh brand, so
    /// the positions it returns can only name records of that hunk.
    pub fn record_map<E>(
        &self,
        mut entries: impl for<'h> FnMut(
            &BySide<FileHeader>,
            HunkRecords<'_, 'h>,
        ) -> Vec<RecordEntry<'h, E>>,
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
                    for RecordEntry { position, value } in entries(&body.headers, records) {
                        values[position.index].push(value);
                    }
                    values
                })
            })
            .collect();

        RecordMap { hunks }
    }
}

/// What a file section holds after its identity.
#[derive(Debug, PartialEq, Eq)]
pub enum FileBody {
    /// A Git section without unified headers, such as a rename, mode change, or binary
    /// change.
    Metadata(MetadataBody),
    Unified(Box<UnifiedBody>),
}

/// A Git section's `diff --git` line with at least one extended header after it, which
/// Git writes for every change it reports without `---` / `+++` headers, then the binary
/// patch Git writes for a binary change when it has one.
#[derive(Debug, PartialEq, Eq)]
pub struct MetadataBody {
    pub git: GitHeader<NonEmpty<SourceLine>>,
    pub binary_patch: Option<Box<BinaryPatch>>,
}

/// Git's binary patch: the `GIT binary patch` line, then its `literal` and `delta`
/// blocks. Rules: docs/spec/architecture.md#input-contract.
#[derive(Debug, PartialEq, Eq)]
pub struct BinaryPatch {
    pub header: SourceLine,
    pub blocks: NonEmpty<BinaryBlock>,
}

impl BinaryPatch {
    /// Every line of the patch, in input order.
    pub fn lines(&self) -> impl Iterator<Item = &SourceLine> {
        let blocks = self.blocks.iter().flat_map(BinaryBlock::lines);

        iter::once(&self.header).chain(blocks)
    }
}

/// A `literal` or `delta` block: its header, its base-85 payload lines, and the blank
/// line that ends it.
#[derive(Debug, PartialEq, Eq)]
pub struct BinaryBlock {
    pub header: SourceLine,
    pub payload: NonEmpty<SourceLine>,
    pub end: SourceLine,
}

impl BinaryBlock {
    /// Every line of the block, in input order.
    fn lines(&self) -> impl Iterator<Item = &SourceLine> {
        iter::once(&self.header)
            .chain(self.payload.iter())
            .chain(iter::once(&self.end))
    }
}

/// A section with `---` / `+++` headers and hunks, after its Git header when it has one.
#[derive(Debug, PartialEq, Eq)]
pub struct UnifiedBody {
    pub git: Option<GitHeader<Box<[SourceLine]>>>,
    pub headers: BySide<FileHeader>,
    pub hunks: NonEmpty<Hunk>,
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

/// An `@@` header line and the records it declares.
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

    /// The sides of the file the no-newline markers among the hunk's records end.
    pub fn ended_sides(&self) -> BySide<bool> {
        self.records
            .iter()
            .map(Record::ended_sides)
            .fold(BySide::default(), BySide::or)
    }
}

/// A hunk with its place among its file's hunks.
#[derive(Clone, Copy)]
pub struct AddressedHunk<'a> {
    index: usize,
    hunk: &'a Hunk,
}

impl<'a> AddressedHunk<'a> {
    pub fn header(self) -> &'a SourceLine {
        &self.hunk.header
    }

    /// The hunk's records, each with its address in the file.
    pub fn records(self) -> impl Iterator<Item = AddressedRecord<'a>> {
        let hunk_index = self.index;
        self.hunk
            .records
            .iter()
            .enumerate()
            .map(move |(record_index, record)| AddressedRecord {
                address: RecordAddress::new(hunk_index, record_index),
                record,
            })
    }
}

/// A record with its address in its file, as [`AddressedHunk::records`] yields them.
pub struct AddressedRecord<'a> {
    pub address: RecordAddress,
    pub record: &'a Record,
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

    /// The hunk's records, each at its branded position among them.
    pub fn positioned(self) -> impl Iterator<Item = RecordEntry<'h, &'a Record>> {
        self.hunk
            .records
            .iter()
            .enumerate()
            .map(move |(index, value)| RecordEntry {
                position: RecordPosition {
                    index,
                    brand: self.brand,
                },
                value,
            })
    }
}

/// A record's position among the records of the hunk whose [`HunkRecords`] carries the
/// brand `'h`; positions from another hunk carry another brand and do not type-check.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RecordPosition<'h> {
    index: usize,
    brand: Brand<'h>,
}

/// A value at a record's `position`: the record itself, as [`HunkRecords::positioned`]
/// yields it, or a value [`DiffFile::record_map`] gathers for it.
pub struct RecordEntry<'h, E> {
    pub position: RecordPosition<'h>,
    pub value: E,
}

/// An invariant lifetime marker: `'h` can neither grow nor shrink, so two brands unify
/// only when they are the same brand.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Brand<'h>(PhantomData<fn(&'h ()) -> &'h ()>);

/// One hunk record: its kind, the line after its marker, and the no-newline marker after
/// it when the record's line ends its file without a line ending. A marker belongs to
/// the record it qualifies, so a hunk can neither open with one nor hold two in a row.
#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub kind: RecordKind,
    pub payload: SourceLine,
    /// The `\ ` line after the record, whatever language its producer wrote it in.
    pub no_newline: Option<SourceLine>,
}

impl Record {
    /// The sides of the file the record's no-newline marker ends: the record's own sides
    /// when it has a marker, and none otherwise.
    pub fn ended_sides(&self) -> BySide<bool> {
        let marked = self.no_newline.is_some();

        self.kind.sides().map(|on_side| marked && on_side)
    }
}

/// Which side or sides of a hunk a record belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    /// A line of both sides, spelled with the marker its producer wrote.
    Context(ContextMarker),
    Addition,
    Deletion,
}

/// How a context line was spelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextMarker {
    /// The usual leading space.
    Space,
    /// A blank context line written as an empty line, with no marker at all, as GNU
    /// `diff --suppress-blank-empty` writes it.
    Blank,
}

impl RecordKind {
    /// The kind a hunk line's leading marker gives, or `None` for an unmarked line.
    pub fn of_marker(marker: char) -> Option<Self> {
        match marker {
            ' ' => Some(Self::Context(ContextMarker::Space)),
            '+' => Some(Self::Addition),
            '-' => Some(Self::Deletion),
            _ => None,
        }
    }

    /// The marker that precedes the payload as the source spelled it; a blank context
    /// line has none.
    pub fn marker(self) -> &'static str {
        match self {
            Self::Context(ContextMarker::Space) => " ",
            Self::Context(ContextMarker::Blank) => "",
            Self::Addition => "+",
            Self::Deletion => "-",
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

    /// The side a change removes from or adds to; context changes neither.
    pub fn changed_side(self) -> Option<Side> {
        Side::BOTH
            .into_iter()
            .find(|side| Self::change_of(*side) == self)
    }

    /// The sides whose file the record is a line of: context is a line of both sides, and
    /// a change a line of the side it changes.
    pub fn sides(self) -> BySide<bool> {
        let changed_side = self.changed_side();

        BySide::of(|side| changed_side.is_none_or(|changed| changed == side))
    }

    /// Whether the record is a line of `side`'s file.
    pub fn on_side(self, side: Side) -> bool {
        *self.sides().get(side)
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
