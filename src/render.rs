use crate::document::{DiffDocument, DiffFile, RecordKind, SourceLine, sanitize};

pub fn render_unified(document: &DiffDocument) -> Vec<u8> {
    let mut output = Vec::new();

    for file in &document.files {
        output.extend_from_slice(&render_file(file));
    }

    output
}

pub fn render_file(file: &DiffFile) -> Vec<u8> {
    let mut output = Vec::new();

    match file {
        DiffFile::Metadata { lines } => {
            for line in lines {
                append_line(&mut output, None, line);
            }
        }
        DiffFile::Unified(file) => {
            for line in &file.metadata {
                append_line(&mut output, None, line);
            }
            append_line(&mut output, None, &file.old_header);
            append_line(&mut output, None, &file.new_header);
            for hunk in &file.hunks {
                append_line(&mut output, None, &hunk.header);
                for record in &hunk.records {
                    append_line(&mut output, marker(&record.kind), &record.payload);
                }
            }
        }
    }

    output
}

fn append_line(output: &mut Vec<u8>, marker: Option<u8>, line: &SourceLine) {
    if let Some(marker) = marker {
        output.push(marker);
    }
    output.extend_from_slice(sanitize(&line.text).as_bytes());
    if line.bytes.ends_with(b"\r\n") {
        output.extend_from_slice(b"\r\n");
    } else if line.bytes.ends_with(b"\n") {
        output.push(b'\n');
    }
}

fn marker(kind: &RecordKind) -> Option<u8> {
    match kind {
        RecordKind::Context => Some(b' '),
        RecordKind::Addition => Some(b'+'),
        RecordKind::Deletion => Some(b'-'),
        RecordKind::Raw => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{render_file, render_unified};
    use crate::parser::parse_unified_diff;

    #[test]
    fn renders_unified_structure_in_source_order() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        let document = parse_unified_diff(input).expect("valid diff");

        let rendered = render_unified(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn renders_git_fixture_in_original_order() {
        let input = include_bytes!("../tests/fixtures/git-multiline.patch");

        let document = parse_unified_diff(input).expect("valid Git fixture");
        let rendered = render_unified(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn renders_metadata_only_git_fixture_in_original_order() {
        let input = include_bytes!("../tests/fixtures/git-rename-only.patch");

        let document = parse_unified_diff(input).expect("valid metadata-only Git fixture");
        let rendered = render_unified(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn renders_colored_git_fixture_without_terminal_controls() {
        let input = include_bytes!("../tests/fixtures/git-multiline-coloured.patch");

        let document = parse_unified_diff(input).expect("valid colored Git fixture");
        let rendered = render_unified(&document);

        assert_eq!(
            rendered,
            include_bytes!("../tests/fixtures/git-multiline.patch")
        );
    }

    #[test]
    fn removes_control_sequences_before_a_record_marker() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n\x1b]8;;https://bad\x07+new\n";

        let document = parse_unified_diff(input).expect("valid diff with a controlled record");
        let rendered = render_unified(&document);

        assert_eq!(
            rendered,
            b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n"
        );
    }

    #[test]
    fn preserves_printable_text_and_neutralizes_terminal_controls() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+tab\t e\xcc\x81 \xe7\x95\x8c \x1b[31mred\x1b[0m \x1b]8;;https://bad\x07link\x1bPbad\x1b\\\x01\n";
        let document = parse_unified_diff(input).expect("valid diff");

        let rendered = render_unified(&document);
        let text = String::from_utf8(rendered).expect("safe UTF-8 output");

        assert!(text.contains("tab\t e\u{301} \u{754c} red link"));
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{1}'));
        assert!(!text.contains("https://bad"));
    }

    #[test]
    fn replaces_invalid_utf8_without_losing_following_text() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+\xffnew\n";
        let document = parse_unified_diff(input).expect("valid diff");

        let rendered = render_unified(&document);
        let text = String::from_utf8(rendered).expect("safe UTF-8 output");

        assert!(text.contains("+\u{fffd}new\n"));
    }

    #[test]
    fn preserves_raw_no_newline_marker_without_adding_a_line_ending() {
        let input =
            b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file";
        let document = parse_unified_diff(input).expect("valid diff with raw marker");

        let rendered = render_unified(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn renders_one_file_with_the_same_safe_output_as_the_document_renderer() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-\x1b[31mold\x1b[0m\n+new\n";
        let document = parse_unified_diff(input).expect("valid diff");

        let rendered = render_file(&document.files[0]);

        assert_eq!(rendered, render_unified(&document));
        assert!(!rendered.contains(&b'\x1b'));
    }
}
