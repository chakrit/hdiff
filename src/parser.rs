//! Viewer input and `diff` output read into a document: surrounding text, file
//! sections, hunks and their records. Rules: docs/spec/architecture.md#input-contract.

mod binary_patch;

use std::{fmt, iter, mem};

use self::binary_patch::{is_binary_patch_header, parse_binary_patch};
use crate::{
    document::{
        ContextMarker, DiffDocument, DiffFile, FileBody, FileSection, Hunk, MetadataBody, Record,
        RecordKind, UnifiedBody,
    },
    file_identity::FileIdentity,
    headers::{
        FileHeader, GitHeader, GitMetadata, HeaderPath, decimal_count, file_header_field,
        git_header_names, is_combined_diff_header,
    },
    non_empty::NonEmpty,
    side::{BySide, Side},
    source_text::SourceLine,
};

/// Why input was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Nonempty viewer input holds no file diff.
    NoDiff,
    /// A recognized diff section is malformed at a 1-based input line.
    Malformed { line: usize, reason: Malformed },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDiff => formatter.write_str("input contains no diff"),
            Self::Malformed { line, reason } => {
                write!(formatter, "line {line}: {}", reason.label())
            }
        }
    }
}

impl std::error::Error for Error {}

/// How a recognized diff section breaks the unified diff or Git patch format, grouped by
/// the part of the input it breaks: section starts, file headers, hunks, Git metadata,
/// then binary patches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Malformed {
    CombinedDiff,
    UnexpectedDiffHeader,

    MissingNewFileHeader,
    ExpectedNewFileHeader,
    FileWithoutHunks,

    InvalidHunkHeader,
    TruncatedHunk,
    ExcessRecords,
    UnexpectedFileHeaderInHunk,
    UnmarkedHunkLine,
    MisplacedNoNewlineMarker,

    MissingGitMetadata,

    InvalidBinaryPatchHeader,
    MissingBinaryPatchBlock,
    MissingBinaryPatchPayload,
    InvalidBinaryPatchPayload,
    TruncatedBinaryPatch,
}

impl Malformed {
    /// The reason as error messages state it.
    pub fn label(self) -> &'static str {
        match self {
            Self::CombinedDiff => "combined diffs are not supported",
            Self::UnexpectedDiffHeader => "unexpected diff header",

            Self::MissingNewFileHeader => "missing new file header",
            Self::ExpectedNewFileHeader => "expected new file header",
            Self::FileWithoutHunks => "file has no hunks",

            Self::InvalidHunkHeader => "invalid hunk header",
            Self::TruncatedHunk => "truncated hunk",
            Self::ExcessRecords => "hunk records exceed declared counts",
            Self::UnexpectedFileHeaderInHunk => "unexpected file header in hunk",
            Self::UnmarkedHunkLine => "unmarked line in hunk",
            Self::MisplacedNoNewlineMarker => "no-newline marker does not follow a record",

            Self::MissingGitMetadata => "Git header has no extended header or file headers",

            Self::InvalidBinaryPatchHeader => "invalid binary patch header",
            Self::MissingBinaryPatchBlock => "missing binary patch block",
            Self::MissingBinaryPatchPayload => "missing binary patch payload",
            Self::InvalidBinaryPatchPayload => "invalid binary patch payload line",
            Self::TruncatedBinaryPatch => "truncated binary patch",
        }
    }
}

/// Parses viewer input, a patch, rejecting nonempty input that holds no file diff.
pub fn parse_patch(input: &[u8]) -> Result<DiffDocument, Error> {
    let document = parse_comparison(input)?;

    match (&document, input.is_empty()) {
        (DiffDocument::Files(_), _) | (DiffDocument::Text(_), true) => Ok(document),
        (DiffDocument::Text(_), false) => Err(Error::NoDiff),
    }
}

