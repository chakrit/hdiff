use crate::document::{DiffDocument, DiffFile, Hunk, Record, RecordKind};

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

pub fn parse_unified_diff(input: &str) -> Result<DiffDocument, ParseError> {
    let lines: Vec<&str> = input.lines().collect();
    let mut files = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let old_header = lines[index];
        if !old_header.starts_with("--- ") {
            return Err(error(index, "expected file header"));
        }
        let new_header = lines
            .get(index + 1)
            .ok_or_else(|| error(index, "missing new file header"))?;
        if !new_header.starts_with("+++ ") {
            return Err(error(index + 1, "expected new file header"));
        }

        let mut hunks = Vec::new();
        index += 2;
        while index < lines.len() && lines[index].starts_with("@@ ") {
            let header = lines[index].to_owned();
            let expected =
                hunk_count(&header).ok_or_else(|| error(index, "invalid hunk header"))?;
            index += 1;
            let mut records = Vec::new();
            let mut old_seen = 0;
            let mut new_seen = 0;
            while index < lines.len()
                && !lines[index].starts_with("--- ")
                && !lines[index].starts_with("@@ ")
            {
                let line = lines[index];
                let (kind, payload) = match line.chars().next() {
                    Some(' ') => (RecordKind::Context, &line[1..]),
                    Some('+') => (RecordKind::Addition, &line[1..]),
                    Some('-') => (RecordKind::Deletion, &line[1..]),
                    _ => (RecordKind::Raw, line),
                };
                if kind != RecordKind::Raw {
                    old_seen += (kind != RecordKind::Addition) as usize;
                    new_seen += (kind != RecordKind::Deletion) as usize;
                }
                records.push(Record {
                    kind,
                    payload: payload.to_owned(),
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
            old_path: old_header[4..].to_owned(),
            new_path: new_header[4..].to_owned(),
            hunks,
        });
    }
    Ok(DiffDocument { files })
}

fn error(line: usize, message: &str) -> ParseError {
    ParseError {
        message: format!("line {}: {message}", line + 1),
    }
}

fn hunk_count(header: &str) -> Option<(usize, usize)> {
    let mut parts = header.split_whitespace();
    parts.next()?;
    let old = parts
        .next()?
        .trim_start_matches('-')
        .split(',')
        .next()?
        .parse()
        .ok()?;
    let new = parts
        .next()?
        .trim_start_matches('+')
        .split(',')
        .next()?
        .parse()
        .ok()?;
    Some((old, new))
}

#[cfg(test)]
mod tests {
    use super::parse_unified_diff;

    #[test]
    fn parses_file_hunk_and_records() {
        let input = "--- a/readme.txt\n+++ b/readme.txt\n@@ -1 +1 @@\n-old\n+new\n";

        let document = parse_unified_diff(input).expect("valid unified diff");

        assert_eq!(document.files.len(), 1);
        assert_eq!(document.files[0].hunks.len(), 1);
        assert_eq!(document.files[0].hunks[0].records.len(), 2);
    }

    #[test]
    fn rejects_truncated_hunk() {
        let input = "--- a/readme.txt\n+++ b/readme.txt\n@@ -1,2 +1,2 @@\n-old\n";

        assert!(parse_unified_diff(input).is_err());
    }
}
