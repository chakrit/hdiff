use std::fmt;

use crate::{
    document::{
        DiffDocument, DiffFile, FileBody, Hunk, Record, RecordKind, Section, Side, UnifiedBody,
    },
    file_identity::FileIdentity,
    headers::{FileHeader, GitHeader, HeaderPath},
    source_text::SourceLine,
};

/// Why input was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Nonempty viewer input holds no file diff.
    NoDiff,
    /// A recognized diff section is malformed at a 1-based input line.
    Malformed { line: usize, reason: Malformed },
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDiff => formatter.write_str("input contains no diff"),
            Self::Malformed { line, reason } => {
                write!(formatter, "line {line}: {}", reason.label())
            }
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Malformed {
    CombinedDiff,
    UnexpectedDiffHeader,
    UnexpectedFileHeaderInHunk,
    MissingNewFileHeader,
    ExpectedNewFileHeader,
    InvalidHunkHeader,
    TruncatedHunk,
    ExcessRecords,
    FileWithoutHunks,
    InvalidBinaryPatchHeader,
    TruncatedBinaryPatch,
    MissingBinaryPatchPayload,
    MissingBinaryPatchBlock,
}

impl Malformed {
    pub fn label(self) -> &'static str {
        match self {
            Self::CombinedDiff => "combined diffs are not supported",
            Self::UnexpectedDiffHeader => "unexpected diff header",
            Self::UnexpectedFileHeaderInHunk => "unexpected file header in hunk",
            Self::MissingNewFileHeader => "missing new file header",
            Self::ExpectedNewFileHeader => "expected new file header",
            Self::InvalidHunkHeader => "invalid hunk header",
            Self::TruncatedHunk => "truncated hunk",
            Self::ExcessRecords => "hunk records exceed declared counts",
            Self::FileWithoutHunks => "file has no hunks",
            Self::InvalidBinaryPatchHeader => "invalid binary patch header",
            Self::TruncatedBinaryPatch => "truncated binary patch",
            Self::MissingBinaryPatchPayload => "missing binary patch payload",
            Self::MissingBinaryPatchBlock => "missing binary patch block",
        }
    }
}

/// Parses viewer input, a patch, rejecting nonempty input that holds no file diff.
pub fn parse_patch(input: &[u8]) -> Result<DiffDocument, ParseError> {
    let document = parse_sections(input)?;

    match !input.is_empty() && document.files().next().is_none() {
        true => Err(ParseError::NoDiff),
        false => Ok(document),
    }
}

/// Parses `diff` output, which may hold only surrounding text such as `Binary files … differ`.
pub fn parse_comparison(input: &[u8]) -> Result<DiffDocument, ParseError> {
    parse_sections(input)
}

/// Parses any diff output into its surrounding text and file sections.
fn parse_sections(input: &[u8]) -> Result<DiffDocument, ParseError> {
    let mut lines = Lines::new(input);
    let mut sections = Vec::new();

    while let Some(line) = lines.next() {
        let start = lines.at_last_line(line_start(line.sanitized()))?;
        let section = match start {
            LineStart::Text => Section::Text(parse_text(line, &mut lines)),
            LineStart::GitHeader => parse_git_section(line, &mut lines)?,
            LineStart::OldFileHeader(field) => {
                let old = FileHeader {
                    path: HeaderPath::of_field(field),
                    line,
                };
                Section::File(parse_unified(None, old, &mut lines)?)
            }
        };
        sections.push(section);
    }

    Ok(DiffDocument::new(sections.into_boxed_slice()))
}

// Sections

/// How a line outside a file section starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineStart<'a> {
    Text,
    GitHeader,
    /// A `---` line, with its field after the prefix.
    OldFileHeader(&'a str),
}

fn line_start(text: &str) -> Result<LineStart<'_>, Malformed> {
    reject_unsupported_header(text)?;

    if text.starts_with("diff --git ") {
        return Ok(LineStart::GitHeader);
    }
    let start = header_field(text, Side::Old).map_or(LineStart::Text, LineStart::OldFileHeader);
    Ok(start)
}

