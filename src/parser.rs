//! Viewer input and `diff` output read into a document: surrounding text, file
//! sections, hunks and their records. A section that breaks the format is kept as a
//! broken file, and reading resumes at the next file header. Rules:
//! docs/spec/architecture.md#input-contract.

mod binary_patch;
mod hunk;

use std::{
    fmt,
    io::{self, BufRead},
    iter, mem,
};

use self::{
    binary_patch::{is_binary_patch_header, parse_binary_patch},
    hunk::{hunk_counts, parse_hunk, starts_combined_hunk_header, starts_hunk_header},
};
use crate::{
    document::{
        BrokenBody, DiffDocument, DiffFile, Failure, FileBody, FileSection, Hunk, MetadataBody,
        ReadPart, UnifiedBody,
    },
    file_identity::FileIdentity,
    headers::{
        FileHeader, GitHeader, GitMetadata, HeaderPath, file_header_field, git_header_names,
        is_combined_diff_header,
    },
    malformed::Malformed,
    non_empty::NonEmpty,
    side::{BySide, Side},
    source_text::SourceLine,
};

/// Why viewer input was rejected: it is not empty and holds no file diff.
#[derive(Debug, PartialEq, Eq)]
pub struct NoDiff;

impl fmt::Display for NoDiff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("input contains no diff")
    }
}

impl std::error::Error for NoDiff {}

/// Parses viewer input, a patch, rejecting nonempty input that holds no file diff.
pub fn parse_patch(input: &[u8]) -> Result<DiffDocument, NoDiff> {
    let document = parse_comparison(input);

    match (&document, input.is_empty()) {
        (DiffDocument::Files(_), true | false) | (DiffDocument::Text(_), true) => Ok(document),
        (DiffDocument::Text(_), false) => Err(NoDiff),
    }
}

/// Parses `diff` output into its surrounding text and file sections. It may hold only
/// surrounding text, such as `Binary files … differ`.
pub fn parse_comparison(input: &[u8]) -> DiffDocument {
    let mut bytes = input;
    let mut reader = SectionReader::new(&mut bytes);
    let sections = iter::from_fn(|| reader.next_section()).collect();
    let trailing = reader.finish().expect("reading a byte slice never fails");

    DiffDocument::new(sections, trailing)
}

/// A diff read from its input one file section at a time, as its lines arrive.
pub struct SectionReader<'a> {
    lines: SourceLines<'a>,
    /// Surrounding text read since the last section.
    text: Vec<SourceLine>,
}

impl<'a> SectionReader<'a> {
    pub fn new(input: &'a mut dyn BufRead) -> Self {
        Self {
            lines: SourceLines::new(input),
            text: Vec::new(),
        }
    }

    /// The next file section with the surrounding text before it; none once the input
    /// ends. A section that breaks the format is kept as a broken file, and reading
    /// resumes at the next file header.
    pub(crate) fn next_section(&mut self) -> Option<FileSection> {
        self.next_section_reporting(&mut |_| {})
    }

