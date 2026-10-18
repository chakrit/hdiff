//! Input text decoded once: each line's exact bytes and its sanitized display text, read
//! through one escape-sequence scanner. Contract:
//! docs/spec/architecture.md#lossless-and-safe-rendering.

use std::ops::Range;

/// An input line: its exact bytes, including any line ending, and the sanitized text
/// parsing and display read.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceLine {
    bytes: Box<[u8]>,
    sanitized: Box<str>,
}

impl SourceLine {
    pub fn decode(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.into(),
            sanitized: sanitize(bytes).into_boxed_str(),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The visible text: lossy UTF-8 without terminal control sequences, control
    /// characters other than tab, or the line ending.
    pub fn sanitized(&self) -> &str {
        &self.sanitized
    }

    /// The first visible character and the rest of the line after it.
    pub fn split_first(&self) -> Option<(char, SourceLine)> {
        let (source, first) = visible(&self.bytes).next()?;

        // Visible text resumes from plain text after a visible character, so the rest's
        // sanitized text is the remainder of this line's.
        let rest = Self {
            bytes: self.bytes[source.end..].into(),
            sanitized: self.sanitized[first.len_utf8()..].into(),
        };
        Some((first, rest))
    }
}

/// The visible text of `bytes`, decoded as lossy UTF-8.
pub fn sanitize(bytes: &[u8]) -> String {
    visible(bytes).map(|(_, character)| character).collect()
}

/// Each visible character of `bytes`, decoded as lossy UTF-8, with the source bytes it
/// was decoded from. Terminal control sequences and control characters other than tab
/// are not visible.
fn visible(bytes: &[u8]) -> impl Iterator<Item = (Range<usize>, char)> {
    let mut state = State::Text;

    decoded(bytes).filter_map(move |(source, character)| {
        let (next, shown) = state.after(character);
        state = next;

        shown.then_some((source, character))
    })
}

/// Lossy UTF-8 decoding with the source bytes of each character; an invalid sequence
/// decodes to one replacement character.
fn decoded(bytes: &[u8]) -> impl Iterator<Item = (Range<usize>, char)> {
    let mut start = 0;

    bytes.utf8_chunks().flat_map(move |chunk| {
        let valid_start = start;
        let invalid_start = valid_start + chunk.valid().len();
        start = invalid_start + chunk.invalid().len();

        let valid = chunk
            .valid()
            .char_indices()
            .map(move |(offset, character)| {
                let begin = valid_start + offset;
                (begin..begin + character.len_utf8(), character)
            });
        let invalid = (!chunk.invalid().is_empty())
            .then_some((invalid_start..start, char::REPLACEMENT_CHARACTER));

        valid.chain(invalid)
    })
}

/// Where the scanner is within terminal control sequences.
#[derive(Clone, Copy)]
enum State {
    Text,
    Escape,
    ControlSequence,
    StringSequence,
    StringEscape,
}

impl State {
    /// The state after `character`, and whether `character` is visible.
    fn after(self, character: char) -> (Self, bool) {
        match (self, character) {
            (Self::Text, '\u{1b}') => (Self::Escape, false),
            (Self::Text, '\t') => (Self::Text, true),
            (Self::Text, character) => (Self::Text, !character.is_control()),
            (Self::Escape, '[') => (Self::ControlSequence, false),
            (Self::Escape, ']' | 'P' | 'X' | '^' | '_') => (Self::StringSequence, false),
            (Self::Escape, _) => (Self::Text, false),
            (Self::ControlSequence, '@'..='~') => (Self::Text, false),
            (Self::ControlSequence, _) => (Self::ControlSequence, false),
            (Self::StringSequence, '\u{7}') => (Self::Text, false),
            (Self::StringSequence, '\u{1b}') => (Self::StringEscape, false),
            (Self::StringSequence, _) => (Self::StringSequence, false),
            (Self::StringEscape, '\\') => (Self::Text, false),
            (Self::StringEscape, '\u{1b}') => (Self::StringEscape, false),
            (Self::StringEscape, _) => (Self::StringSequence, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceLine, sanitize};

    #[test]
    fn splitting_matches_decoding_the_rest_separately() {
        for line in [
            b"-old\n".as_slice(),
            b"\xc2\x85+new\r\n",
            b"\x1b[31m-\x1b[0mred\n",
            b"\x1b]x\x1b\x07a\x1b\\ context\n",
            b"\xff\xfe-after invalid",
        ] {
            let (_, rest) = SourceLine::decode(line)
                .split_first()
                .expect("visible text");
            let suffix = &line[line.len() - rest.bytes().len()..];

            assert_eq!(rest, SourceLine::decode(suffix), "{line:?}");
        }
    }

    #[test]
    fn splits_after_the_first_visible_character() {
        let line = SourceLine::decode(b"\xc2\x85\x1b[1m-old\n");

        let (first, rest) = line.split_first().expect("visible text");

        assert_eq!(first, '-');
        assert_eq!(rest.bytes(), b"old\n");
        assert_eq!(rest.sanitized(), "old");
    }

    #[test]
    fn lines_without_visible_text_do_not_split() {
        for line in [b"".as_slice(), b"\n", b"\x1b]8;;https://x\x07\r\n"] {
            assert_eq!(SourceLine::decode(line).split_first(), None, "{line:?}");
        }
    }

    #[test]
    fn keeps_printable_text_and_tabs_only() {
        assert_eq!(
            sanitize(
                b"\x1b[31mred\x1b[0m\ttab \x1b]8;;u\x07link\x1bPdcs\x1b\\\x01\xc2\x9b\xffend\r\n"
            ),
            "red\ttab link\u{fffd}end"
        );
    }
}
