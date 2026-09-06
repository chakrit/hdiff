use crate::document::{
    DiffDocument, DiffFile, Hunk, Record, RecordKind, Section, SourceLine, UnifiedDiffFile,
};

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

pub fn parse_unified_diff(input: &[u8]) -> Result<DiffDocument, ParseError> {
    let lines = input
        .split_inclusive(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    let mut sections = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = decoded_line(lines[index]);
        reject_unsupported_header(&line, index)?;
        if !line.starts_with("diff --git ") && !line.starts_with("--- ") {
            let mut text = Vec::new();
            while index < lines.len() {
                let line = decoded_line(lines[index]);
                reject_unsupported_header(&line, index)?;
                if line.starts_with("diff --git ") || line.starts_with("--- ") {
                    break;
                }
                text.push(source_line(lines[index]));
                index += 1;
            }
            sections.push(Section::Text(text.into_boxed_slice()));
            continue;
        }

        let mut metadata = Vec::new();
        if line.starts_with("diff --git ") {
            metadata.push(source_line(lines[index]));
            index += 1;
            while index < lines.len() && is_git_metadata(&decoded_line(lines[index])) {
                metadata.push(source_line(lines[index]));
                index += 1;
            }
            if index < lines.len() && decoded_line(lines[index]) == "GIT binary patch" {
                metadata.push(source_line(lines[index]));
                index += 1;
                parse_binary_blocks(&lines, &mut index, &mut metadata)?;
            }
        }

        if index == lines.len() || !decoded_line(lines[index]).starts_with("--- ") {
            sections.push(Section::File(DiffFile::Metadata { lines: metadata }));
            continue;
        }

        let Some(old_header_bytes) = lines.get(index) else {
            return Err(error(index, "missing file header"));
        };
        let old_header = decoded_line(old_header_bytes);
        if !old_header.starts_with("--- ") {
            return Err(error(index, "expected file header"));
        }
        let new_header_bytes = lines
            .get(index + 1)
            .ok_or_else(|| error(index, "missing new file header"))?;
        let new_header = decoded_line(new_header_bytes);
        if !new_header.starts_with("+++ ") {
            return Err(error(index + 1, "expected new file header"));
        }

        let mut hunks = Vec::new();
        index += 2;
        while index < lines.len() && decoded_line(lines[index]).starts_with("@@ ") {
            let header = source_line(lines[index]);
            let expected = hunk_count(&header.structural_text)
                .ok_or_else(|| error(index, "invalid hunk header"))?;
            index += 1;
            let mut records = Vec::new();
            let mut old_seen = 0;
            let mut new_seen = 0;
            while index < lines.len() {
                let line = lines[index];
                let counts_complete = old_seen == expected.0 && new_seen == expected.1;
                let is_no_newline_marker = line.starts_with(b"\\ No newline at end of file");
                if counts_complete && !is_no_newline_marker {
                    reject_excess_record(line, index)?;
                    break;
                }

                let record = parse_record(line, index)?;
                if record.kind != RecordKind::Raw {
                    old_seen += (record.kind != RecordKind::Addition) as usize;
                    new_seen += (record.kind != RecordKind::Deletion) as usize;
                }
                records.push(record);
                index += 1;
            }
            if old_seen != expected.0 || new_seen != expected.1 {
                return Err(error(index, "truncated hunk"));
            }
            hunks.push(Hunk { header, records });
        }
        if hunks.is_empty() {
            return Err(error(index, "file has no hunks"));
        }
        sections.push(Section::File(DiffFile::Unified(Box::new(
            UnifiedDiffFile {
                metadata,
                old_path: old_header[4..].to_owned(),
                new_path: new_header[4..].to_owned(),
                old_header: source_line(old_header_bytes),
                new_header: source_line(new_header_bytes),
                hunks,
            },
        ))));
    }
    let document = DiffDocument { sections };
    if !input.is_empty() && document.files().next().is_none() {
        return Err(error(0, "input contains no diff"));
    }

    Ok(document)
}

fn reject_unsupported_header(line: &str, index: usize) -> Result<(), ParseError> {
    if line.starts_with("diff --cc ")
        || line.starts_with("diff --combined ")
        || line.starts_with("@@@")
    {
        return Err(error(index, "combined diffs are not supported"));
    }
    if line.starts_with("@@") || line.starts_with("+++ ") {
        return Err(error(index, "unexpected diff header"));
    }

    Ok(())
}