/// Parses `diff` output into its surrounding text and file sections. It may hold only
/// surrounding text, such as `Binary files … differ`.
pub fn parse_comparison(input: &[u8]) -> Result<DiffDocument, Error> {
    let mut lines = SourceLines::new(input);
    let mut sections = Vec::new();
    let mut text = Vec::new();

    while let Some(line) = lines.next() {
        match lines.at_last_line(line_start(line.sanitized()))? {
            LineStart::Text => text.push(line),
            LineStart::GitHeader => {
                let file = parse_git_section(line, &mut lines)?;
                sections.push(section_after(&mut text, file));
            }
            LineStart::OldFileHeader(path) => {
                let file = parse_unified(None, FileHeader { line, path }, &mut lines)?;
                sections.push(section_after(&mut text, file));
            }
        }
    }

    Ok(DiffDocument::new(sections, text.into_boxed_slice()))
}

/// `file` with the surrounding text gathered before it, which leaves `text` empty for the
/// next file.
fn section_after(text: &mut Vec<SourceLine>, file: DiffFile) -> FileSection {
    FileSection {
        leading: mem::take(text).into_boxed_slice(),
        file,
    }
}

// Sections

/// How a line outside a file section starts.
#[derive(PartialEq, Eq)]
enum LineStart {
    Text,
    GitHeader,
    /// A `---` line, with the path it names.
    OldFileHeader(HeaderPath),
}

/// How `text` starts; a diff header that cannot start a section here is malformed.
fn line_start(text: &str) -> Result<LineStart, Malformed> {
    reject_unsupported_header(text)?;

    if git_header_names(text).is_some() {
        return Ok(LineStart::GitHeader);
    }
    let start =
        HeaderPath::of_header(text, Side::Old).map_or(LineStart::Text, LineStart::OldFileHeader);
    Ok(start)
}

/// Rejects combined-diff headers, and hunk or `+++` headers outside a file section.
fn reject_unsupported_header(text: &str) -> Result<(), Malformed> {
    if is_combined_diff_header(text) || text.starts_with("@@@") {
        return Err(Malformed::CombinedDiff);
    }
    if starts_hunk_header(text) || file_header_field(text, Side::New).is_some() {
        return Err(Malformed::UnexpectedDiffHeader);
    }

    Ok(())
}

/// A `diff --git` section: unified when `---` / `+++` headers follow its metadata.
fn parse_git_section(header: SourceLine, lines: &mut SourceLines<'_>) -> Result<DiffFile, Error> {
    let mut metadata = iter::from_fn(|| lines.next_if(is_git_metadata)).collect::<Vec<_>>();
    if let Some(binary_patch) = lines.next_if(is_binary_patch_header) {
        metadata.push(binary_patch);
        metadata.extend(parse_binary_patch(lines)?);
    }

    let git = GitHeader {
        line: header,
        metadata: metadata.into_boxed_slice(),
    };
    if let Some(old) = lines.next_file_header(Side::Old) {
        return parse_unified(Some(git), old, lines);
    }

    let Some(body) = MetadataBody::of(git) else {
        return Err(lines.next_line_error(Malformed::MissingGitMetadata));
    };
    Ok(DiffFile {
        identity: FileIdentity::of_metadata(body.git()),
        body: FileBody::Metadata(body),
    })
}

/// Whether `text` is one of Git's extended header lines after `diff --git`.
fn is_git_metadata(text: &str) -> bool {
    GitMetadata::of(text).is_some()
}

/// A section with `---` / `+++` headers, after its Git header when it has one.
fn parse_unified(
    git: Option<GitHeader>,
    old: FileHeader,
    lines: &mut SourceLines<'_>,
) -> Result<DiffFile, Error> {
    let Some(new) = lines.next_file_header(Side::New) else {
        let error = match lines.peek() {
            None => lines.last_line_error(Malformed::MissingNewFileHeader),
            Some(_) => lines.next_line_error(Malformed::ExpectedNewFileHeader),
        };
        return Err(error);
    };

    let mut hunks = Vec::new();
    while let Some(header) = lines.next_if(starts_hunk_header) {
        let expected = lines.at_last_line(hunk_counts(header.sanitized()))?;
        hunks.push(parse_hunk(header, expected, lines)?);
    }
    let Some(hunks) = NonEmpty::of(hunks) else {
        return Err(lines.next_line_error(Malformed::FileWithoutHunks));
    };

    let headers = BySide { old, new };
    let identity = FileIdentity::of_unified(git.as_ref(), &headers);
    Ok(DiffFile {
        identity,
        body: FileBody::Unified(Box::new(UnifiedBody {
            git,
            headers,
            hunks,
        })),
    })
}

