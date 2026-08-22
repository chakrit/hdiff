use crate::document::{DiffDocument, DiffFile, Hunk, Record, RecordKind, SourceLine};

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

pub fn parse_unified_diff(input: &[u8]) -> Result<DiffDocument, ParseError> {
    let lines = input
        .split_inclusive(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    let mut files = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let mut metadata = Vec::new();
        while index < lines.len() && decoded_line(lines[index]).starts_with("diff --git ") {
            while index < lines.len() && !decoded_line(lines[index]).starts_with("--- ") {
                metadata.push(source_line(lines[index]));
                index += 1;
            }
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
                    break;
                }

                let structural_line = decoded_line(line);
                let (kind, payload) = match structural_line.as_bytes().first() {
                    Some(b' ') => (RecordKind::Context, after_visible_marker(line)),
                    Some(b'+') => (RecordKind::Addition, after_visible_marker(line)),
                    Some(b'-') => (RecordKind::Deletion, after_visible_marker(line)),
                    _ => (RecordKind::Raw, line),
                };
                if kind != RecordKind::Raw {
                    old_seen += (kind != RecordKind::Addition) as usize;
                    new_seen += (kind != RecordKind::Deletion) as usize;
                }
                records.push(Record {
                    kind,
                    payload: source_line(payload),
                });
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
        files.push(DiffFile {
            metadata,
            old_path: old_header[4..].to_owned(),
            new_path: new_header[4..].to_owned(),
            old_header: source_line(old_header_bytes),
            new_header: source_line(new_header_bytes),
            hunks,
        });
    }
    Ok(DiffDocument { files })
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
        if line[index] == b'\x1b' {
            index += 1;
            if line.get(index) == Some(&b'[') {
                index += 1;
                while let Some(byte) = line.get(index) {
                    index += 1;
                    if (b'@'..=b'~').contains(byte) {
                        break;
                    }
                }
            }
            continue;
        }
        if !line[index].is_ascii_control() {
            return &line[index + 1..];
        }
        index += 1;
    }

    unreachable!("record has a visible marker")
}

#[cfg(test)]
mod tests {
    use super::parse_unified_diff;

    #[test]
    fn parses_file_hunk_and_records() {
        let input = "--- a/readme.txt\n+++ b/readme.txt\n@@ -1 +1 @@\n-old\n+new\n";

        let document = parse_unified_diff(input.as_bytes()).expect("valid unified diff");

        assert_eq!(document.files.len(), 1);
        assert_eq!(document.files[0].hunks.len(), 1);
        assert_eq!(document.files[0].hunks[0].records.len(), 2);
    }

    #[test]
    fn parses_git_fixture_with_extended_headers_and_multiline_hunks() {
        let input = include_bytes!("../tests/fixtures/git-multiline.patch");

        let document = parse_unified_diff(input).expect("valid Git fixture");

        assert_eq!(document.files.len(), 2);
        assert_eq!(document.files[0].metadata.len(), 3);
        assert_eq!(document.files[0].hunks[0].records.len(), 3);
        assert_eq!(document.files[1].metadata.len(), 2);
        assert_eq!(document.files[1].hunks[0].records.len(), 4);
    }

    #[test]
    fn parses_colored_git_fixture_with_multiline_hunks() {
        let input = include_bytes!("../tests/fixtures/git-multiline-coloured.patch");

        let document = parse_unified_diff(input).expect("valid colored Git fixture");

        assert_eq!(document.files.len(), 2);
        assert_eq!(document.files[0].hunks[0].records.len(), 3);
        assert_eq!(document.files[1].hunks[0].records.len(), 4);
    }

    #[test]
    fn rejects_git_metadata_without_a_file_header() {
        let input = include_bytes!("../tests/fixtures/git-metadata-without-file-header.patch");

        assert!(parse_unified_diff(input).is_err());
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
        let payload = &document.files[0].hunks[0].records[1].payload;

        assert_eq!(payload.bytes, b"\xffnew\n");
        assert_eq!(payload.text, "\u{fffd}new");
    }

    #[test]
    fn preserves_crlf_in_source_bytes() {
        let input = b"--- a/file\r\n+++ b/file\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n";

        let document = parse_unified_diff(input).expect("valid CRLF diff");
        let payload = &document.files[0].hunks[0].records[0].payload;

        assert_eq!(payload.bytes, b"old\r\n");
        assert_eq!(payload.text, "old");
    }

    #[test]
    fn parses_deletion_payload_that_looks_like_a_file_header() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +0,0 @@\n--- filename\n";

        let document = parse_unified_diff(input).expect("valid deletion record");
        let record = &document.files[0].hunks[0].records[0];

        assert_eq!(record.kind, crate::document::RecordKind::Deletion);
        assert_eq!(record.payload.text, "-- filename");
    }
}