fn parse_record(line: &[u8], index: usize) -> Result<Record, ParseError> {
    let structural_line = decoded_line(line);
    let (kind, payload) = match structural_line.as_bytes().first() {
        Some(b' ') => (RecordKind::Context, after_visible_marker(line)),
        Some(b'+') => (RecordKind::Addition, after_visible_marker(line)),
        Some(b'-') => (RecordKind::Deletion, after_visible_marker(line)),
        _ => {
            reject_unsupported_header(&structural_line, index)?;
            if structural_line.starts_with("diff --git ") {
                return Err(error(index, "unexpected file header in hunk"));
            }
            (RecordKind::Raw, line)
        }
    };

    Ok(Record {
        kind,
        payload: source_line(payload),
    })
}

fn reject_excess_record(line: &[u8], index: usize) -> Result<(), ParseError> {
    let structural_line = decoded_line(line);
    if structural_line.starts_with("--- ") {
        return Ok(());
    }
    if matches!(structural_line.as_bytes().first(), Some(b' ' | b'+' | b'-')) {
        return Err(error(index, "hunk records exceed declared counts"));
    }

    Ok(())
}

fn is_git_metadata(line: &str) -> bool {
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
    .any(|prefix| line.starts_with(prefix))
        || (line.starts_with("Binary files ") && line.ends_with(" differ"))
}

fn parse_binary_blocks(
    lines: &[&[u8]],
    index: &mut usize,
    metadata: &mut Vec<SourceLine>,
) -> Result<(), ParseError> {
    let mut blocks = 0;
    while *index < lines.len() {
        let header = decoded_line(lines[*index]);
        let size = if let Some(size) = header.strip_prefix("literal ") {
            size
        } else if let Some(size) = header.strip_prefix("delta ") {
            size
        } else {
            break;
        };
        size.parse::<usize>()
            .map_err(|_| error(*index, "invalid binary patch header"))?;
        metadata.push(source_line(lines[*index]));
        *index += 1;
        let payload_start = *index;
        while *index < lines.len() && !decoded_line(lines[*index]).is_empty() {
            let line = decoded_line(lines[*index]);
            reject_unsupported_header(&line, *index)?;
            if line.starts_with("diff --") || line.starts_with("--- ") {
                return Err(error(*index, "truncated binary patch"));
            }
            metadata.push(source_line(lines[*index]));
            *index += 1;
        }
        if *index == payload_start {
            return Err(error(*index, "missing binary patch payload"));
        }
        if *index < lines.len() {
            metadata.push(source_line(lines[*index]));
            *index += 1;
        }
        blocks += 1;
    }
    if blocks == 0 {
        return Err(error(*index, "missing binary patch block"));
    }

    Ok(())
}

fn source_line(bytes: &[u8]) -> SourceLine {
    SourceLine::from_bytes(bytes)
}

fn decoded_line(bytes: &[u8]) -> String {
    SourceLine::from_bytes(bytes).structural_text
}

fn error(line: usize, message: &str) -> ParseError {
    ParseError {
        message: format!("line {}: {message}", line + 1),
    }
}

