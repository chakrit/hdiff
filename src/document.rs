use crate::{
    file_identity::{FileIdentity, HeaderPath},
    source_text::SourceLine,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffDocument {
    pub sections: Vec<Section>,
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

    pub fn file(&self, index: usize) -> Option<&DiffFile> {
        self.files().nth(index)
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

#[derive(Debug, PartialEq, Eq)]
pub enum DiffFile {
    /// A Git section without unified headers: its `diff --git` line and the metadata
    /// lines that follow it.
    Metadata {
        header: SourceLine,
        lines: Vec<SourceLine>,
        identity: FileIdentity,
    },
    Unified(Box<UnifiedDiffFile>),
}

impl DiffFile {
    pub fn identity(&self) -> &FileIdentity {
        match self {
            Self::Metadata { identity, .. } => identity,
            Self::Unified(file) => &file.identity,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnifiedDiffFile {
    pub metadata: Vec<SourceLine>,
    pub identity: FileIdentity,
    pub old_path: HeaderPath,
    pub new_path: HeaderPath,
    pub old_header: SourceLine,
    pub new_header: SourceLine,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Hunk {
    pub header: SourceLine,
    pub records: Vec<Record>,
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
}