// Hunks

/// The records after a hunk `header` that declares `expected` counts, each with the
/// no-newline marker that follows it, which may follow the last of them.
fn parse_hunk(
    header: SourceLine,
    expected: BySide<usize>,
    lines: &mut SourceLines<'_>,
) -> Result<Hunk, Error> {
    let exhausted = BySide::<usize>::default();
    let mut remaining = expected;
    let mut records: Vec<Record> = Vec::new();
    while let Some(line) =
        lines.next_if(|text| remaining != exhausted || is_no_newline_marker(text))
    {
        match is_no_newline_marker(line.sanitized()) {
            true => lines.at_last_line(mark_no_newline(&mut records, line))?,
            false => {
                let record = lines.at_last_line(parse_record(line))?;
                remaining = lines.at_last_line(count_record(remaining, record.kind))?;
                records.push(record);
            }
        }
    }

    if remaining != exhausted {
        return Err(lines.next_line_error(Malformed::TruncatedHunk));
    }
    if lines.peek().is_some_and(is_excess_record) {
        return Err(lines.next_line_error(Malformed::ExcessRecords));
    }
    Ok(Hunk {
        header,
        records: records.into_boxed_slice(),
    })
}

/// The counts `remaining` after one record of `kind`; a record on a side whose declared
/// count is already used up is the first excess record.
fn count_record(remaining: BySide<usize>, kind: RecordKind) -> Result<BySide<usize>, Malformed> {
    let counted = remaining.map_sides(|side, count| match kind.on_side(side) {
        true => count.checked_sub(1),
        false => Some(count),
    });

    counted.transpose().ok_or(Malformed::ExcessRecords)
}

/// Gives the last of `records` the no-newline `marker` after it; a marker that follows no
/// record, or a record that already has one, is misplaced.
fn mark_no_newline(records: &mut [Record], marker: SourceLine) -> Result<(), Malformed> {
    let unmarked = records
        .last_mut()
        .filter(|record| record.no_newline.is_none());
    let Some(record) = unmarked else {
        return Err(Malformed::MisplacedNoNewlineMarker);
    };

    record.no_newline = Some(marker);
    Ok(())
}

/// The record a hunk line holds: a record marked by its first visible character, or blank
/// context when the line is empty. A diff header there is unexpected, and any other line
/// is unmarked.
fn parse_record(line: SourceLine) -> Result<Record, Malformed> {
    if let Some(record) = marked_record(&line) {
        return Ok(record);
    }
    if line.sanitized().is_empty() {
        return Ok(Record {
            kind: RecordKind::Context(ContextMarker::Blank),
            payload: line,
            no_newline: None,
        });
    }

    let text = line.sanitized();
    reject_unsupported_header(text)?;
    match git_header_names(text) {
        Some(_) => Err(Malformed::UnexpectedFileHeaderInHunk),
        None => Err(Malformed::UnmarkedHunkLine),
    }
}

/// Whether `text` is a no-newline marker. Producers translate its wording, so like
/// `git apply` it is known by its `\ ` prefix alone.
fn is_no_newline_marker(text: &str) -> bool {
    text.starts_with("\\ ")
}

/// The record `line` holds when its first visible character is a record marker.
fn marked_record(line: &SourceLine) -> Option<Record> {
    let (marker, payload) = line.split_first()?;
    let kind = RecordKind::of_marker(marker)?;

    Some(Record {
        kind,
        payload,
        no_newline: None,
    })
}

/// A marked record after a hunk's declared counts are complete; a `---` line starts the
/// next file instead.
fn is_excess_record(line: &SourceLine) -> bool {
    let marked = marked_record(line).is_some();

    marked && file_header_field(line.sanitized(), Side::Old).is_none()
}

