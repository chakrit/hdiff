//! A unified hunk: its `@@` header and the counts it declares, then the records and
//! no-newline markers inside those counts. Rules:
//! docs/spec/architecture.md#input-contract.

use super::{Error, Malformed, SourceLines, decimal_count, reject_unsupported_header};
use crate::{
    document::{ContextMarker, Hunk, Record, RecordKind},
    headers::{file_header_field, git_header_names},
    side::{BySide, Side},
    source_text::SourceLine,
};

/// The records after a hunk `header` that declares `expected` counts, each with the
/// no-newline marker that follows it, which may follow the last of them. `ended` holds
/// the sides of the file an earlier marker ended, and gains the sides this hunk's marker
/// ends.
pub(super) fn parse_hunk(
    header: SourceLine,
    expected: BySide<usize>,
    ended: &mut BySide<bool>,
    lines: &mut SourceLines<'_>,
) -> Result<Hunk, Error> {
    let exhausted = BySide::<usize>::default();
    let mut remaining = expected;
    let mut records: Vec<Record> = Vec::new();
    while let Some(line) =
        lines.next_if(|text| remaining != exhausted || is_no_newline_marker(text))
    {
        match is_no_newline_marker(line.sanitized()) {
            true => {
                let marked = lines.at_last_line(mark_no_newline(&mut records, line))?;
                *ended = ended.map_sides(|side, done| done || marked.on_side(side));
            }
            false => {
                let record = lines.at_last_line(parse_record(line))?;
                lines.at_last_line(check_side_open(*ended, record.kind))?;
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

/// Checks that a record of `kind` lies on no side of the file a no-newline marker already
/// `ended`.
fn check_side_open(ended: BySide<bool>, kind: RecordKind) -> Result<(), Malformed> {
    let past_end = ended.map_sides(|side, done| done && kind.on_side(side));

    match past_end.into_iter().any(|past| past) {
        true => Err(Malformed::RecordAfterNoNewlineMarker),
        false => Ok(()),
    }
}

/// Gives the last of `records` the no-newline `marker` after it, returning the kind of
/// the record it marks; a marker that follows no record, or a record that already has
/// one, is misplaced.
fn mark_no_newline(records: &mut [Record], marker: SourceLine) -> Result<RecordKind, Malformed> {
    let unmarked = records
        .last_mut()
        .filter(|record| record.no_newline.is_none());
    let Some(record) = unmarked else {
        return Err(Malformed::MisplacedNoNewlineMarker);
    };

    record.no_newline = Some(marker);
    Ok(record.kind)
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
pub(super) fn starts_hunk_header(text: &str) -> bool {
    text.starts_with("@@") && !text.starts_with("@@@")
}

/// The old-side and new-side line counts an `@@ -old +new @@` hunk header declares. The
/// fields are single-spaced, and only a section heading after a space may follow.
pub(super) fn hunk_counts(header: &str) -> Result<BySide<usize>, Malformed> {
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
    match range.split_once(',') {
        Some((start, count)) => {
            decimal_count(start)?;
            decimal_count(count)
        }
        None => decimal_count(range).map(|_| 1),
    }
}