/// The field of a `---` (old) or `+++` (new) file header line.
fn header_field(text: &str, side: Side) -> Option<&str> {
    let prefix = match side {
        Side::Old => "--- ",
        Side::New => "+++ ",
    };

    text.strip_prefix(prefix)
}

fn reject_unsupported_header(text: &str) -> Result<(), Malformed> {
    if text.starts_with("diff --cc ")
        || text.starts_with("diff --combined ")
        || text.starts_with("@@@")
    {
        return Err(Malformed::CombinedDiff);
    }
    if text.starts_with("@@") || text.starts_with("+++ ") {
        return Err(Malformed::UnexpectedDiffHeader);
    }

    Ok(())
}

fn parse_text(first: SourceLine, lines: &mut Lines<'_>) -> Box<[SourceLine]> {
    let rest = std::iter::from_fn(|| lines.next_if(|text| line_start(text) == Ok(LineStart::Text)));

    std::iter::once(first).chain(rest).collect()
}

/// A `diff --git` section: unified when `---` / `+++` headers follow its metadata.
fn parse_git_section(header: SourceLine, lines: &mut Lines<'_>) -> Result<Section, ParseError> {
    let mut metadata = std::iter::from_fn(|| lines.next_if(is_git_metadata)).collect::<Vec<_>>();
    if let Some(binary_patch) = lines.next_if(|text| text == "GIT binary patch") {
        metadata.push(binary_patch);
        metadata.extend(parse_binary_patch(lines)?);
    }

    let git = GitHeader {
        line: header,
        metadata: metadata.into_boxed_slice(),
    };
    let file = match lines.next_file_header(Side::Old) {
        Some(old) => parse_unified(Some(git), old, lines)?,
        None => DiffFile {
            identity: FileIdentity::of_metadata(&git),
            body: FileBody::Metadata(git),
        },
    };
    Ok(Section::File(file))
}

