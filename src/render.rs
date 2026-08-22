use crate::document::{DiffDocument, DiffFile, RecordKind, SourceLine};

pub fn render_unified(document: &DiffDocument) -> Vec<u8> {
    let mut output = Vec::new();

    for file in &document.files {
        output.extend_from_slice(&render_file(file));
    }

    output
}

pub fn render_file(file: &DiffFile) -> Vec<u8> {
    let mut output = Vec::new();

    append_line(&mut output, None, &file.old_header);
    append_line(&mut output, None, &file.new_header);
    for hunk in &file.hunks {
        append_line(&mut output, None, &hunk.header);
        for record in &hunk.records {
            append_line(&mut output, marker(&record.kind), &record.payload);
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

pub(crate) fn sanitize(text: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Text,
        Escape,
        ControlSequence,
        StringSequence,
        StringEscape,
    }

    let mut output = String::new();
    let mut state = State::Text;
    for character in text.chars() {
        state = match (state, character) {
            (State::Text, '\u{1b}') => State::Escape,
            (State::Text, '\t') => {
                output.push(character);
                State::Text
            }
            (State::Text, character) if !character.is_control() => {
                output.push(character);
                State::Text
            }
            (State::Text, _) => State::Text,
            (State::Escape, '[') => State::ControlSequence,
            (State::Escape, ']' | 'P' | 'X' | '^' | '_') => State::StringSequence,
            (State::Escape, _) => State::Text,
            (State::ControlSequence, '@'..='~') => State::Text,
            (State::ControlSequence, _) => State::ControlSequence,
            (State::StringSequence, '\u{7}') => State::Text,
            (State::StringSequence, '\u{1b}') => State::StringEscape,
            (State::StringSequence, _) => State::StringSequence,
            (State::StringEscape, '\\') => State::Text,
            (State::StringEscape, '\u{1b}') => State::StringEscape,
            (State::StringEscape, _) => State::StringSequence,
        };
    }

    output
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