fn hunk_count(header: &str) -> Option<(usize, usize)> {
    let mut parts = header.split_whitespace();
    (parts.next()? == "@@").then_some(())?;
    let old = range_count(parts.next()?, '-')?;
    let new = range_count(parts.next()?, '+')?;
    (parts.next()? == "@@").then_some(())?;

    Some((old, new))
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

fn after_visible_marker(line: &[u8]) -> &[u8] {
    let mut index = 0;
    while index < line.len() {
        match (line[index], line.get(index + 1)) {
            (b'\x1b', Some(b'[')) => index = after_control_sequence(line, index + 2),
            (b'\x1b', Some(b']' | b'P' | b'X' | b'^' | b'_')) => {
                index = after_string_sequence(line, index + 2);
            }
            (b'\x1b', _) => index += 2,
            (byte, _) if !byte.is_ascii_control() => return &line[index + 1..],
            _ => index += 1,
        }
    }

    unreachable!("record has a visible marker")
}

fn after_control_sequence(line: &[u8], mut index: usize) -> usize {
    while let Some(byte) = line.get(index) {
        index += 1;
        if (b'@'..=b'~').contains(byte) {
            break;
        }
    }

    index
}

fn after_string_sequence(line: &[u8], mut index: usize) -> usize {
    while index < line.len() {
        match (line[index], line.get(index + 1)) {
            (b'\x07', _) => return index + 1,
            (b'\x1b', Some(b'\\')) => return index + 2,
            _ => index += 1,
        }
    }

    index
}

#[cfg(test)]
mod tests {
    use super::parse_unified_diff;
    use crate::document::{DiffDocument, DiffFile, Hunk};

    #[test]
    fn preserves_surrounding_text_in_source_order() {
        let input = b"commit message\r\n\r\n--- a/one\n+++ b/one\n@@ -1 +1 @@\n-old\n+new\n\nbetween files\n--- a/two\n+++ b/two\n@@ -1 +1 @@\n-before\n+after\ntrailing text";

        let document = parse_unified_diff(input).expect("diff with surrounding text");

        assert_eq!(crate::render::render_unified(&document), input);
    }

    #[test]
    fn rejects_nonempty_text_only_input() {
        for input in [b"ordinary text\n".as_slice(), b" \t\r\n", b"\n"] {
            assert!(parse_unified_diff(input).is_err());
        }
        assert_eq!(parse_unified_diff(b""), Ok(DiffDocument::default()));
    }

    #[test]
    fn rejects_malformed_and_combined_sections_after_valid_diff() {
        let valid = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        for suffix in [
            "@@ malformed\n",
            "@@@ -1,1 -1,1 +1,1 @@@\n",
            "diff --cc file\n",
            "diff --combined file\n",
            "--- a/second\nmissing header\n",
            "+++ b/second\n",
        ] {
            let input = format!("{valid}{suffix}");
            assert!(parse_unified_diff(input.as_bytes()).is_err(), "{suffix}");
        }
    }

    #[test]
    fn rejects_diff_headers_inside_incomplete_hunks() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n@@ malformed\n-old\n+new\n";

        assert!(parse_unified_diff(input).is_err());
    }

    #[test]
    fn rejects_records_exceeding_hunk_counts() {
        let valid = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        for suffix in ["-extra\n", "+extra\n", " context\n"] {
            let input = format!("{valid}{suffix}");

            assert!(parse_unified_diff(input.as_bytes()).is_err(), "{suffix}");
        }
    }

    #[test]
    fn keeps_text_separate_from_metadata_only_files() {
        let input =
            b"message\ndiff --git a/file b/file\nold mode 100644\nnew mode 100755\nfooter\n";
        let document = parse_unified_diff(input).expect("mode-only diff with text");
        let views = document.file_views().collect::<Vec<_>>();
        let DiffFile::Metadata { lines } = views[0].file else {
            panic!("mode-only diff is metadata");
        };

        assert_eq!(views.len(), 1);
        assert_eq!(lines.len(), 3);
        assert_eq!(views[0].leading[0].text, "message");
        assert_eq!(views[0].trailing[0].text, "footer");
        assert_eq!(crate::render::render_unified(&document), input);
    }

    #[test]
    fn keeps_binary_payload_and_surrounding_text_in_order() {
        let input = b"message\ndiff --git a/image b/image\nindex 1..2 100644\nGIT binary patch\nliteral 3\nKcmZQzWC8#H2LJ>B\n\nliteral 0\nHcmV?d00001\n\nfooter\n";
        let document = parse_unified_diff(input).expect("binary diff with text");
        let view = document.file_views().next().expect("binary file");
        let DiffFile::Metadata { lines } = view.file else {
            panic!("binary diff is metadata");
        };

        assert_eq!(view.trailing[0].text, "footer");
        assert!(lines.iter().any(|line| line.text == "KcmZQzWC8#H2LJ>B"));
        assert_eq!(crate::render::render_unified(&document), input);
    }

    fn single_unified_hunks(document: &DiffDocument) -> &[Hunk] {
        assert_eq!(document.files().count(), 1);
        let DiffFile::Unified(file) = document.file(0).expect("first file") else {
            panic!("fixture section should be unified");
        };

        &file.hunks
    }

    #[test]
    fn parses_file_hunk_and_records() {
        let input = "--- a/readme.txt\n+++ b/readme.txt\n@@ -1 +1 @@\n-old\n+new\n";

        let document = parse_unified_diff(input.as_bytes()).expect("valid unified diff");

        assert_eq!(document.files().count(), 1);
        assert_eq!(single_unified_hunks(&document).len(), 1);
        assert_eq!(single_unified_hunks(&document)[0].records.len(), 2);
    }

    #[test]
    fn parses_git_fixture_with_extended_headers_and_multiline_hunks() {
        let input = include_bytes!("../tests/fixtures/git-multiline.patch");

        let document = parse_unified_diff(input).expect("valid Git fixture");

        assert_eq!(document.files().count(), 2);
        let DiffFile::Unified(file) = document.file(0).expect("first file") else {
            panic!("first fixture section should be unified");
        };
        assert_eq!(file.metadata.len(), 3);
        assert_eq!(file.hunks[0].records.len(), 3);
        let DiffFile::Unified(file) = document.file(1).expect("second file") else {
            panic!("second fixture section should be unified");
        };
        assert_eq!(file.metadata.len(), 2);
        assert_eq!(file.hunks[0].records.len(), 4);
    }

    #[test]
    fn parses_colored_git_fixture_with_multiline_hunks() {
        let input = include_bytes!("../tests/fixtures/git-multiline-coloured.patch");

        let document = parse_unified_diff(input).expect("valid colored Git fixture");

        assert_eq!(document.files().count(), 2);
        let DiffFile::Unified(file) = document.file(0).expect("first file") else {
            panic!("first fixture section should be unified");
        };
        assert_eq!(file.hunks[0].records.len(), 3);
        let DiffFile::Unified(file) = document.file(1).expect("second file") else {
            panic!("second fixture section should be unified");
        };
        assert_eq!(file.hunks[0].records.len(), 4);
    }

    #[test]
    fn parses_git_metadata_without_a_file_header() {
        let input = include_bytes!("../tests/fixtures/git-metadata-without-file-header.patch");

        let document = parse_unified_diff(input).expect("valid metadata-only Git fixture");

        assert!(matches!(
            document.file(0).expect("first file"),
            DiffFile::Metadata { .. }
        ));
    }

    #[test]
    fn parses_metadata_only_git_fixture() {
        let input = include_bytes!("../tests/fixtures/git-rename-only.patch");

        let document = parse_unified_diff(input).expect("valid metadata-only Git fixture");

        assert_eq!(document.files().count(), 1);
        assert!(matches!(
            document.file(0).expect("first file"),
            DiffFile::Metadata { .. }
        ));
    }

    #[test]
    fn rejects_truncated_hunk() {
        let input = "--- a/readme.txt\n+++ b/readme.txt\n@@ -1,2 +1,2 @@\n-old\n";

        assert!(parse_unified_diff(input.as_bytes()).is_err());
    }

    #[test]
    fn preserves_invalid_utf8_with_lossy_display_text() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+\xffnew\n";

        let document = parse_unified_diff(input).expect("structurally valid diff");
        let payload = &single_unified_hunks(&document)[0].records[1].payload;

        assert_eq!(payload.bytes, b"\xffnew\n");
        assert_eq!(payload.text, "\u{fffd}new");
    }

    #[test]
    fn preserves_crlf_in_source_bytes() {
        let input = b"--- a/file\r\n+++ b/file\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n";

        let document = parse_unified_diff(input).expect("valid CRLF diff");
        let payload = &single_unified_hunks(&document)[0].records[0].payload;

        assert_eq!(payload.bytes, b"old\r\n");
        assert_eq!(payload.text, "old");
    }

    #[test]
    fn parses_deletion_payload_that_looks_like_a_file_header() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +0,0 @@\n--- filename\n";

        let document = parse_unified_diff(input).expect("valid deletion record");
        let record = &single_unified_hunks(&document)[0].records[0];

        assert_eq!(record.kind, crate::document::RecordKind::Deletion);
        assert_eq!(record.payload.text, "-- filename");
    }
}