fn is_git_metadata(text: &str) -> bool {
    [
        "old mode ",
        "new mode ",
        "deleted file mode ",
        "new file mode ",
        "copy from ",
        "copy to ",
        "rename from ",
        "rename to ",
        "similarity index ",
        "dissimilarity index ",
        "index ",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
        || (text.starts_with("Binary files ") && text.ends_with(" differ"))
}

/// The `literal` and `delta` blocks after `GIT binary patch`, each with its payload and
/// blank separator.
fn parse_binary_patch(lines: &mut Lines<'_>) -> Result<Vec<SourceLine>, ParseError> {
    let mut patch = Vec::new();
    let mut blocks = 0;

    while let Some((header, valid_size)) =
        lines.next_if_map(|text| binary_block_size(text).map(|size| size.parse::<usize>().is_ok()))
    {
        if !valid_size {
            return Err(lines.last_line_error(Malformed::InvalidBinaryPatchHeader));
        }
        patch.push(header);

        let payload_start = patch.len();
        while let Some(line) = lines.next_if(|text| !text.is_empty()) {
            let text = line.sanitized();
            lines.at_last_line(reject_unsupported_header(text))?;
            if text.starts_with("diff --") || text.starts_with("--- ") {
                return Err(lines.last_line_error(Malformed::TruncatedBinaryPatch));
            }
            patch.push(line);
        }
        if patch.len() == payload_start {
            return Err(lines.next_line_error(Malformed::MissingBinaryPatchPayload));
        }
        patch.extend(lines.next_if(str::is_empty));
        blocks += 1;
    }

    match blocks {
        0 => Err(lines.next_line_error(Malformed::MissingBinaryPatchBlock)),
        _ => Ok(patch),
    }
}

fn binary_block_size(text: &str) -> Option<&str> {
    let literal = text.strip_prefix("literal ");
    let delta = text.strip_prefix("delta ");

    literal.or(delta)
}

/// A section with `---` / `+++` headers, after its Git header when it has one.
fn parse_unified(
    git: Option<GitHeader>,
    old: FileHeader,
    lines: &mut Lines<'_>,
) -> Result<DiffFile, ParseError> {
    let Some(new) = lines.next_file_header(Side::New) else {
        let error = match lines.peek() {
            None => lines.last_line_error(Malformed::MissingNewFileHeader),
            Some(_) => lines.next_line_error(Malformed::ExpectedNewFileHeader),
        };
        return Err(error);
    };

    let mut hunks = Vec::new();
    while let Some((header, declared)) = lines.next_if_map(|text| {
        text.starts_with("@@ ")
            .then(|| SideCounts::of_header(text).ok_or(Malformed::InvalidHunkHeader))
    }) {
        let expected = lines.at_last_line(declared)?;
        hunks.push(parse_hunk(header, expected, lines)?);
    }
    if hunks.is_empty() {
        return Err(lines.next_line_error(Malformed::FileWithoutHunks));
    }

    let identity = FileIdentity::of_unified(git.as_ref(), &old, &new);
    Ok(DiffFile {
        identity,
        body: FileBody::Unified(Box::new(UnifiedBody {
            git,
            old,
            new,
            hunks: hunks.into_boxed_slice(),
        })),
    })
}

// Hunks

const NO_NEWLINE_MARKER: &str = "\\ No newline at end of file";

/// The records after a hunk `header` that declares `expected` counts.
fn parse_hunk(
    header: SourceLine,
    expected: SideCounts,
    lines: &mut Lines<'_>,
) -> Result<Hunk, ParseError> {
    let mut seen = SideCounts::default();
    let mut records = Vec::new();
    while let Some(line) =
        lines.next_if(|text| seen != expected || text.starts_with(NO_NEWLINE_MARKER))
    {
        let record = lines.at_last_line(parse_record(line))?;
        seen.count(record.kind);
        records.push(record);
    }

    if seen != expected {
        return Err(lines.next_line_error(Malformed::TruncatedHunk));
    }
    if lines
        .peek()
        .is_some_and(|line| is_excess_record(line.sanitized()))
    {
        return Err(lines.next_line_error(Malformed::ExcessRecords));
    }
    Ok(Hunk {
        header,
        records: records.into_boxed_slice(),
    })
}

fn parse_record(line: SourceLine) -> Result<Record, Malformed> {
    let marked = line.split_first().and_then(|(marker, payload)| {
        let kind = RecordKind::of_marker(marker)?;
        Some(Record { kind, payload })
    });
    if let Some(record) = marked {
        return Ok(record);
    }

    let text = line.sanitized();
    reject_unsupported_header(text)?;
    if text.starts_with("diff --git ") {
        return Err(Malformed::UnexpectedFileHeaderInHunk);
    }
    Ok(Record {
        kind: RecordKind::Raw,
        payload: line,
    })
}

/// A marked record after a hunk's declared counts are complete; a `---` line starts the
/// next file instead.
fn is_excess_record(text: &str) -> bool {
    let marked = text
        .chars()
        .next()
        .and_then(RecordKind::of_marker)
        .is_some();

    marked && !text.starts_with("--- ")
}

/// Old-side and new-side line counts of a hunk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SideCounts {
    old: usize,
    new: usize,
}

impl SideCounts {
    /// The counts an `@@ -old +new @@` header declares.
    fn of_header(header: &str) -> Option<Self> {
        let mut parts = header.split_whitespace();
        (parts.next()? == "@@").then_some(())?;
        let old = range_count(parts.next()?, '-')?;
        let new = range_count(parts.next()?, '+')?;
        (parts.next()? == "@@").then_some(())?;

        Some(Self { old, new })
    }

    fn count(&mut self, kind: RecordKind) {
        self.old += usize::from(kind.on_side(Side::Old));
        self.new += usize::from(kind.on_side(Side::New));
    }
}

fn range_count(range: &str, prefix: char) -> Option<usize> {
    let range = range.strip_prefix(prefix)?;
    let (start, count) = match range.split_once(',') {
        Some((start, count)) => (start, count),
        None => (range, "1"),
    };
    start.parse::<usize>().ok()?;

    count.parse().ok()
}

