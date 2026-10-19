//! Finite output: the document's safe text, written without the interactive preparation
//! and terminal path. Rules: docs/spec/architecture.md#lossless-and-safe-rendering.

use crate::{display_lines, document::DiffDocument};

/// The document as finite output: every displayed line, in input order.
pub fn of_document(document: &DiffDocument) -> Vec<u8> {
    let mut output = String::new();

    for line in display_lines::of_document(document) {
        line.write_text(&mut output);
    }

    output.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::of_document;
    use crate::parser::parse_patch;

    #[test]
    fn sanitizes_surrounding_text_and_preserves_line_endings() {
        let input = b"\x1b[31mmessage\x1b[0m\r\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n\x1b]8;;https://bad\x07footer\xff";
        let document = parse_patch(input).expect("diff with surrounding text");

        let rendered = of_document(&document);

        assert_eq!(
            rendered,
            "message\r\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\nfooter\u{fffd}".as_bytes()
        );
    }

    #[test]
    fn writes_diff_structure_in_source_order() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        let document = parse_patch(input).expect("valid diff");

        let rendered = of_document(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn renders_git_fixture_in_original_order() {
        let input = include_bytes!("../tests/fixtures/git-multiline.patch");
        let document = parse_patch(input).expect("valid Git fixture");

        let rendered = of_document(&document);

        assert_eq!(rendered, input);
    }

    /// docs/spec/architecture.md#lossless-and-safe-rendering: Git's colored output of a
    /// CRLF file paints an added line's CR as a whitespace error, inside SGR codes; the
    /// output still ends each line in the source's CRLF, exactly as the plain output of
    /// the same change does.
    #[test]
    fn writes_the_crlf_ending_git_colors_as_a_whitespace_error() {
        let plain_patch =
            b"--- a/crlf.txt\n+++ b/crlf.txt\n@@ -1,3 +1,3 @@\n a\r\n \r\n-b\r\n+c\r\n";
        let colored_patch = b"\x1b[1m--- a/crlf.txt\x1b[m\n\x1b[1m+++ b/crlf.txt\x1b[m\n\x1b[36m@@ -1,3 +1,3 @@\x1b[m\n a\x1b[m\r\n \x1b[m\r\n\x1b[31m-b\x1b[m\r\n\x1b[32m+\x1b[m\x1b[32mc\x1b[m\x1b[41m\r\x1b[m\n";
        let colored = parse_patch(colored_patch).expect("colored diff");
        let plain = parse_patch(plain_patch).expect("plain diff");

        let rendered = of_document(&colored);

        assert_eq!(rendered, of_document(&plain));
    }

    #[test]
    fn renders_metadata_only_git_fixture_in_original_order() {
        let input = include_bytes!("../tests/fixtures/git-rename-only.patch");
        let document = parse_patch(input).expect("valid metadata-only Git fixture");

        let rendered = of_document(&document);

        assert_eq!(rendered, input);
    }

    #[test]
    fn renders_colored_git_fixture_without_terminal_controls() {
        let input = include_bytes!("../tests/fixtures/git-multiline-coloured.patch");
        let document = parse_patch(input).expect("valid colored Git fixture");

        let rendered = of_document(&document);

        assert_eq!(
            rendered,
            include_bytes!("../tests/fixtures/git-multiline.patch")
        );
    }

    #[test]
    fn removes_control_sequences_before_a_record_marker() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n\x1b]8;;https://bad\x07+new\n";
        let document = parse_patch(input).expect("valid diff with a controlled record");

        let rendered = of_document(&document);

        assert_eq!(
            rendered,
            b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n"
        );
    }

    #[test]
    fn preserves_printable_text_and_neutralizes_terminal_controls() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+tab\t e\xcc\x81 \xe7\x95\x8c \x1b[31mred\x1b[0m \x1b]8;;https://bad\x07link\x1bPbad\x1b\\\x01\n";
        let document = parse_patch(input).expect("valid diff");

        let rendered = of_document(&document);
        let text = String::from_utf8(rendered).expect("safe UTF-8 output");

        assert!(
            text.contains("tab\t e\u{301} \u{754c} red link"),
            "{text:?}"
        );
        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert!(!text.contains('\u{1}'), "{text:?}");
        assert!(!text.contains("https://bad"), "{text:?}");
    }

    #[test]
    fn replaces_invalid_utf8_without_losing_following_text() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+\xffnew\n";
        let document = parse_patch(input).expect("valid diff");

        let rendered = of_document(&document);
        let text = String::from_utf8(rendered).expect("safe UTF-8 output");

        assert!(text.contains("+\u{fffd}new\n"), "{text:?}");
    }

    #[test]
    fn preserves_a_no_newline_marker_without_adding_a_line_ending() {
        let input =
            b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file";
        let document = parse_patch(input).expect("valid diff with a no-newline marker");

        let rendered = of_document(&document);

        assert_eq!(rendered, input);
    }
}
