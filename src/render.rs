use crate::{
    document::{DiffDocument, DiffFile, FileBody, RecordAddress, RecordKind, Section, Side},
    headers::GitHeader,
    source_text::SourceLine,
};

/// What a rendered line shows.
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

    fn marker(self) -> &'static str {
        self.record_kind().map_or("", RecordKind::marker)
    }
}

/// A byte range of a rendered line's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadSpan {
    start: usize,
    end: usize,
}

impl PayloadSpan {
    /// The bytes from `start` up to `end`.
    ///
    /// Panics when `start` is after `end`.
    pub fn new(start: usize, end: usize) -> Self {
        assert!(start <= end, "a payload span starts before it ends");

        Self { start, end }
    }

    pub fn start(self) -> usize {
        self.start
    }

    pub fn end(self) -> usize {
        self.end
    }

    pub fn len(self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The same span `offset` bytes later.
    pub fn offset(self, offset: usize) -> Self {
        Self {
            start: self.start + offset,
            end: self.end + offset,
        }
    }

    /// The span from this span's start to the end of `later`, which ends no earlier.
    pub fn through(self, later: Self) -> Self {
        Self::new(self.start, later.end)
    }
}

/// A line's sanitized text: its record marker, its payload, and its source line ending.
#[derive(Debug, PartialEq, Eq)]
pub struct RenderedLine {
    text: Box<str>,
    kind: LineKind,
}

impl RenderedLine {
    fn new(kind: LineKind, line: &SourceLine) -> Self {
        let ending = LineEnding::of(line.bytes());
        let marker = kind.marker();
        let payload = line.sanitized();
        let mut text = String::with_capacity(marker.len() + payload.len() + ending.text().len());

        text.push_str(marker);
        text.push_str(payload);
        text.push_str(ending.text());

        Self {
            text: text.into_boxed_str(),
            kind,
        }
    }

    pub fn kind(&self) -> LineKind {
        self.kind
    }