// Reading lines

/// The input's lines, each decoded once when first read.
struct Lines<'a> {
    rest: &'a [u8],
    peeked: Option<SourceLine>,
    taken: usize,
}

impl<'a> Lines<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            rest: input,
            peeked: None,
            taken: 0,
        }
    }

    fn peek(&mut self) -> Option<&SourceLine> {
        if self.peeked.is_none() && !self.rest.is_empty() {
            let end = self
                .rest
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(self.rest.len(), |line_feed| line_feed + 1);
            let (line, rest) = self.rest.split_at(end);
            self.rest = rest;
            self.peeked = Some(SourceLine::decode(line));
        }

        self.peeked.as_ref()
    }

    fn next(&mut self) -> Option<SourceLine> {
        self.peek();
        let line = self.peeked.take()?;

        self.taken += 1;
        Some(line)
    }

    /// Takes the next line when its sanitized text satisfies `accepts`.
    fn next_if(&mut self, accepts: impl FnOnce(&str) -> bool) -> Option<SourceLine> {
        self.next_if_map(|text| accepts(text).then_some(()))
            .map(|(line, ())| line)
    }

    /// Takes the next line when `extract` reads a value from its sanitized text, with
    /// that value.
    fn next_if_map<T>(
        &mut self,
        extract: impl FnOnce(&str) -> Option<T>,
    ) -> Option<(SourceLine, T)> {
        let extracted = self.peek().and_then(|line| extract(line.sanitized()))?;

        self.next().map(|line| (line, extracted))
    }

    /// Takes the next line when it is `side`'s file header.
    fn next_file_header(&mut self, side: Side) -> Option<FileHeader> {
        self.next_if_map(|text| header_field(text, side).map(HeaderPath::of_field))
            .map(|(line, path)| FileHeader { line, path })
    }

    /// `result`, with a malformation located at the line most recently taken.
    fn at_last_line<T>(&self, result: Result<T, Malformed>) -> Result<T, ParseError> {
        result.map_err(|reason| self.last_line_error(reason))
    }

    /// An error at the line most recently taken.
    fn last_line_error(&self, reason: Malformed) -> ParseError {
        ParseError::Malformed {
            line: self.taken,
            reason,
        }
    }

    /// An error at the line that would be taken next, or just past the end.
    fn next_line_error(&self, reason: Malformed) -> ParseError {
        ParseError::Malformed {
            line: self.taken + 1,
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Malformed, ParseError, parse_comparison, parse_patch};
    use crate::{
        document::{DiffDocument, FileBody, Hunk, RecordKind, UnifiedBody},
        render::render_unified,
    };

    fn malformed(line: usize, reason: Malformed) -> Result<DiffDocument, ParseError> {
        Err(ParseError::Malformed { line, reason })
    }

    #[test]
    fn preserves_surrounding_text_in_source_order() {
        let input = b"commit message\r\n\r\n--- a/one\n+++ b/one\n@@ -1 +1 @@\n-old\n+new\n\nbetween files\n--- a/two\n+++ b/two\n@@ -1 +1 @@\n-before\n+after\ntrailing text";

        let document = parse_patch(input).expect("diff with surrounding text");

        assert_eq!(render_unified(&document), input);
    }

    #[test]
    fn rejects_nonempty_text_only_input() {
        for input in [b"ordinary text\n".as_slice(), b" \t\r\n", b"\n"] {
            assert_eq!(parse_patch(input), Err(ParseError::NoDiff));
        }
        assert_eq!(parse_patch(b""), Ok(DiffDocument::default()));
    }

    #[test]
    fn keeps_comparison_output_that_holds_only_text() {
        let input = b"Binary files before.bin and after.bin differ\n";

        let document = parse_comparison(input).expect("text-only comparison output");

        assert_eq!(render_unified(&document), input);
    }

    #[test]
    fn rejects_malformed_and_combined_sections_after_valid_diff() {
        let valid = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        for (suffix, line, reason) in [
            ("@@ malformed\n", 6, Malformed::InvalidHunkHeader),
            ("@@@ -1,1 -1,1 +1,1 @@@\n", 6, Malformed::CombinedDiff),
            ("diff --cc file\n", 6, Malformed::CombinedDiff),
            ("diff --combined file\n", 6, Malformed::CombinedDiff),
            (
                "--- a/second\nmissing header\n",
                7,
                Malformed::ExpectedNewFileHeader,
            ),
            ("+++ b/second\n", 6, Malformed::ExcessRecords),
        ] {
            let input = format!("{valid}{suffix}");
            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(line, reason),
                "{suffix}"
            );
        }
    }

    #[test]
    fn rejects_diff_headers_inside_incomplete_hunks() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n@@ malformed\n-old\n+new\n";

        assert_eq!(
            parse_patch(input),
            malformed(4, Malformed::UnexpectedDiffHeader)
        );
    }

    #[test]
    fn rejects_records_exceeding_hunk_counts() {
        let valid = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        for suffix in ["-extra\n", "+extra\n", " context\n"] {
            let input = format!("{valid}{suffix}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(6, Malformed::ExcessRecords),
                "{suffix}"
            );
        }
    }

    #[test]
    fn keeps_text_separate_from_metadata_only_files() {
        let input =
            b"message\ndiff --git a/file b/file\nold mode 100644\nnew mode 100755\nfooter\n";
        let document = parse_patch(input).expect("mode-only diff with text");
        let views = document.file_views().collect::<Vec<_>>();
        let FileBody::Metadata(header) = &views[0].file.body else {
            panic!("mode-only diff is metadata");
        };

        assert_eq!(views.len(), 1);
        assert_eq!(header.metadata.len(), 2);
        assert_eq!(views[0].leading[0].sanitized(), "message");
        assert_eq!(views[0].trailing[0].sanitized(), "footer");
        assert_eq!(render_unified(&document), input);
    }

    #[test]
    fn keeps_binary_payload_and_surrounding_text_in_order() {
        let input = b"message\ndiff --git a/image b/image\nindex 1..2 100644\nGIT binary patch\nliteral 3\nKcmZQzWC8#H2LJ>B\n\nliteral 0\nHcmV?d00001\n\nfooter\n";
        let document = parse_patch(input).expect("binary diff with text");
        let view = document.file_views().next().expect("binary file");
        let FileBody::Metadata(header) = &view.file.body else {
            panic!("binary diff is metadata");
        };

        assert_eq!(view.trailing[0].sanitized(), "footer");
        assert!(
            header
                .metadata
                .iter()
                .any(|line| line.sanitized() == "KcmZQzWC8#H2LJ>B")
        );
        assert_eq!(render_unified(&document), input);
    }

    /// Every file's unified body, in input order; each fixture file must be unified.
    fn unified_bodies(document: &DiffDocument) -> Vec<&UnifiedBody> {
        document
            .files()
            .map(|file| match &file.body {
                FileBody::Unified(body) => body.as_ref(),
                FileBody::Metadata(_) => panic!("fixture section should be unified"),
            })
            .collect()
    }

    /// The first hunk of a document holding one unified file.
    fn first_hunk(document: &DiffDocument) -> &Hunk {
        let [body] = unified_bodies(document)[..] else {
            panic!("fixture should hold one file");
        };

        &body.hunks[0]
    }

    /// Each body's first-hunk record count.
    fn first_hunk_record_counts(bodies: &[&UnifiedBody]) -> Vec<usize> {
        bodies
            .iter()
            .map(|body| body.hunks[0].records.len())
            .collect()
    }

    #[test]
    fn parses_file_hunk_and_records() {
        let input = "--- a/readme.txt\n+++ b/readme.txt\n@@ -1 +1 @@\n-old\n+new\n";

        let document = parse_patch(input.as_bytes()).expect("valid unified diff");
        let bodies = unified_bodies(&document);

        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].hunks.len(), 1);
        assert_eq!(first_hunk_record_counts(&bodies), [2]);
    }

    #[test]
    fn parses_git_fixture_with_extended_headers_and_multiline_hunks() {
        let input = include_bytes!("../tests/fixtures/git-multiline.patch");

        let document = parse_patch(input).expect("valid Git fixture");
        let bodies = unified_bodies(&document);
        let metadata_counts = bodies
            .iter()
            .map(|body| body.git.as_ref().map(|header| header.metadata.len()))
            .collect::<Vec<_>>();

        assert_eq!(metadata_counts, [Some(2), Some(1)]);
        assert_eq!(first_hunk_record_counts(&bodies), [3, 4]);
    }

    #[test]
    fn parses_colored_git_fixture_with_multiline_hunks() {
        let input = include_bytes!("../tests/fixtures/git-multiline-coloured.patch");

        let document = parse_patch(input).expect("valid colored Git fixture");

        assert_eq!(first_hunk_record_counts(&unified_bodies(&document)), [3, 4]);
    }

    #[test]
    fn parses_git_metadata_without_a_file_header() {
        let input = include_bytes!("../tests/fixtures/git-metadata-without-file-header.patch");

        let document = parse_patch(input).expect("valid metadata-only Git fixture");

        assert!(matches!(
            document.files().next().expect("first file").body,
            FileBody::Metadata(_)
        ));
    }

    #[test]
    fn parses_metadata_only_git_fixture() {
        let input = include_bytes!("../tests/fixtures/git-rename-only.patch");

        let document = parse_patch(input).expect("valid metadata-only Git fixture");

        assert_eq!(document.files().count(), 1);
        assert!(matches!(
            document.files().next().expect("first file").body,
            FileBody::Metadata(_)
        ));
    }

    #[test]
    fn rejects_truncated_hunk() {
        let input = "--- a/readme.txt\n+++ b/readme.txt\n@@ -1,2 +1,2 @@\n-old\n";

        assert_eq!(
            parse_patch(input.as_bytes()),
            malformed(5, Malformed::TruncatedHunk)
        );
    }

    #[test]
    fn preserves_invalid_utf8_with_lossy_display_text() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+\xffnew\n";

        let document = parse_patch(input).expect("structurally valid diff");
        let payload = &first_hunk(&document).records[1].payload;

        assert_eq!(payload.bytes(), b"\xffnew\n");
        assert_eq!(payload.sanitized(), "\u{fffd}new");
    }

    #[test]
    fn preserves_crlf_in_source_bytes() {
        let input = b"--- a/file\r\n+++ b/file\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n";

        let document = parse_patch(input).expect("valid CRLF diff");
        let payload = &first_hunk(&document).records[0].payload;

        assert_eq!(payload.bytes(), b"old\r\n");
        assert_eq!(payload.sanitized(), "old");
    }

    /// Contract: docs/spec/architecture.md#lossless-and-safe-rendering (parsing uses ANSI-free
    /// text): the payload starts after the marker the sanitized text shows.
    #[test]
    fn cuts_record_payloads_after_the_marker_sanitizing_recognizes() {
        for (line, payload) in [
            (b"\xc2\x85-old\n".as_slice(), b"old\n".as_slice()),
            (b"\x1b]x\x1b\x07a\x1b\\-old\n", b"old\n"),
        ] {
            let input = [b"--- a/file\n+++ b/file\n@@ -1 +0,0 @@\n".as_slice(), line].concat();

            let document = parse_patch(&input).expect("valid deletion");
            let record = &first_hunk(&document).records[0];

            assert_eq!(record.kind, RecordKind::Deletion);
            assert_eq!(record.payload.bytes(), payload, "{line:?}");
        }
    }

    #[test]
    fn parses_deletion_payload_that_looks_like_a_file_header() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +0,0 @@\n--- filename\n";

        let document = parse_patch(input).expect("valid deletion record");
        let record = &first_hunk(&document).records[0];

        assert_eq!(record.kind, RecordKind::Deletion);
        assert_eq!(record.payload.sanitized(), "-- filename");
    }
}
