//! A unified hunk: its `@@` header and the counts it declares, then the records those
//! counts hold, any of which, the last included, may be followed by one no-newline
//! marker. Rules: docs/spec/architecture.md#input-contract.

use super::{Error, Malformed, SourceLines, decimal_count, is_decimal, reject_unsupported_header};
use crate::{
    document::{ContextMarker, Hunk, Record, RecordKind},
    headers::{file_header_field, git_header_names},
    side::{BySide, Side},
    source_text::SourceLine,
};

/// The records after a hunk `header` that declares `expected` counts. Any record, the
/// last included, may be followed by one no-newline marker, which ends its side of the
/// file. `earlier` holds the sides earlier hunks' markers ended.
pub(super) fn parse_hunk(
    header: SourceLine,
    expected: BySide<usize>,
    earlier: BySide<bool>,
    lines: &mut SourceLines<'_>,
) -> Result<Hunk, Error> {
    let exhausted = BySide::default();
    let mut remaining = expected;
    let mut ended = earlier;
    let mut records = Vec::new();
    while let Some(hunk_line) = next_hunk_line(lines, remaining != exhausted) {
        match hunk_line {
            HunkLine::NoNewlineMarker(line) => {
                let marked = lines.at_last_line(mark_no_newline(&mut records, line))?;
                ended = ended.or(marked);
            }
            HunkLine::Record(line) => {
                let record = lines.at_last_line(parse_record(line))?;
                lines.at_last_line(check_side_open(ended, record.kind))?;
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

/// A line inside a hunk, read once as a no-newline marker or as a line holding a record.
enum HunkLine {
    NoNewlineMarker(SourceLine),
    Record(SourceLine),
}

/// Takes the next line of a hunk: a no-newline marker whenever one comes, and any other
/// line only while the declared counts are `open`.
fn next_hunk_line(lines: &mut SourceLines<'_>, open: bool) -> Option<HunkLine> {
    let is_marker = is_no_newline_marker(lines.peek()?.sanitized());

    match (is_marker, open) {
        (true, true | false) => lines.next().map(HunkLine::NoNewlineMarker),
        (false, true) => lines.next().map(HunkLine::Record),
        (false, false) => None,
    }
}

/// The counts `remaining` after one record of `kind`; a record on a side whose declared
/// count is already used up is the first excess record.
fn count_record(remaining: BySide<usize>, kind: RecordKind) -> Result<BySide<usize>, Malformed> {
    let counted = remaining
        .zip(kind.sides())
        .map(|(count, on_side)| match on_side {
            true => count.checked_sub(1),
            false => Some(count),
        });

    counted.transpose().ok_or(Malformed::ExcessRecords)
}

/// Checks that a record of `kind` lies on no side of the file a no-newline marker already
/// `ended`.
fn check_side_open(ended: BySide<bool>, kind: RecordKind) -> Result<(), Malformed> {
    let past_end = ended
        .zip(kind.sides())
        .map(|(side_ended, on_side)| side_ended && on_side);

    match past_end.into_iter().any(|past| past) {
        true => Err(Malformed::RecordAfterNoNewlineMarker),
        false => Ok(()),
    }
}

/// Gives the last of `records` the no-newline `marker` after it, returning the sides of
/// the file it ends; a marker that follows no record, or a record that already has one,
/// is misplaced.
fn mark_no_newline(records: &mut [Record], marker: SourceLine) -> Result<BySide<bool>, Malformed> {
    let unmarked = records
        .last_mut()
        .filter(|record| record.no_newline.is_none());
    let Some(record) = unmarked else {
        return Err(Malformed::MisplacedNoNewlineMarker);
    };

    record.no_newline = Some(marker);
    Ok(record.ended_sides())
}

/// The record a hunk line holds: a record marked by its first visible character, or blank
/// context when the line is empty. A diff header there is unexpected, and any other line
/// is unmarked.
fn parse_record(line: SourceLine) -> Result<Record, Malformed> {
    if let Some(record) = marked_record(&line) {
        return Ok(record);
    }

    let text = line.sanitized();
    if text.is_empty() {
        return Ok(Record {
            kind: RecordKind::Context(ContextMarker::Blank),
            payload: line,
            no_newline: None,
        });
    }

    reject_unsupported_header(text)?;
    match git_header_names(text) {
        Some(_) => Err(Malformed::UnexpectedGitHeaderInHunk),
        None => Err(Malformed::UnmarkedHunkLine),
    }
}

/// Whether `text` is a no-newline marker. Producers translate its wording, so hdiff knows
/// it by its `\ ` prefix alone, whatever follows.
fn is_no_newline_marker(text: &str) -> bool {
    text.starts_with("\\ ")
}

/// The record `line` holds when its first visible character is a record marker.
fn marked_record(line: &SourceLine) -> Option<Record> {
    let (marker, payload) = line.split_plain_first()?;
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
pub(super) fn starts_hunk_header(text: &str) -> bool {
    text.starts_with("@@") && !starts_combined_hunk_header(text)
}

/// Whether `text` starts a combined diff's `@@@` hunk header, which hdiff rejects.
pub(super) fn starts_combined_hunk_header(text: &str) -> bool {
    text.starts_with("@@@")
}

/// The old-side and new-side line counts an `@@ -old +new @@` hunk header declares. The
/// fields are single-spaced, and only a section heading after a space may follow.
pub(super) fn hunk_counts(header: &str) -> Result<BySide<usize>, Malformed> {
    let ranges = header
        .strip_prefix("@@ -")
        .and_then(|rest| rest.split_once(" @@"))
        .filter(|(_, heading)| heading.is_empty() || heading.starts_with(' '));
    let Some((old, new)) = ranges.and_then(|(range_fields, _)| range_fields.split_once(" +"))
    else {
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
    match range.split_once(',') {
        Some((start, _)) if !is_decimal(start) => None,
        Some((_, count)) => decimal_count(count),
        None => is_decimal(range).then_some(1),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        document::{ContextMarker, DiffDocument, FileBody, Hunk, Record, RecordKind},
        finite_output,
        parser::{Error, Malformed, parse_patch},
    };

    fn malformed(line: usize, reason: Malformed) -> Result<DiffDocument, Error> {
        Err(Error::Malformed { line, reason })
    }

    /// The first hunk of a document holding one unified file.
    fn first_hunk(document: &DiffDocument) -> &Hunk {
        let files = document.files().collect::<Vec<_>>();
        let [file] = files[..] else {
            panic!("fixture should hold one file: {} files", files.len());
        };
        let FileBody::Unified(body) = &file.body else {
            panic!("fixture section should be unified");
        };

        body.hunks.first()
    }

    /// The `N` records of the first hunk of a document holding one unified file.
    fn first_hunk_records<const N: usize>(document: &DiffDocument) -> &[Record; N] {
        let records = &first_hunk(document).records;

        records.as_ref().try_into().unwrap_or_else(|error| {
            panic!(
                "the first hunk holds {} records, not {N}: {error}",
                records.len()
            )
        })
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
    }

    /// docs/spec/architecture.md#input-contract: a range's start is any run of ASCII
    /// digits, and a count too large to hold makes the header invalid.
    #[test]
    fn reads_a_start_of_any_length_and_rejects_a_count_too_large_to_hold() {
        let files = "--- a/file\n+++ b/file\n";
        let long_start = format!("{files}@@ -{0},1 +{0},1 @@\n-old\n+new\n", "9".repeat(30));
        let long_count = format!("{files}@@ -1,{} +1 @@\n-old\n+new\n", "9".repeat(30));

        assert!(
            parse_patch(long_start.as_bytes()).is_ok(),
            "{long_start:?} has a long start"
        );
        assert_eq!(
            parse_patch(long_count.as_bytes()),
            malformed(3, Malformed::InvalidHunkHeader),
            "{long_count:?} has a count too large to hold"
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
        let FileBody::Unified(first_body) = &first.file.body else {
            panic!("first file is unified");
        };
        let marker = first_body
            .hunks
            .first()
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

    /// docs/spec/architecture.md#input-contract: a no-newline marker ends its record's
    /// side of the file, so no later record on that side follows it, in its hunk or a
    /// later one.
    #[test]
    fn rejects_a_record_after_the_marker_that_ended_its_side() {
        let files = "--- a/f\n+++ b/f\n";
        let marker = "\\ No newline at end of file\n";
        for (hunks, line) in [
            (format!("@@ -1,2 +1 @@\n-a\n{marker}-b\n+c\n"), 6),
            (
                format!("@@ -1 +1 @@\n-a\n{marker}+c\n@@ -5 +5 @@\n-x\n+y\n"),
                8,
            ),
            (format!("@@ -1,2 +0,0 @@\n-a\n{marker}-b\n{marker}"), 6),
            (format!("@@ -1,2 +1,2 @@\n a\n{marker}-b\n+c\n"), 6),
        ] {
            let input = format!("{files}{hunks}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(line, Malformed::RecordAfterNoNewlineMarker),
                "{hunks}"
            );
        }

        let other_side = format!("{files}@@ -1 +1 @@\n-a\n{marker}+c\n{marker}");
        assert!(
            parse_patch(other_side.as_bytes()).is_ok(),
            "an addition follows the old side's marker"
        );
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
        let files = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n";
        for (header, reason) in [
            ("@@ malformed", Malformed::UnexpectedDiffHeader),
            ("diff --git a/g b/g", Malformed::UnexpectedGitHeaderInHunk),
        ] {
            let input = format!("{files}{header}\n-old\n+new\n");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(4, reason),
                "{header}"
            );
        }
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
        let [_, addition] = first_hunk_records(&document);
        let payload = &addition.payload;

        assert_eq!(payload.bytes(), b"\xffnew\n");
        assert_eq!(payload.sanitized(), "\u{fffd}new");
    }

    #[test]
    fn preserves_crlf_in_source_bytes() {
        let input = b"--- a/file\r\n+++ b/file\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n";

        let document = parse_patch(input).expect("valid CRLF diff");
        let [deletion, _] = first_hunk_records(&document);
        let payload = &deletion.payload;

        assert_eq!(payload.bytes(), b"old\r\n");
        assert_eq!(payload.sanitized(), "old");
    }

    /// docs/spec/architecture.md#lossless-and-safe-rendering: parsing uses ANSI-free
    /// text, so the payload starts after the marker the sanitized text shows.
    #[test]
    fn cuts_record_payloads_after_the_marker_sanitizing_recognizes() {
        for (line, payload) in [
            (b"\xc2\x85-old\n".as_slice(), b"old\n".as_slice()),
            (b"\x1b]x\x07\x1b]y\x1b\\-old\n", b"old\n"),
        ] {
            let input = [b"--- a/file\n+++ b/file\n@@ -1 +0,0 @@\n".as_slice(), line].concat();

            let document = parse_patch(&input).expect("valid deletion");
            let [record] = first_hunk_records(&document);

            assert_eq!(record.kind, RecordKind::Deletion);
            assert_eq!(record.payload.bytes(), payload, "{line:?}");
        }
    }

    #[test]
    fn parses_deletion_payload_that_looks_like_a_file_header() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +0,0 @@\n--- filename\n";

        let document = parse_patch(input).expect("valid deletion record");
        let [record] = first_hunk_records(&document);

        assert_eq!(record.kind, RecordKind::Deletion);
        assert_eq!(record.payload.sanitized(), "-- filename");
    }
}
