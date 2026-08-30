use crate::document::{DiffDocument, DiffFile, RecordKind, SourceLine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordAddress {
    pub hunk_index: usize,
    pub record_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderedLineKind {
    Metadata,
    FileHeader,
    HunkHeader,
    Record(RecordAddress),
}

#[derive(Debug, PartialEq, Eq)]
pub struct RenderedLine {
    pub bytes: Vec<u8>,
    pub kind: RenderedLineKind,
}

pub fn render_unified(document: &DiffDocument) -> Vec<u8> {
    let mut output = Vec::new();

    for file in &document.files {
        output.extend_from_slice(&render_file(file));
    }

    output
}

pub fn render_file(file: &DiffFile) -> Vec<u8> {
    render_file_lines(file)
        .into_iter()
        .flat_map(|line| line.bytes)
        .collect()
}

pub fn render_file_lines(file: &DiffFile) -> Vec<RenderedLine> {
    let mut rendered_lines = Vec::new();

    match file {
        DiffFile::Metadata { lines } => {
            for line in lines {
                rendered_lines.push(rendered_line(RenderedLineKind::Metadata, None, line));
            }
        }
        DiffFile::Unified(file) => {
            for line in &file.metadata {
                rendered_lines.push(rendered_line(RenderedLineKind::Metadata, None, line));
            }
            rendered_lines.push(rendered_line(
                RenderedLineKind::FileHeader,
                None,
                &file.old_header,
            ));
            rendered_lines.push(rendered_line(
                RenderedLineKind::FileHeader,
                None,
                &file.new_header,
            ));
            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                rendered_lines.push(rendered_line(
                    RenderedLineKind::HunkHeader,
                    None,
                    &hunk.header,
                ));
                for (record_index, record) in hunk.records.iter().enumerate() {
                    let address = RecordAddress {
                        hunk_index,
                        record_index,
                    };
                    rendered_lines.push(rendered_line(
                        RenderedLineKind::Record(address),
                        marker(&record.kind),
                        &record.payload,
                    ));
                }
            }
        }
    }

    rendered_lines
}

fn rendered_line(kind: RenderedLineKind, marker: Option<u8>, line: &SourceLine) -> RenderedLine {
    let line_ending: &[u8] = if line.bytes.ends_with(b"\r\n") {
        b"\r\n"
    } else if line.bytes.ends_with(b"\n") {
        b"\n"
    } else {
        b""
    };
    let final_length =
        usize::from(marker.is_some()) + line.structural_text.len() + line_ending.len();
    let mut bytes = Vec::with_capacity(final_length);

    if let Some(marker) = marker {
        bytes.push(marker);
    }
    bytes.extend_from_slice(line.structural_text.as_bytes());
    bytes.extend_from_slice(line_ending);

    RenderedLine { bytes, kind }
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
    use super::{RecordAddress, RenderedLineKind, render_file, render_file_lines, render_unified};
    use crate::{document::DiffFile, parser::parse_unified_diff};

    #[test]
    fn renders_unified_structure_in_source_order() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        let document = parse_unified_diff(input).expect("valid diff");

        let rendered = render_unified(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn classifies_metadata_file_headers_hunks_and_records() {
        let input = b"diff --git a/file b/file\nindex 1..2 100644\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n@@ -3 +3 @@\n-before\n+after\n";
        let document = parse_unified_diff(input).expect("valid Git diff with two hunks");
        let DiffFile::Unified(file) = &document.files[0] else {
            panic!("expected unified file");
        };

        let lines = render_file_lines(&document.files[0]);
        let kinds = lines.iter().map(|line| line.kind).collect::<Vec<_>>();

        assert_eq!(kinds[0], RenderedLineKind::Metadata);
        assert_eq!(kinds[1], RenderedLineKind::Metadata);
        assert_eq!(kinds[2], RenderedLineKind::FileHeader);
        assert_eq!(kinds[3], RenderedLineKind::FileHeader);
        assert_eq!(kinds[4], RenderedLineKind::HunkHeader);
        assert_eq!(kinds[7], RenderedLineKind::HunkHeader);
        assert_eq!(
            kinds[5],
            RenderedLineKind::Record(RecordAddress {
                hunk_index: 0,
                record_index: 0,
            })
        );
        assert_eq!(
            kinds[8],
            RenderedLineKind::Record(RecordAddress {
                hunk_index: 1,
                record_index: 0,
            })
        );
        assert_eq!(file.hunks.len(), 2);
    }

    #[test]
    fn renders_lines_in_exact_sized_buffers() {
        let input = b"--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n";
        let document = parse_unified_diff(input).expect("valid diff");

        let lines = render_file_lines(&document.files[0]);

        for line in lines {
            assert_eq!(line.bytes.capacity(), line.bytes.len());
        }
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
