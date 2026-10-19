use std::{fmt, iter, mem};

use crate::{
    document::{
        DiffDocument, DiffFile, FileBody, FileSection, Hunk, Record, RecordKind, UnifiedBody,
    },
    file_identity::FileIdentity,
    headers::{
        FileHeader, GitHeader, GitMetadata, HeaderPath, file_header_field, git_header_names,
        is_combined_diff_header,
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

/// How a recognized diff section breaks the unified diff or Git patch format.
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
    /// The reason as error messages state it.
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
#[derive(PartialEq)]
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
    if text.starts_with("@@") || file_header_field(text, Side::New).is_some() {
        return Err(Malformed::UnexpectedDiffHeader);
    }

    Ok(())
}

/// A `diff --git` section: unified when `---` / `+++` headers follow its metadata.
fn parse_git_section(header: SourceLine, lines: &mut SourceLines<'_>) -> Result<DiffFile, Error> {
    let mut metadata = iter::from_fn(|| lines.next_if(is_git_metadata)).collect::<Vec<_>>();
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
    Ok(file)
}

fn is_git_metadata(text: &str) -> bool {
    GitMetadata::of(text).is_some()
}

/// The `literal` and `delta` blocks after `GIT binary patch`, each with its payload and
/// blank separator.
fn parse_binary_patch(lines: &mut SourceLines<'_>) -> Result<Vec<SourceLine>, Error> {
    let mut patch = Vec::new();

    loop {
        let header = match lines.next_header(binary_block_header) {
            HeaderRead::Other => break,
            HeaderRead::Read(Taken { line, value: () }) => line,
            HeaderRead::Malformed(error) => return Err(error),
        };
        patch.push(header);

        let payload_start = patch.len();
        while let Some(line) = lines.next_if(|text| !text.is_empty()) {
            if lines.at_last_line(line_start(line.sanitized()))? != LineStart::Text {
                return Err(lines.last_line_error(Malformed::TruncatedBinaryPatch));
            }
            patch.push(line);
        }
        if patch.len() == payload_start {
            return Err(lines.next_line_error(Malformed::MissingBinaryPatchPayload));
        }
        patch.extend(lines.next_if(str::is_empty));
    }

    match patch.is_empty() {
        true => Err(lines.next_line_error(Malformed::MissingBinaryPatchBlock)),
        false => Ok(patch),
    }
}

/// What `text` reads as where a `literal` or `delta` block header may start.
fn binary_block_header(text: &str) -> HeaderRead<(), Malformed> {
    let literal = text.strip_prefix("literal ");
    let delta = text.strip_prefix("delta ");
    let Some(size) = literal.or(delta) else {
        return HeaderRead::Other;
    };

    match is_decimal_count(size) {
        true => HeaderRead::Read(()),
        false => HeaderRead::Malformed(Malformed::InvalidBinaryPatchHeader),
    }
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
    loop {
        let Taken {
            line: header,
            value: expected,
        } = match lines.next_header(hunk_header) {
            HeaderRead::Other => break,
            HeaderRead::Read(taken) => taken,
            HeaderRead::Malformed(error) => return Err(error),
        };
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

/// The line that follows a record whose source line has no line ending.
const NO_NEWLINE_MARKER: &str = "\\ No newline at end of file";

/// The records after a hunk `header` that declares `expected` counts.
fn parse_hunk(
    header: SourceLine,
    expected: BySide<usize>,
    lines: &mut SourceLines<'_>,
) -> Result<Hunk, Error> {
    let exhausted = BySide::<usize>::default();
    let mut remaining = expected;
    let mut records = Vec::new();
    while let Some(line) =
        lines.next_if(|text| remaining != exhausted || text.starts_with(NO_NEWLINE_MARKER))
    {
        let record = lines.at_last_line(parse_record(line))?;
        remaining = lines.at_last_line(count_record(remaining, record.kind))?;
        records.push(record);
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

/// The record a hunk line holds: marked by its first visible character, or raw when
/// unmarked; a Git header cannot appear inside a hunk.
fn parse_record(line: SourceLine) -> Result<Record, Malformed> {
    if let Some(record) = marked_record(&line) {
        return Ok(record);
    }

    match line_start(line.sanitized())? {
        LineStart::GitHeader => Err(Malformed::UnexpectedFileHeaderInHunk),
        LineStart::Text | LineStart::OldFileHeader(_) => Ok(Record {
            kind: RecordKind::Raw,
            payload: line,
        }),
    }
}

/// The record `line` holds when its first visible character is a record marker.
fn marked_record(line: &SourceLine) -> Option<Record> {
    let (marker, payload) = line.split_first()?;
    let kind = RecordKind::of_marker(marker)?;

    Some(Record { kind, payload })
}

/// A marked record after a hunk's declared counts are complete; a `---` line starts the
/// next file instead.
fn is_excess_record(line: &SourceLine) -> bool {
    let marked = marked_record(line).is_some();

    marked && file_header_field(line.sanitized(), Side::Old).is_none()
}

/// What `text` reads as where a hunk header may start: an `@@ ` line is a hunk header,
/// and states its counts or is malformed.
fn hunk_header(text: &str) -> HeaderRead<BySide<usize>, Malformed> {
    if !text.starts_with("@@ ") {
        return HeaderRead::Other;
    }

    match hunk_counts(text) {
        Some(counts) => HeaderRead::Read(counts),
        None => HeaderRead::Malformed(Malformed::InvalidHunkHeader),
    }
}

/// The old-side and new-side line counts an `@@ -old +new @@` hunk header declares.
fn hunk_counts(header: &str) -> Option<BySide<usize>> {
    let mut parts = header.split_whitespace();
    let [Some("@@"), Some(old), Some(new), Some("@@")] =
        [parts.next(), parts.next(), parts.next(), parts.next()]
    else {
        return None;
    };

    let old = range_count(old, '-')?;
    let new = range_count(new, '+')?;
    Some(BySide { old, new })
}

/// The line count a `-start,count` or `+start,count` hunk range states; a range without
/// a count states one line.
fn range_count(range: &str, prefix: char) -> Option<usize> {
    let range = range.strip_prefix(prefix)?;
    let (start, count) = match range.split_once(',') {
        Some((start, count)) => (start, count),
        None => (range, "1"),
    };
    if !is_decimal_count(start) {
        return None;
    }

    count.parse().ok()
}

/// Whether `text` is a line number or count a header may state.
fn is_decimal_count(text: &str) -> bool {
    text.parse::<usize>().is_ok()
}

// Reading lines

/// The input's lines, each decoded once when first read.
struct SourceLines<'a> {
    rest: &'a [u8],
    peeked: Option<SourceLine>,
    taken: usize,
}

/// A line [`SourceLines`] took, with the value it read from the line.
struct Taken<T> {
    line: SourceLine,
    value: T,
}

/// What a line reads as where a header of one kind may start.
enum HeaderRead<T, E> {
    /// Not a header of this kind.
    Other,
    /// A header of this kind, with the value it states.
    Read(T),
    /// A header of this kind that states its value wrongly.
    Malformed(E),
}

impl<'a> SourceLines<'a> {
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
        let accepted = self.peek().is_some_and(|line| accepts(line.sanitized()));

        match accepted {
            true => self.next(),
            false => None,
        }
    }

    /// Takes the next line when `extract` reads a value from its sanitized text, with
    /// that value.
    fn next_if_map<T>(&mut self, extract: impl FnOnce(&str) -> Option<T>) -> Option<Taken<T>> {
        let value = self.peek().and_then(|line| extract(line.sanitized()))?;

        self.next().map(|line| Taken { line, value })
    }

    /// Takes the next line when `read` finds it a header, well-formed or malformed; a
    /// malformed header's error is located at that line.
    fn next_header<T>(
        &mut self,
        read: impl FnOnce(&str) -> HeaderRead<T, Malformed>,
    ) -> HeaderRead<Taken<T>, Error> {
        let read = self
            .peek()
            .map_or(HeaderRead::Other, |line| read(line.sanitized()));
        let is_header = !matches!(read, HeaderRead::Other);
        let Some(line) = self.next_if(|_| is_header) else {
            return HeaderRead::Other;
        };

        match read {
            HeaderRead::Other => HeaderRead::Other,
            HeaderRead::Read(value) => HeaderRead::Read(Taken { line, value }),
            HeaderRead::Malformed(reason) => HeaderRead::Malformed(self.last_line_error(reason)),
        }
    }

    /// Takes the next line when it is `side`'s file header.
    fn next_file_header(&mut self, side: Side) -> Option<FileHeader> {
        self.next_if_map(|text| HeaderPath::of_header(text, side))
            .map(|Taken { line, value: path }| FileHeader { line, path })
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
        document::{DiffDocument, FileBody, Hunk, RecordKind, UnifiedBody},
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
        assert_eq!(finite_output::of_document(&document), input);
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
        assert_eq!(finite_output::of_document(&document), input);
    }

    #[test]
    fn only_a_section_start_cuts_a_binary_payload_short() {
        let patch = |payload: &str| {
            format!(
                "diff --git a/image b/image\nindex 1..2 100644\nGIT binary patch\nliteral 3\n{payload}\n\n"
            )
        };

        assert_eq!(
            parse_patch(patch("diff --git a/next b/next").as_bytes()),
            malformed(5, Malformed::TruncatedBinaryPatch)
        );
        assert_eq!(
            parse_patch(patch("--- a/next").as_bytes()),
            malformed(5, Malformed::TruncatedBinaryPatch)
        );
        assert!(
            parse_patch(patch("diff --stat").as_bytes()).is_ok(),
            "a line that starts no section is not a new section inside a payload either"
        );
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
