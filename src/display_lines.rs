//! The lines a document displays, in input order: what each line shows, and the safe text it
//! displays. Finite output writes them directly, and page rows retain them. Rules:
//! `docs/spec/architecture.md` (Lossless and safe rendering).

use crate::{
    document::{DiffDocument, DiffFile, FileBody, RecordAddress, RecordKind, Section},
    headers::GitHeader,
    side::Side,
    source_text::SourceLine,
};

/// What a displayed line shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Text,
    Metadata,
    FileHeader,
    HunkHeader,
    Record {
        address: RecordAddress,
        kind: RecordKind,
    },
}

impl LineKind {
    /// The record's kind; a line that is not a record has none.
    pub fn record_kind(self) -> Option<RecordKind> {
        match self {
            Self::Record { kind, .. } => Some(kind),
            Self::Text | Self::Metadata | Self::FileHeader | Self::HunkHeader => None,
        }
    }

    /// The side a changed record removes from or adds to; other lines change neither.
    pub fn changed_side(self) -> Option<Side> {
        self.record_kind()?.changed_side()
    }

    /// The record marker that precedes the line's payload; other lines have none.
    pub fn marker(self) -> &'static str {
        self.record_kind().map_or("", RecordKind::marker)
    }
}

/// One source line as a document displays it.
#[derive(Clone, Copy)]
pub struct DisplayLine<'a> {
    pub kind: LineKind,
    pub source: &'a SourceLine,
}

impl<'a> DisplayLine<'a> {
    /// The sanitized text between the record marker and the line ending.
    pub fn payload(self) -> &'a str {
        self.source.sanitized()
    }

    /// Appends the line's text to `text`: its record marker, its sanitized payload, and
    /// its source line ending. Sanitized text holds no line ending, so the text ends in
    /// exactly the source's.
    pub fn write_text(self, text: &mut String) {
        text.push_str(self.kind.marker());
        text.push_str(self.payload());
        text.push_str(LineEnding::of(self.source.bytes()).text());
    }
}

/// The line ending a source line ends with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineEnding {
    None,
    Lf,
    CrLf,
}

impl LineEnding {
    fn of(text: &[u8]) -> Self {
        if text.ends_with(b"\r\n") {
            Self::CrLf
        } else if text.ends_with(b"\n") {
            Self::Lf
        } else {
            Self::None
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

/// Every line of `document`, in input order.
pub fn of_document(document: &DiffDocument) -> impl Iterator<Item = DisplayLine<'_>> {
    document.sections().iter().flat_map(
        |section| -> Box<dyn Iterator<Item = DisplayLine<'_>> + '_> {
            match section {
                Section::File(file) => Box::new(of_file(file)),
                Section::Text(lines) => Box::new(of_text(lines)),
            }
        },
    )
}

/// Surrounding-text lines.
pub fn of_text<'a>(
    lines: impl IntoIterator<Item = &'a SourceLine>,
) -> impl Iterator<Item = DisplayLine<'a>> {
    lines.into_iter().map(|source| DisplayLine {
        kind: LineKind::Text,
        source,
    })
}

/// The lines of `file`: its Git header and metadata, its file headers, then each hunk's
/// header and records.
pub fn of_file(file: &DiffFile) -> impl Iterator<Item = DisplayLine<'_>> {
    let (header, unified) = match &file.body {
        FileBody::Metadata(header) => (Some(header), None),
        FileBody::Unified(body) => (body.git.as_ref(), Some(body)),
    };
    let file_headers = unified
        .into_iter()
        .flat_map(|body| [&body.headers.old, &body.headers.new])
        .map(|header| DisplayLine {
            kind: LineKind::FileHeader,
            source: &header.line,
        });
    let hunks = unified
        .into_iter()
        .flat_map(|body| body.addressed_hunks())
        .flat_map(|hunk| {
            let header = DisplayLine {
                kind: LineKind::HunkHeader,
                source: &hunk.hunk.header,
            };
            let records = hunk.records().map(|(address, record)| {
                let kind = LineKind::Record {
                    address,
                    kind: record.kind,
                };
                DisplayLine {
                    kind,
                    source: &record.payload,
                }
            });
            std::iter::once(header).chain(records)
        });

    header
        .into_iter()
        .flat_map(git_header_lines)
        .chain(file_headers)
        .chain(hunks)
}

fn git_header_lines(header: &GitHeader) -> impl Iterator<Item = DisplayLine<'_>> {
    std::iter::once(&header.line)
        .chain(&header.metadata)
        .map(|source| DisplayLine {
            kind: LineKind::Metadata,
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::{LineKind, of_file};
    use crate::{
        document::{FileBody, RecordKind},
        parser::parse_patch,
    };

    #[test]
    fn classifies_metadata_file_headers_hunks_and_records() {
        let input = b"diff --git a/file b/file\nindex 1..2 100644\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n@@ -3 +3 @@\n-before\n+after\n";
        let document = parse_patch(input).expect("valid Git diff with two hunks");
        let file = document.files().next().expect("first file");
        let FileBody::Unified(body) = &file.body else {
            panic!("fixture should contain a unified file");
        };
        let first_records = body
            .addressed_hunks()
            .map(|hunk| hunk.records().next().expect("hunk with records").0)
            .collect::<Vec<_>>();

        let kinds = of_file(file).map(|line| line.kind).collect::<Vec<_>>();

        assert_eq!(kinds[0], LineKind::Metadata);
        assert_eq!(kinds[1], LineKind::Metadata);
        assert_eq!(kinds[2], LineKind::FileHeader);
        assert_eq!(kinds[3], LineKind::FileHeader);
        assert_eq!(kinds[4], LineKind::HunkHeader);
        assert_eq!(kinds[7], LineKind::HunkHeader);
        assert_eq!(
            kinds[5],
            LineKind::Record {
                address: first_records[0],
                kind: RecordKind::Deletion,
            }
        );
        assert_eq!(
            kinds[8],
            LineKind::Record {
                address: first_records[1],
                kind: RecordKind::Deletion,
            }
        );
    }
}