    /// [`next_section`](Self::next_section), with `progress` hearing of a unified
    /// section's headers and of each of its hunks as each is read.
    pub(crate) fn next_section_reporting(
        &mut self,
        progress: &mut dyn FnMut(Progress<'_>),
    ) -> Option<FileSection> {
        let lines = &mut self.lines;
        while let Some(line) = lines.next() {
            let parsed = match line_start(line.sanitized()) {
                Ok(LineStart::Text) => {
                    self.text.push(line);
                    continue;
                }
                Ok(LineStart::GitHeader) => parse_git_section(line, lines, &self.text, progress),
                Ok(LineStart::OldFileHeader(path)) => {
                    parse_unified(None, FileHeader { line, path }, lines, &self.text, progress)
                }
                Err(reason) => Err(Box::new(Broken {
                    read: ReadPart::default(),
                    failure: lines.last_line_failure(reason),
                    failing: Some(line),
                })),
            };
            let file = parsed.unwrap_or_else(|broken| broken.into_file(lines));
            return Some(section_after(&mut self.text, file));
        }

        None
    }

    /// The surrounding text after the last section, or the read error that ended the
    /// input.
    pub fn finish(self) -> io::Result<Box<[SourceLine]>> {
        match self.lines.error {
            Some(error) => Err(error),
            None => Ok(self.text.into_boxed_slice()),
        }
    }
}

/// `file` with the surrounding text gathered before it, which leaves `text` empty for the
/// next file.
fn section_after(text: &mut Vec<SourceLine>, file: DiffFile) -> FileSection {
    FileSection {
        leading: mem::take(text).into_boxed_slice(),
        file,
    }
}

/// What a section reader reports while it reads a unified section, before the section
/// ends.
pub(crate) enum Progress<'a> {
    /// The section's file headers, with the surrounding text before the section and its
    /// Git header.
    Headers {
        leading: &'a [SourceLine],
        git: Option<&'a GitHeader<Box<[SourceLine]>>>,
        headers: &'a BySide<FileHeader>,
    },
    /// The section's hunk at `index`.
    Hunk { index: usize, hunk: &'a Hunk },
}

// Broken sections

/// A section that broke: what it read, where and why, and the failing line when the
/// reader already took it.
struct Broken {
    read: ReadPart,
    failure: Failure,
    failing: Option<SourceLine>,
}

impl Broken {
    /// The broken file: the section's read part, its failure, and the rest of the
    /// section up to the next file header as raw text. A failing line that starts the
    /// next section is left to start it.
    fn into_file(self, lines: &mut SourceLines<'_>) -> DiffFile {
        let Self {
            read,
            failure,
            failing,
        } = self;
        let failing_rest = failing.and_then(|line| match starts_section(line.sanitized()) {
            true => {
                lines.put_back(line);
                None
            }
            false => Some(line),
        });
        let later_rest = iter::from_fn(|| lines.next_if(|text| !starts_section(text)));
        let rest = failing_rest.into_iter().chain(later_rest).collect();
        let identity = FileIdentity::of_broken(read.metadata.first(), &read.headers);

        DiffFile {
            identity,
            body: FileBody::Broken(Box::new(BrokenBody {
                read,
                failure,
                rest,
            })),
        }
    }
}

/// Whether `text` starts a file section: a `diff --git` line or a `---` file header.
fn starts_section(text: &str) -> bool {
    matches!(
        line_start(text),
        Ok(LineStart::GitHeader | LineStart::OldFileHeader(_))
    )
}

/// A Git header's lines in order: its `diff --git` line, then its extended headers.
fn git_lines<H: IntoIterator<Item = SourceLine>>(git: GitHeader<H>) -> Vec<SourceLine> {
    iter::once(git.line).chain(git.extended_headers).collect()
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
fn parse_git_section(
    header: SourceLine,
    lines: &mut SourceLines<'_>,
    leading: &[SourceLine],
    progress: &mut dyn FnMut(Progress<'_>),
) -> Result<DiffFile, Box<Broken>> {
    let header_lines = iter::from_fn(|| lines.next_if(is_git_metadata)).collect::<Vec<_>>();
    if let Some(old) = lines.next_file_header(Side::Old) {
        let git = GitHeader {
            line: header,
            extended_headers: header_lines.into_boxed_slice(),
        };
        return parse_unified(Some(git), old, lines, leading, progress);
    }

    let Some(extended_headers) = NonEmpty::of(header_lines) else {
        return Err(Box::new(Broken {
            read: ReadPart {
                metadata: Box::new([header]),
                ..ReadPart::default()
            },
            failure: lines.next_line_failure(Malformed::MissingGitMetadata),
            failing: None,
        }));
    };
    let git = GitHeader {
        line: header,
        extended_headers,
    };
    let binary_patch = match lines.next_if(is_binary_patch_header) {
        None => None,
        Some(binary_header) => match parse_binary_patch(binary_header, lines) {
            Ok(patch) => Some(Box::new(patch)),
            Err(broken) => {
                let metadata = git_lines(git).into_iter().chain(broken.read).collect();
                return Err(Box::new(Broken {
                    read: ReadPart {
                        metadata,
                        ..ReadPart::default()
                    },
                    failure: broken.failure,
                    failing: broken.failing,
                }));
            }
        },
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

/// A section with `---` / `+++` headers, after its Git header when it has one; `leading`
/// text comes before it. `progress` hears of the headers and of each hunk as each is read.
fn parse_unified(
    git: Option<GitHeader<Box<[SourceLine]>>>,
    old: FileHeader,
    lines: &mut SourceLines<'_>,
    leading: &[SourceLine],
    progress: &mut dyn FnMut(Progress<'_>),
) -> Result<DiffFile, Box<Broken>> {
    let Some(new) = lines.next_file_header(Side::New) else {
        let failure = match lines.peek() {
            None => lines.last_line_failure(Malformed::MissingNewFileHeader),
            Some(_) => lines.next_line_failure(Malformed::ExpectedNewFileHeader),
        };
        return Err(Box::new(Broken {
            read: unified_read(git, vec![old], Vec::new()),
            failure,
            failing: None,
        }));
    };
    let headers = BySide { old, new };
    progress(Progress::Headers {
        leading,
        git: git.as_ref(),
        headers: &headers,
    });

    let mut parsed_hunks = Vec::new();
    let mut ended = BySide::default();
    while let Some(header) = lines.next_if(starts_hunk_header) {
        let expected = match hunk_counts(header.sanitized()) {
            Ok(expected) => expected,
            Err(reason) => {
                return Err(Box::new(Broken {
                    read: unified_read(git, headers.into_iter().collect(), parsed_hunks),
                    failure: lines.last_line_failure(reason),
                    failing: Some(header),
                }));
            }
        };
        let hunk = match parse_hunk(header, expected, ended, lines) {
            Ok(hunk) => hunk,
            Err(broken) => {
                parsed_hunks.push(broken.hunk);
                return Err(Box::new(Broken {
                    read: unified_read(git, headers.into_iter().collect(), parsed_hunks),
                    failure: broken.failure,
                    failing: broken.failing,
                }));
            }
        };
        ended = ended.or(hunk.ended_sides());
        progress(Progress::Hunk {
            index: parsed_hunks.len(),
            hunk: &hunk,
        });
        parsed_hunks.push(hunk);
    }
    let Some(hunks) = NonEmpty::of(parsed_hunks) else {
        return Err(Box::new(Broken {
            read: unified_read(git, headers.into_iter().collect(), Vec::new()),
            failure: lines.next_line_failure(Malformed::FileWithoutHunks),
            failing: None,
        }));
    };

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

/// What a unified section read before it broke: its Git header's lines, its file
/// `headers`, and its `hunks`.
fn unified_read(
    git: Option<GitHeader<Box<[SourceLine]>>>,
    headers: Vec<FileHeader>,
    hunks: Vec<Hunk>,
) -> ReadPart {
    ReadPart {
        metadata: git.map(git_lines).unwrap_or_default().into_boxed_slice(),
        headers: headers.into_boxed_slice(),
        hunks: hunks.into_boxed_slice(),
    }
}

/// The line number or count a header `field` states, when it is one or more ASCII digits
/// small enough to hold. Rust's integer parsing also admits a leading `+`, which no diff
/// writes. Rules: docs/spec/architecture.md#input-contract.
fn header_number(field: &str) -> Option<usize> {
    let digits = !field.is_empty() && field.bytes().all(|byte| byte.is_ascii_digit());
    if !digits {
        return None;
    }

    field.parse().ok()
}

// Reading lines

/// The input's lines, each decoded once when first read, as they arrive. A read error
/// ends the input and is kept for [`SectionReader::finish`].
struct SourceLines<'a> {
    input: &'a mut dyn BufRead,
    peeked: Option<SourceLine>,
    taken: usize,
    /// A line's bytes when a read splits it across the input's buffer.
    partial: Vec<u8>,
    error: Option<io::Error>,
}

impl<'a> SourceLines<'a> {
    fn new(input: &'a mut dyn BufRead) -> Self {
        Self {
            input,
            peeked: None,
            taken: 0,
            partial: Vec::new(),
            error: None,
        }
    }

    /// The next line without taking it, decoded the first time it is read.
    fn peek(&mut self) -> Option<&SourceLine> {
        if self.peeked.is_none() && self.error.is_none() {
            match self.read_line() {
                Ok(line) => self.peeked = line,
                Err(error) => self.error = Some(error),
            }
        }

        self.peeked.as_ref()
    }

    /// Reads the next line from the input: decoded straight from the input's buffer when
    /// the whole line is there, and gathered across reads otherwise. None at the end.
    fn read_line(&mut self) -> io::Result<Option<SourceLine>> {
        let buffered = loop {
            match self.input.fill_buf() {
                Ok(buffered) => break buffered,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        };
        if buffered.is_empty() {
            return Ok(None);
        }

        if let Some(line_feed) = buffered.iter().position(|byte| *byte == b'\n') {
            let line = SourceLine::decode(&buffered[..=line_feed]);
            self.input.consume(line_feed + 1);
            return Ok(Some(line));
        }
        self.partial.clear();
        self.input.read_until(b'\n', &mut self.partial)?;
        Ok(Some(SourceLine::decode(&self.partial)))
    }

    /// Takes the next line, counting it so failures can name the line they are at.
    fn next(&mut self) -> Option<SourceLine> {
        self.peek();
        let line = self.peeked.take()?;

        self.taken += 1;
        Some(line)
    }

    /// Returns `line`, the line most recently taken, so the next take reads it again.
    fn put_back(&mut self, line: SourceLine) {
        assert!(self.peeked.is_none(), "only the line just taken goes back");

        self.taken -= 1;
        self.peeked = Some(line);
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

    /// A failure at the line most recently taken.
    fn last_line_failure(&self, reason: Malformed) -> Failure {
        Failure::new(self.taken, reason)
    }

    /// A failure at the line that would be taken next, or just past the end.
    fn next_line_failure(&self, reason: Malformed) -> Failure {
        Failure::new(self.taken + 1, reason)
    }
}

/// Each broken file's failure in `parsed`, as its line and reason, in input order.
#[cfg(test)]
fn failures(parsed: Result<DiffDocument, NoDiff>) -> Vec<(usize, Malformed)> {
    parsed
        .expect("the input holds a diff")
        .files()
        .filter_map(|file| match &file.body {
            FileBody::Broken(body) => Some((body.failure.line, body.failure.reason)),
            FileBody::Metadata(_) | FileBody::Unified(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        io::{self, BufReader, Read},
        iter,
    };

    use super::{NoDiff, SectionReader, failures, parse_comparison, parse_patch};
    use crate::{
        document::{BrokenBody, DiffDocument, DiffFile, FileBody, FileSection, UnifiedBody},
        file_identity::FileIdentity,
        finite_output,
        malformed::Malformed,
        source_text::SourceLine,
    };

    fn malformed(line: usize, reason: Malformed) -> Vec<(usize, Malformed)> {
        vec![(line, reason)]
    }

    /// Every file's unified body, in input order; each fixture file must be unified.
    fn unified_bodies(document: &DiffDocument) -> Vec<&UnifiedBody> {
        document
            .files()
            .map(|file| match &file.body {
                FileBody::Unified(body) => body.as_ref(),
                FileBody::Metadata(_) | FileBody::Broken(_) => {
                    panic!("fixture section should be unified")
                }
            })
            .collect()
    }

    /// The fixture's only file; a fixture holding any other count fails the test.
    fn only_file(document: &DiffDocument) -> &DiffFile {
        let files = document.files().collect::<Vec<_>>();
        let [file] = files.as_slice() else {
            panic!("the fixture holds one file: {} files", files.len());
        };
        file
    }

    /// Each body's first-hunk record count.
    fn first_hunk_record_counts(bodies: &[&UnifiedBody]) -> Vec<usize> {
        bodies
            .iter()
            .map(|body| body.hunks.first().records.len())
            .collect()
    }

    /// Input that arrives a few bytes per read, then fails when `fails` is set.
    struct Trickle<'a> {
        input: &'a [u8],
        fails: bool,
    }

    impl Read for Trickle<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.input.is_empty() && self.fails {
                return Err(io::Error::other("the producer went away"));
            }

            let count = buffer.len().min(self.input.len()).min(3);
            buffer[..count].copy_from_slice(&self.input[..count]);
            self.input = &self.input[count..];
            Ok(count)
        }
    }

    /// The sections `input` yields when it arrives a few bytes per read, and how reading
    /// it ended.
    fn trickled(input: &[u8], fails: bool) -> (Vec<FileSection>, io::Result<Box<[SourceLine]>>) {
        let mut buffered = BufReader::with_capacity(4, Trickle { input, fails });
        let mut reader = SectionReader::new(&mut buffered);
        let sections = iter::from_fn(|| reader.next_section()).collect();

        (sections, reader.finish())
    }

    const STREAMED: &[u8] = b"intro\n--- a/one\n+++ b/one\n@@ -1 +1 @@\n-a\n+b\n--- a/two\n+++ b/two\n@@ -1,2 +1,2 @@\n-c\nbroken\n--- a/three\n+++ b/three\n@@ -1 +1 @@\n-d\n+e\ntrailer";

    /// docs/spec/performance.md#preparation-boundary: input is parsed as it streams in, so
    /// lines split across reads read as the whole input does.
    #[test]
    fn reads_input_that_arrives_in_pieces_as_the_whole_input() {
        let (sections, trailing) = trickled(STREAMED, false);
        let streamed = DiffDocument::new(sections, trailing.expect("the input reads"));

        assert_eq!(streamed, parse_comparison(STREAMED));
        assert_eq!(streamed.files().count(), 3);
    }

    /// docs/spec/architecture.md#lossless-and-safe-rendering: a read error after partial
    /// input keeps the sections read before it and is reported.
    #[test]
    fn keeps_the_sections_read_before_a_read_error() {
        let (sections, ended) = trickled(STREAMED, true);

        assert_eq!(sections.len(), 3);
        assert_eq!(
            ended.map_err(|error| error.to_string()),
            Err("the producer went away".to_owned())
        );
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
            assert_eq!(parse_patch(input), Err(NoDiff));
        }
        assert_eq!(parse_patch(b""), Ok(DiffDocument::Text(Box::default())));
    }

    /// The broken body of `file`, which must have failed to parse.
    fn broken(file: &DiffFile) -> &BrokenBody {
        match &file.body {
            FileBody::Broken(body) => body,
            FileBody::Metadata(_) | FileBody::Unified(_) => panic!("the file should be broken"),
        }
    }

    /// The sanitized text of each of `lines`.
    fn texts(lines: &[SourceLine]) -> Vec<&str> {
        lines.iter().map(SourceLine::sanitized).collect()
    }

    /// docs/spec/architecture.md#input-contract: a broken file shows what was read before
    /// its failure, its failure, and the rest of its section, and parsing resumes at the
    /// next file header.
    #[test]
    fn keeps_a_broken_file_and_resumes_at_the_next_file_header() {
        let input = "--- a/one\n+++ b/one\n@@ -1 +1 @@\n-a\n+b\n--- a/two.go\n+++ b/two.go\n@@ -1,3 +1,3 @@\n x\n-y\ngarbage\n z\n--- a/three\n+++ b/three\n@@ -1 +1 @@\n-c\n+d\n";
        let document = parse_patch(input.as_bytes()).expect("a diff");
        let files = document.files().collect::<Vec<_>>();
        let [one, two, three] = files[..] else {
            panic!("three files, not {}", files.len());
        };
        let body = broken(two);

        assert!(matches!(one.body, FileBody::Unified(_)));
        assert!(matches!(three.body, FileBody::Unified(_)));
        assert_eq!(
            (body.failure.line, body.failure.reason),
            (11, Malformed::UnmarkedHunkLine)
        );
        assert_eq!(
            body.failure.row.sanitized(),
            "line 11: unmarked line in hunk"
        );
        assert_eq!(body.read.headers.len(), 2);
        assert_eq!(
            body.read
                .hunks
                .iter()
                .map(|hunk| hunk.records.len())
                .collect::<Vec<_>>(),
            [2]
        );
        assert_eq!(texts(&body.rest), ["garbage", " z"]);
        assert_eq!(
            two.identity,
            FileIdentity::of_broken(None, &body.read.headers)
        );
        assert!(matches!(
            &two.identity,
            FileIdentity::Broken { name: Some(name) } if name.as_str() == "two.go"
        ));
    }

    /// docs/spec/architecture.md#input-contract: input that ends part-way through a file
    /// marks that last file broken.
    #[test]
    fn marks_a_file_cut_short_by_the_input_s_end_broken() {
        let input = b"--- a/one\n+++ b/one\n@@ -1,2 +1,2 @@\n-a\n+b\n";
        let document = parse_patch(input).expect("a diff");
        let files = document.files().collect::<Vec<_>>();
        let body = broken(files[0]);

        assert_eq!(
            (body.failure.line, body.failure.reason),
            (6, Malformed::TruncatedHunk)
        );
        assert_eq!(body.read.hunks[0].records.len(), 2);
        assert!(body.rest.is_empty());
    }

    /// docs/spec/architecture.md#file-tree: a broken file whose header gives no name is
    /// unnamed.
    #[test]
    fn reads_a_stray_hunk_header_as_an_unnamed_broken_file() {
        let document = parse_patch(b"text\n@@ -1 +1 @@\n-a\n+b\n").expect("a diff");
        let files = document.files().collect::<Vec<_>>();
        let body = broken(files[0]);

        assert_eq!(files[0].identity, FileIdentity::Broken { name: None });
        assert_eq!(
            (body.failure.line, body.failure.reason),
            (2, Malformed::UnexpectedDiffHeader)
        );
        assert_eq!(texts(&body.rest), ["@@ -1 +1 @@", "-a", "+b"]);
    }

    #[test]
    fn keeps_comparison_output_that_holds_only_text() {
        let input = b"Binary files before.bin and after.bin differ\n";

        let document = parse_comparison(input);

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
                failures(parse_patch(input.as_bytes())),
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
                failures(parse_patch(input.as_bytes())),
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
        let file = only_file(&document);

        assert!(
            matches!(file.body, FileBody::Metadata(_)),
            "a section without a file header is metadata: {:?}",
            file.body
        );
    }

    #[test]
    fn parses_metadata_only_git_fixture() {
        let input = include_bytes!("../tests/fixtures/git-rename-only.patch");

        let document = parse_patch(input).expect("valid metadata-only Git fixture");
        let file = only_file(&document);

        assert!(
            matches!(file.body, FileBody::Metadata(_)),
            "a rename-only section is metadata: {:?}",
            file.body
        );
    }
}