/// Whether `text` starts a unified hunk header, well-formed or not: an `@@` line that is
/// not a combined diff's `@@@`. Both a section's hunk reader and the check for headers
/// outside a section ask this, so a malformed header gets one reading wherever it sits.
fn starts_hunk_header(text: &str) -> bool {
    text.starts_with("@@") && !text.starts_with("@@@")
}

/// The old-side and new-side line counts an `@@ -old +new @@` hunk header declares. The
/// fields are single-spaced, and only a section heading after a space may follow.
fn hunk_counts(header: &str) -> Result<BySide<usize>, Malformed> {
    let ranges = header
        .strip_prefix("@@ -")
        .and_then(|rest| rest.split_once(" @@"))
        .filter(|(_, heading)| heading.is_empty() || heading.starts_with(' '));
    let Some((old, new)) = ranges.and_then(|(ranges, _)| ranges.split_once(" +")) else {
        return Err(Malformed::InvalidHunkHeader);
    };

    BySide {
        old: range_count(old),
        new: range_count(new),
    }
    .transpose()
    .ok_or(Malformed::InvalidHunkHeader)
}

/// The line count a `start,count` hunk range states, after its `-` or `+`; a range
/// without a count states one line.
fn range_count(range: &str) -> Option<usize> {
    let (start, count) = match range.split_once(',') {
        Some((start, count)) => (start, decimal_count(count)?),
        None => (range, 1),
    };

    decimal_count(start).and(Some(count))
}

// Reading lines

/// The input's lines, each decoded once when first read.
struct SourceLines<'a> {
    rest: &'a [u8],
    peeked: Option<SourceLine>,
    taken: usize,
}