    /// The whole line as finite output writes it.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The text between the record marker and the line ending. Sanitized text holds no
    /// line ending, so the line's text ends in exactly the source's.
    pub fn payload(&self) -> &str {
        let start = self.kind.marker().len();
        let end = self.text.len() - LineEnding::of(self.text.as_bytes()).text().len();

        &self.text[start..end]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineEnding {
    None,
    Lf,
    CrLf,
}

impl LineEnding {
    fn of(source: &[u8]) -> Self {
        if source.ends_with(b"\r\n") {
            Self::CrLf
        } else if source.ends_with(b"\n") {
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

/// The document as finite output: every rendered line, in input order.
pub fn render_unified(document: &DiffDocument) -> Vec<u8> {
    let lines = document.sections.iter().flat_map(|section| match section {
        Section::File(file) => render_file_lines(file),
        Section::Text(lines) => render_text_lines(lines),
    });

    let mut output = Vec::new();
    for line in lines {
        output.extend_from_slice(line.text().as_bytes());
    }
    output
}

pub fn render_text_lines<'a>(lines: impl IntoIterator<Item = &'a SourceLine>) -> Vec<RenderedLine> {
    lines
        .into_iter()
        .map(|line| RenderedLine::new(LineKind::Text, line))
        .collect()
}

pub fn render_file_lines(file: &DiffFile) -> Vec<RenderedLine> {
    let header_lines = |header: &GitHeader| {
        std::iter::once(&header.line)
            .chain(&header.metadata)
            .map(|line| RenderedLine::new(LineKind::Metadata, line))
            .collect::<Vec<_>>()
    };

    match &file.body {
        FileBody::Metadata(header) => header_lines(header),
        FileBody::Unified(file) => {
            let git_header = file.git.iter().flat_map(header_lines);
            let file_headers = [&file.old, &file.new]
                .map(|header| RenderedLine::new(LineKind::FileHeader, &header.line));
            let hunks = file
                .hunks
                .iter()
                .enumerate()
                .flat_map(|(hunk_index, hunk)| {
                    let header = RenderedLine::new(LineKind::HunkHeader, &hunk.header);
                    let records = hunk.records.iter().enumerate().map(move |(index, record)| {
                        let kind = LineKind::Record {
                            address: RecordAddress::new(hunk_index, index),
                            kind: record.kind,
                        };
                        RenderedLine::new(kind, &record.payload)
                    });
                    std::iter::once(header).chain(records)
                });

            git_header.chain(file_headers).chain(hunks).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LineKind, render_file_lines, render_unified};
    use crate::{
        document::{RecordAddress, RecordKind},
        parser::parse_patch,
    };

    #[test]
    fn sanitizes_surrounding_text_and_preserves_line_endings() {
        let input = b"\x1b[31mmessage\x1b[0m\r\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n\x1b]8;;https://bad\x07footer\xff";
        let document = parse_patch(input).expect("diff with surrounding text");

        let rendered = render_unified(&document);

        assert_eq!(
            rendered,
            "message\r\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\nfooter\u{fffd}".as_bytes()
        );
    }

    #[test]
    fn renders_unified_structure_in_source_order() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        let document = parse_patch(input).expect("valid diff");

        let rendered = render_unified(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn classifies_metadata_file_headers_hunks_and_records() {
        let input = b"diff --git a/file b/file\nindex 1..2 100644\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n@@ -3 +3 @@\n-before\n+after\n";
        let document = parse_patch(input).expect("valid Git diff with two hunks");

        let lines = render_file_lines(document.files().next().expect("first file"));
        let kinds = lines.iter().map(|line| line.kind()).collect::<Vec<_>>();

        assert_eq!(kinds[0], LineKind::Metadata);
        assert_eq!(kinds[1], LineKind::Metadata);
        assert_eq!(kinds[2], LineKind::FileHeader);
        assert_eq!(kinds[3], LineKind::FileHeader);
        assert_eq!(kinds[4], LineKind::HunkHeader);
        assert_eq!(kinds[7], LineKind::HunkHeader);
        assert_eq!(
            kinds[5],
            LineKind::Record {
                address: RecordAddress::new(0, 0),
                kind: RecordKind::Deletion,
            }
        );
        assert_eq!(
            kinds[8],
            LineKind::Record {
                address: RecordAddress::new(1, 0),
                kind: RecordKind::Deletion,
            }
        );
    }

    #[test]
    fn payloads_exclude_the_record_marker_and_line_ending() {
        let input =
            b"before\r\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\r\n+new\n\\ No newline at end of file";
        let document = parse_patch(input).expect("valid diff");
        let view = document.file_views().next().expect("first file");

        let text = super::render_text_lines(view.leading);
        let lines = render_file_lines(view.file);
        let payloads = lines.iter().map(|line| line.payload()).collect::<Vec<_>>();

        assert_eq!(text[0].payload(), "before");
        assert_eq!(
            payloads,
            [
                "--- a/file",
                "+++ b/file",
                "@@ -1 +1 @@",
                "old",
                "new",
                "\\ No newline at end of file"
            ]
        );
    }

    #[test]
    fn renders_git_fixture_in_original_order() {
        let input = include_bytes!("../tests/fixtures/git-multiline.patch");

        let document = parse_patch(input).expect("valid Git fixture");
        let rendered = render_unified(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn renders_metadata_only_git_fixture_in_original_order() {
        let input = include_bytes!("../tests/fixtures/git-rename-only.patch");

        let document = parse_patch(input).expect("valid metadata-only Git fixture");
        let rendered = render_unified(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn renders_colored_git_fixture_without_terminal_controls() {
        let input = include_bytes!("../tests/fixtures/git-multiline-coloured.patch");

        let document = parse_patch(input).expect("valid colored Git fixture");
        let rendered = render_unified(&document);

        assert_eq!(
            rendered,
            include_bytes!("../tests/fixtures/git-multiline.patch")
        );
    }

    #[test]
    fn removes_control_sequences_before_a_record_marker() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n\x1b]8;;https://bad\x07+new\n";

        let document = parse_patch(input).expect("valid diff with a controlled record");
        let rendered = render_unified(&document);

        assert_eq!(
            rendered,
            b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n"
        );
    }

    #[test]
    fn preserves_printable_text_and_neutralizes_terminal_controls() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+tab\t e\xcc\x81 \xe7\x95\x8c \x1b[31mred\x1b[0m \x1b]8;;https://bad\x07link\x1bPbad\x1b\\\x01\n";
        let document = parse_patch(input).expect("valid diff");

        let rendered = render_unified(&document);
        let text = String::from_utf8(rendered).expect("safe UTF-8 output");

        assert!(text.contains("tab\t e\u{301} \u{754c} red link"));
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{1}'));
        assert!(!text.contains("https://bad"));
    }

    #[test]
    fn replaces_invalid_utf8_without_losing_following_text() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+\xffnew\n";
        let document = parse_patch(input).expect("valid diff");

        let rendered = render_unified(&document);
        let text = String::from_utf8(rendered).expect("safe UTF-8 output");

        assert!(text.contains("+\u{fffd}new\n"));
    }

    #[test]
    fn preserves_raw_no_newline_marker_without_adding_a_line_ending() {
        let input =
            b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file";
        let document = parse_patch(input).expect("valid diff with raw marker");

        let rendered = render_unified(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn renders_one_file_with_the_same_safe_output_as_the_document_renderer() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-\x1b[31mold\x1b[0m\n+new\n";
        let document = parse_patch(input).expect("valid diff");

        let rendered = render_file_lines(document.files().next().expect("first file"))
            .iter()
            .flat_map(|line| line.text().bytes())
            .collect::<Vec<_>>();

        assert_eq!(rendered, render_unified(&document));
        assert!(!rendered.contains(&b'\x1b'));
    }
}
