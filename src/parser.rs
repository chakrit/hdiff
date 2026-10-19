//! Viewer input and `diff` output read into a document: surrounding text, file
//! sections, hunks and their records. Rules: docs/spec/architecture.md#input-contract.

mod binary_patch;
mod hunk;

use std::{fmt, iter, mem};

use self::{
    binary_patch::{is_binary_patch_header, parse_binary_patch},
    hunk::{hunk_counts, parse_hunk, starts_combined_hunk_header, starts_hunk_header},
};
use crate::{
    document::{DiffDocument, DiffFile, FileBody, FileSection, MetadataBody, UnifiedBody},
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
    UnexpectedGitHeaderInHunk,
    UnmarkedHunkLine,
    MisplacedNoNewlineMarker,
    RecordAfterNoNewlineMarker,

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
            Self::UnexpectedGitHeaderInHunk => "unexpected Git header in hunk",
            Self::UnmarkedHunkLine => "unmarked line in hunk",
            Self::MisplacedNoNewlineMarker => "no-newline marker does not follow a record",
            Self::RecordAfterNoNewlineMarker => {
                "record follows the no-newline marker that ended its side"
            }

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
    if is_combined_diff_header(text) || starts_combined_hunk_header(text) {
        return Err(Malformed::CombinedDiff);
    }
    if starts_hunk_header(text) || file_header_field(text, Side::New).is_some() {
        return Err(Malformed::UnexpectedDiffHeader);
    }

    Ok(())
}

/// A `diff --git` section: unified when `---` / `+++` headers follow its metadata, and
/// otherwise its metadata with the binary patch after it when it has one.
fn parse_git_section(header: SourceLine, lines: &mut SourceLines<'_>) -> Result<DiffFile, Error> {
    let extended_headers = iter::from_fn(|| lines.next_if(is_git_metadata)).collect::<Vec<_>>();
    if let Some(old) = lines.next_file_header(Side::Old) {
        let git = GitHeader {
            line: header,
            extended_headers: extended_headers.into_boxed_slice(),
        };
        return parse_unified(Some(git), old, lines);
    }

    let Some(extended_headers) = NonEmpty::of(extended_headers) else {
        return Err(lines.next_line_error(Malformed::MissingGitMetadata));
    };
    let binary_patch = match lines.next_if(is_binary_patch_header) {
        Some(binary_header) => Some(Box::new(parse_binary_patch(binary_header, lines)?)),
        None => None,
    };

    let git = GitHeader {
        line: header,
        extended_headers,
    };
    let identity = FileIdentity::of_metadata(&git);
    Ok(DiffFile {
        identity,
        body: FileBody::Metadata(MetadataBody { git, binary_patch }),
    })
}

/// Whether `text` is one of Git's extended header lines after `diff --git`.
fn is_git_metadata(text: &str) -> bool {
    GitMetadata::of(text).is_some()
}

/// A section with `---` / `+++` headers, after its Git header when it has one.
fn parse_unified(
    git: Option<GitHeader<Box<[SourceLine]>>>,
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
    let mut ended = BySide::<bool>::default();
    while let Some(header) = lines.next_if(starts_hunk_header) {
        let expected = lines.at_last_line(hunk_counts(header.sanitized()))?;
        let hunk = parse_hunk(header, expected, ended, lines)?;
        ended = ended.or(hunk.ended_sides());
        hunks.push(hunk);
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

/// The line number or count a header `field` states. Only ASCII digits are a count:
/// Rust's integer parsing also admits a leading `+`, which no diff writes. Rules:
/// docs/spec/architecture.md#input-contract.
fn decimal_count(field: &str) -> Option<usize> {
    let digits = !field.is_empty() && field.bytes().all(|byte| byte.is_ascii_digit());
    if !digits {
        return None;
    }

    field.parse().ok()
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
        document::{DiffDocument, FileBody, UnifiedBody},
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

    /// Each body's first-hunk record count.
    fn first_hunk_record_counts(bodies: &[&UnifiedBody]) -> Vec<usize> {
        bodies
            .iter()
            .map(|body| body.hunks.first().records.len())
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
        let [view] = views.as_slice() else {
            panic!("the patch holds one file: {} views", views.len());
        };
        let FileBody::Metadata(body) = &view.file.body else {
            panic!("mode-only diff is metadata");
        };
        let ([message], [footer]) = (view.leading, view.trailing) else {
            panic!(
                "one line on each side: {:?} {:?}",
                view.leading, view.trailing
            );
        };

        assert_eq!(body.git.extended_headers.len().get(), 2);
        assert_eq!(message.sanitized(), "message");
        assert_eq!(footer.sanitized(), "footer");
        assert_eq!(finite_output::of_document(&document), input);
    }

    #[test]
    fn parses_file_hunk_and_records() {
        let input = "--- a/readme.txt\n+++ b/readme.txt\n@@ -1 +1 @@\n-old\n+new\n";

        let document = parse_patch(input.as_bytes()).expect("valid unified diff");
        let bodies = unified_bodies(&document);
        let [body] = bodies[..] else {
            panic!("the patch holds one file: {} bodies", bodies.len());
        };

        assert_eq!(body.hunks.len().get(), 1);
        assert_eq!(first_hunk_record_counts(&bodies), [2]);
    }

    #[test]
    fn parses_git_fixture_with_extended_headers_and_multiline_hunks() {
        let input = include_bytes!("../tests/fixtures/git-multiline.patch");

        let document = parse_patch(input).expect("valid Git fixture");
        let bodies = unified_bodies(&document);
        let extended_header_counts = bodies
            .iter()
            .map(|body| {
                body.git
                    .as_ref()
                    .map(|header| header.extended_headers.len())
            })
            .collect::<Vec<_>>();

        assert_eq!(extended_header_counts, [Some(2), Some(1)]);
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
}
