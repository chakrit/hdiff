//! The lines a document displays, in input order: what each line shows, and the safe text
//! it displays. Finite output writes them directly, and page rows retain them. Rules:
//! docs/spec/architecture.md#lossless-and-safe-rendering.

use std::iter;

use crate::{
    document::{
        AddressedRecord, DiffDocument, DiffFile, FileBody, FileView, RecordAddress, RecordKind,
        UnifiedBody,
    },
    headers::GitHeader,
    one_of::OneOf,
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
enum LineEnding {
    None,
    Lf,
    CrLf,
}

impl LineEnding {
    /// The ending `bytes` end with.
    fn of(bytes: &[u8]) -> Self {
        if bytes.ends_with(b"\r\n") {
            Self::CrLf
        } else if bytes.ends_with(b"\n") {
            Self::Lf
        } else {
            Self::None
        }
    }

    /// The ending as written.
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
    match document {
        DiffDocument::Files(files) => {
            OneOf::First(files.views().into_iter().flat_map(of_file_view))
        }
        DiffDocument::Text(text) => OneOf::Second(of_text(text)),
    }
}

/// The lines `view` displays: its leading text, its file, then its trailing text.
pub fn of_file_view(view: FileView<'_>) -> impl Iterator<Item = DisplayLine<'_>> {
    of_text(view.leading)
        .chain(of_file(view.file))
        .chain(of_text(view.trailing))
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
fn of_file(file: &DiffFile) -> impl Iterator<Item = DisplayLine<'_>> {
    match &file.body {
        FileBody::Metadata(git) => OneOf::First(git_header_lines(git)),
        FileBody::Unified(body) => OneOf::Second(unified_lines(body)),
    }
}

/// The lines of a unified body: its Git header and metadata when it has one, its file
/// headers, then each hunk's header and records.
fn unified_lines(body: &UnifiedBody) -> impl Iterator<Item = DisplayLine<'_>> {
    let git = body.git.iter().flat_map(git_header_lines);
    let file_headers = body.headers.iter().map(|file_header| DisplayLine {
        kind: LineKind::FileHeader,
        source: &file_header.line,
    });
    let hunks = body.addressed_hunks().flat_map(|hunk| {
        let hunk_header = DisplayLine {
            kind: LineKind::HunkHeader,
            source: hunk.header(),
        };
        let records = hunk.records().map(|AddressedRecord { address, record }| {
            let line_kind = LineKind::Record {
                address,
                kind: record.kind,
            };
            DisplayLine {
                kind: line_kind,
                source: &record.payload,
            }
        });
        iter::once(hunk_header).chain(records)
    });

    git.chain(file_headers).chain(hunks)
}

/// The Git header's `diff --git` line, then its metadata lines.
fn git_header_lines(git: &GitHeader) -> impl Iterator<Item = DisplayLine<'_>> {
    iter::once(&git.line)
        .chain(&git.metadata)
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
        let addresses = body
            .addressed_hunks()
            .map(|hunk| {
                let records = hunk
                    .records()
                    .map(|record| record.address)
                    .collect::<Vec<_>>();
                <[_; 2]>::try_from(records).expect("each hunk holds two records")
            })
            .collect::<Vec<_>>();
        let [
            [first_deletion, first_addition],
            [second_deletion, second_addition],
        ] = <[_; 2]>::try_from(addresses).expect("the fixture holds two hunks");
        let record = |address, kind| LineKind::Record { address, kind };

        let kinds = of_file(file).map(|line| line.kind).collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [
                LineKind::Metadata,
                LineKind::Metadata,
                LineKind::FileHeader,
                LineKind::FileHeader,
                LineKind::HunkHeader,
                record(first_deletion, RecordKind::Deletion),
                record(first_addition, RecordKind::Addition),
                LineKind::HunkHeader,
                record(second_deletion, RecordKind::Deletion),
                record(second_addition, RecordKind::Addition),
            ]
        );
    }
}