impl<'a> SourceLines<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            rest: input,
            peeked: None,
            taken: 0,
        }
    }

    /// The next line without taking it, decoded the first time it is read.
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

    /// Takes the next line, counting it so errors can name the line they are at.
    fn next(&mut self) -> Option<SourceLine> {
        self.peek();
        let line = self.peeked.take()?;

        self.taken += 1;
        Some(line)
    }

    /// Takes the next line when its sanitized text satisfies `accepts`.
    fn next_if(&mut self, accepts: impl FnOnce(&str) -> bool) -> Option<SourceLine> {
        let accepted = self.peek().is_some_and(|line| accepts(line.sanitized()));

        match accepted {
            true => self.next(),
            false => None,
        }
    }

    /// Takes the next line when it is `side`'s file header.
    fn next_file_header(&mut self, side: Side) -> Option<FileHeader> {
        let path = self
            .peek()
            .and_then(|line| HeaderPath::of_header(line.sanitized(), side))?;

        self.next().map(|line| FileHeader { line, path })
    }

    /// `result`, with a malformation located at the line most recently taken.
    fn at_last_line<T>(&self, result: Result<T, Malformed>) -> Result<T, Error> {
        result.map_err(|reason| self.last_line_error(reason))
    }

    /// An error at the line most recently taken.
    fn last_line_error(&self, reason: Malformed) -> Error {
        Error::Malformed {
            line: self.taken,
            reason,
        }
    }

    /// An error at the line that would be taken next, or just past the end.
    fn next_line_error(&self, reason: Malformed) -> Error {
        Error::Malformed {
            line: self.taken + 1,
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Malformed, parse_comparison, parse_patch};
    use crate::{
        document::{ContextMarker, DiffDocument, FileBody, Hunk, RecordKind, UnifiedBody},
        finite_output,
    };

    fn malformed(line: usize, reason: Malformed) -> Result<DiffDocument, Error> {
        Err(Error::Malformed { line, reason })
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
    fn preserves_surrounding_text_in_source_order() {
        let input = b"commit message\r\n\r\n--- a/one\n+++ b/one\n@@ -1 +1 @@\n-old\n+new\n\nbetween files\n--- a/two\n+++ b/two\n@@ -1 +1 @@\n-before\n+after\ntrailing text";

        let document = parse_patch(input).expect("diff with surrounding text");

        assert_eq!(finite_output::of_document(&document), input);
    }

    #[test]
    fn rejects_nonempty_text_only_input() {
        for input in [b"ordinary text\n".as_slice(), b" \t\r\n", b"\n"] {
            assert_eq!(parse_patch(input), Err(Error::NoDiff));
        }
        assert_eq!(parse_patch(b""), Ok(DiffDocument::Text(Box::default())));
    }

    #[test]
    fn keeps_comparison_output_that_holds_only_text() {
        let input = b"Binary files before.bin and after.bin differ\n";

        let document = parse_comparison(input).expect("text-only comparison output");

        assert_eq!(finite_output::of_document(&document), input);
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

    /// docs/spec/architecture.md#input-contract: structural parsing is strict, so a
    /// header count is ASCII digits and nothing else.
    #[test]
    fn rejects_signed_header_counts() {
        let files = "--- a/file\n+++ b/file\n";
        for header in ["@@ -+1,+1 +1 @@", "@@ -1,+1 +1 @@", "@@ -1 ++1 @@"] {
            let input = format!("{files}{header}\n-old\n+new\n");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(3, Malformed::InvalidHunkHeader),
                "{header}"
            );
        }

        let binary = "diff --git a/image b/image\nGIT binary patch\nliteral +12\npayload\n\n";
        assert_eq!(
            parse_patch(binary.as_bytes()),
            malformed(3, Malformed::InvalidBinaryPatchHeader)
        );
    }

    /// docs/spec/architecture.md#input-contract: every line inside a hunk is a marked
    /// record, a blank context line, or a no-newline marker.
    #[test]
    fn rejects_unmarked_lines_inside_hunks() {
        let files = "--- a/x\n+++ b/x\n";
        for (hunk, line) in [
            ("@@ -1 +1 @@\nhello\n-a\n+b\n", 4),
            ("@@ -2 +2 @@\n-a\nhello world\n+b\n", 5),
        ] {
            let input = format!("{files}{hunk}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(line, Malformed::UnmarkedHunkLine),
                "{hunk}"
            );
        }
    }

    /// docs/spec/architecture.md#input-contract: GNU `diff --suppress-blank-empty`
    /// writes a blank context line as an empty line.
    #[test]
    fn reads_an_empty_line_inside_a_hunk_as_blank_context() {
        let input = b"--- a/x\n+++ b/x\n@@ -1,3 +1,3 @@\n a\n\n-b\n+c\n";

        let document = parse_patch(input).expect("GNU blank context");
        let kinds = first_hunk(&document)
            .records
            .iter()
            .map(|record| record.kind)
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [
                RecordKind::Context(ContextMarker::Space),
                RecordKind::Context(ContextMarker::Blank),
                RecordKind::Deletion,
                RecordKind::Addition,
            ]
        );
        assert_eq!(finite_output::of_document(&document), input);
    }

    /// docs/spec/architecture.md#input-contract: the marker is known by its `\ ` prefix,
    /// since producers translate the rest.
    #[test]
    fn keeps_a_translated_no_newline_marker_with_its_hunk() {
        let input = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n\\ Kein Zeilenumbruch am Dateiende.\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-c\n+d\n";

        let document = parse_patch(input.as_bytes()).expect("translated marker");
        let views = document.file_views().collect::<Vec<_>>();
        let [first, second] = views.as_slice() else {
            panic!("the patch holds two files: {} views", views.len());
        };
        let FileBody::Unified(first) = &first.file.body else {
            panic!("first file is unified");
        };
        let marker = first.hunks[0]
            .records
            .last()
            .and_then(|record| record.no_newline.as_ref())
            .map(|marker| marker.sanitized());

        assert_eq!(marker, Some("\\ Kein Zeilenumbruch am Dateiende."));
        assert!(
            second.leading.is_empty(),
            "the marker is not the next file's text: {:?}",
            second.leading
        );
    }

    /// docs/spec/architecture.md#input-contract: a no-newline marker follows a record.
    #[test]
    fn rejects_a_no_newline_marker_that_follows_no_record() {
        let files = "--- a/x\n+++ b/x\n";
        let marker = "\\ No newline at end of file\n";
        for (hunk, line) in [
            (format!("@@ -1 +1 @@\n{marker}-a\n+b\n"), 4),
            (format!("@@ -1 +1 @@\n-a\n{marker}{marker}+b\n"), 6),
            (format!("@@ -1 +1 @@\n-a\n+b\n{marker}{marker}"), 7),
        ] {
            let input = format!("{files}{hunk}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(line, Malformed::MisplacedNoNewlineMarker),
                "{hunk}"
            );
        }
    }

    /// docs/spec/architecture.md#input-contract: a hunk header is
    /// `@@ -<range> +<range> @@`, single-spaced as producers write it and `git apply`
    /// reads it.
    #[test]
    fn rejects_hunk_headers_spaced_other_than_producers_write() {
        let files = "--- a/x\n+++ b/x\n";
        for header in [
            "@@-1 +1 @@",
            "@@  -1  +1  @@",
            "@@ -1\t+1 @@",
            "@@ -1 +1  @@",
            "@@ -1  +1 @@",
        ] {
            let input = format!("{files}{header}\n-a\n+b\n");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(3, Malformed::InvalidHunkHeader),
                "{header:?}"
            );
        }

        let headed = format!("{files}@@ -1 +1 @@ fn main() {{\n-a\n+b\n");
        assert!(
            parse_patch(headed.as_bytes()).is_ok(),
            "a section heading follows"
        );
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
    fn rejects_the_record_that_overshoots_one_side_of_a_hunk() {
        let overfull = "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n-b\n+c\n";
        for suffix in ["", "--- a/g\n+++ b/g\n@@ -1 +1 @@\n-x\n+y\n"] {
            let input = format!("{overfull}{suffix}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(5, Malformed::ExcessRecords),
                "{suffix}"
            );
        }
    }

    /// docs/spec/architecture.md#input-contract: Git writes an extended header after
    /// every `diff --git` line it gives no `---` / `+++` headers.
    #[test]
    fn rejects_a_git_header_with_nothing_after_it() {
        for input in ["diff --git a/x b/x\n", "diff --git a/x b/x\nhello\n"] {
            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(2, Malformed::MissingGitMetadata),
                "{input}"
            );
        }
    }

    #[test]
    fn keeps_text_separate_from_metadata_only_files() {
        let input =
            b"message\ndiff --git a/file b/file\nold mode 100644\nnew mode 100755\nfooter\n";
        let document = parse_patch(input).expect("mode-only diff with text");
        let views = document.file_views().collect::<Vec<_>>();
        let FileBody::Metadata(body) = &views[0].file.body else {
            panic!("mode-only diff is metadata");
        };

        assert_eq!(views.len(), 1);
        assert_eq!(body.git().metadata.len(), 2);
        assert_eq!(views[0].leading[0].sanitized(), "message");
        assert_eq!(views[0].trailing[0].sanitized(), "footer");
        assert_eq!(finite_output::of_document(&document), input);
    }

    #[test]
    fn parses_file_hunk_and_records() {
        let input = "--- a/readme.txt\n+++ b/readme.txt\n@@ -1 +1 @@\n-old\n+new\n";

        let document = parse_patch(input.as_bytes()).expect("valid unified diff");
        let bodies = unified_bodies(&document);

        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].hunks.len().get(), 1);
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
        let body = &document.files().next().expect("first file").body;

        assert!(
            matches!(body, FileBody::Metadata(_)),
            "a section without a file header is metadata: {body:?}"
        );
    }

    #[test]
    fn parses_metadata_only_git_fixture() {
        let input = include_bytes!("../tests/fixtures/git-rename-only.patch");

        let document = parse_patch(input).expect("valid metadata-only Git fixture");

        let body = &document.files().next().expect("first file").body;

        assert_eq!(document.files().count(), 1);
        assert!(
            matches!(body, FileBody::Metadata(_)),
            "a rename-only section is metadata: {body:?}"
        );
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

    /// docs/spec/architecture.md#lossless-and-safe-rendering: parsing uses ANSI-free
    /// text, so the payload starts after the marker the sanitized text shows.
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
