//! Input text decoded once: each line's exact bytes and its sanitized display text, read
//! through one escape-sequence scanner. Rules:
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
    /// The line read from `bytes`, which keeps them exactly and decodes its sanitized
    /// text once.
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
        let DecodedChar { source, character } = visible(&self.bytes).next()?;

        // Visible text resumes from plain text after a visible character, so the rest's
        // sanitized text is the remainder of this line's.
        let rest = Self {
            bytes: self.bytes[source.end..].into(),
            sanitized: self.sanitized[character.len_utf8()..].into(),
        };
        Some((character, rest))
    }
}

/// The visible text of `bytes`, decoded as lossy UTF-8.
pub fn sanitize(bytes: &[u8]) -> String {
    visible(bytes).map(|decoded| decoded.character).collect()
}

/// A character decoded from input, with the source bytes it was decoded from.
struct DecodedChar {
    source: Range<usize>,
    character: char,
}

/// Each visible character of `bytes`, decoded as lossy UTF-8. Terminal control
/// sequences and control characters other than tab are not visible.
fn visible(bytes: &[u8]) -> impl Iterator<Item = DecodedChar> {
    let mut state = State::Text;

    decoded(bytes).filter(move |decoded| {
        let shown = state.shows(decoded.character);
        state = state.after(decoded.character);

        shown
    })
}

/// Lossy UTF-8 decoding of `bytes`; an invalid sequence decodes to one replacement
/// character.
fn decoded(bytes: &[u8]) -> impl Iterator<Item = DecodedChar> {
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
                DecodedChar {
                    source: begin..begin + character.len_utf8(),
                    character,
                }
            });
        let invalid = (!chunk.invalid().is_empty()).then_some(DecodedChar {
            source: invalid_start..start,
            character: char::REPLACEMENT_CHARACTER,
        });

        valid.chain(invalid)
    })
}

/// Where the scanner is within terminal control sequences.
#[derive(Clone, Copy)]
enum State {
    Text,
    Escape,
    EscapeIntermediate,
    ControlSequence,
    StringSequence,
}

/// CAN: abandons the sequence it interrupts.
const CANCEL: char = '\u{18}';
/// SUB: abandons the sequence it interrupts, like CAN.
const SUBSTITUTE: char = '\u{1a}';
/// BEL: ends a string sequence, as xterm reads it.
const BELL: char = '\u{7}';
/// ST in its C1 spelling: ends a string sequence.
const STRING_TERMINATOR: char = '\u{9c}';

impl State {
    /// Whether `character`, read in this state, is visible: only text outside a control
    /// sequence is, and among control characters only tab.
    fn shows(self, character: char) -> bool {
        match self {
            Self::Text => character == '\t' || !character.is_control(),
            Self::Escape
            | Self::EscapeIntermediate
            | Self::ControlSequence
            | Self::StringSequence => false,
        }
    }

    /// The state after `character`. Each introducer has a 7-bit spelling (`ESC` and a
    /// letter) and a single-character C1 spelling, and both open the same sequence. An
    /// escape may carry intermediate bytes before its final byte. An introducer read
    /// inside any unfinished sequence starts a new sequence, so `ESC \` ends a string as
    /// an escape whose final byte is `\`; CAN or SUB abandons any sequence.
    fn after(self, character: char) -> Self {
        match (self, character) {
            (Self::Text, _) => Self::opened_by(character).unwrap_or(Self::Text),
            (_, CANCEL | SUBSTITUTE) => Self::Text,
            (Self::Escape, '[') => Self::ControlSequence,
            (Self::Escape, ']' | 'P' | 'X' | '^' | '_') => Self::StringSequence,
            (Self::Escape | Self::EscapeIntermediate, '\u{20}'..='\u{2f}') => {
                Self::EscapeIntermediate
            }
            (Self::Escape | Self::EscapeIntermediate, _) => {
                Self::opened_by(character).unwrap_or(Self::Text)
            }
            (Self::ControlSequence, '@'..='~') => Self::Text,
            (Self::ControlSequence, _) => {
                Self::opened_by(character).unwrap_or(Self::ControlSequence)
            }
            (Self::StringSequence, BELL | STRING_TERMINATOR) => Self::Text,
            (Self::StringSequence, _) => Self::opened_by(character).unwrap_or(Self::StringSequence),
        }
    }

    /// The sequence `character` introduces, if it is an introducer.
    fn opened_by(character: char) -> Option<Self> {
        match character {
            '\u{1b}' => Some(Self::Escape),
            '\u{9b}' => Some(Self::ControlSequence),
            '\u{90}' | '\u{98}' | '\u{9d}' | '\u{9e}' | '\u{9f}' => Some(Self::StringSequence),
            _ => None,
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
                b"\x1b[31mred\x1b[0m\ttab \x1b]8;;u\x07link\x1bPdcs\x1b\\\x01\xc2\x85\xffend\r\n"
            ),
            "red\ttab link\u{fffd}end"
        );
    }

    /// A single-character C1 introducer opens the same sequence as its 7-bit `ESC`
    /// spelling, so the whole sequence is stripped either way.
    #[test]
    fn strips_c1_introduced_sequences_like_their_escape_spellings() {
        for (c1, escape) in [
            (b"\xc2\x9b31mred".as_slice(), b"\x1b[31mred".as_slice()),
            (
                b"\xc2\x9d8;;https://x\x07link",
                b"\x1b]8;;https://x\x07link",
            ),
            (b"\xc2\x90dcs\x1b\\after", b"\x1bPdcs\x1b\\after"),
            (b"\xc2\x98sos\xc2\x9cafter", b"\x1bXsos\x1b\\after"),
            (b"\xc2\x9epm\x07after", b"\x1b^pm\x07after"),
            (b"\xc2\x9fapc\x1b\\after", b"\x1b_apc\x1b\\after"),
        ] {
            assert_eq!(sanitize(c1), sanitize(escape), "{c1:?}");
        }
        assert_eq!(sanitize(b"\xc2\x9b31mred \xc2\x9d8;;u\x07link"), "red link");
    }

    /// An escape with intermediate bytes (`ESC ( B`, which `tput sgr0` writes) ends at
    /// its final byte, and an introducer inside an unfinished sequence starts a new
    /// sequence, so no part of either shows as text.
    #[test]
    fn strips_intermediate_escapes_and_sequences_an_introducer_restarts() {
        for (line, shown) in [
            (b"-\x1b(B\x1b[mold".as_slice(), "-old"),
            (b"\x1b\x1b[31mred", "red"),
            (b"\x1b\xc2\x9b5mblink", "blink"),
            (b"\x1b[31\x1b[0mred", "red"),
            (b"\x1b[31\xc2\x9b0mred", "red"),
            (b"\x1b$)Bkanji", "kanji"),
        ] {
            assert_eq!(sanitize(line), shown, "{line:?}");
        }
    }

    /// An introducer inside a string sequence ends the string and opens its own
    /// sequence, and CAN or SUB abandons any sequence, as a terminal reads them; the text
    /// after shows, so the line loses no visible source text.
    ///
    /// Rules: docs/spec/architecture.md#lossless-and-safe-rendering.
    #[test]
    fn ends_strings_at_a_new_introducer_and_cancels_sequences_at_can_or_sub() {
        for line in [
            b"\x1b]0;title\x1b[31mred".as_slice(),
            b"\x1b]0;title\xc2\x9b31mred",
            b"\x1bPq#0\x1b[0mred",
            b"\x1b[31\x18red",
            b"\x1b[31\x1ared",
            b"\x1b]0;t\x18red",
            b"\x1b(\x1ared",
        ] {
            assert_eq!(sanitize(line), "red", "{line:?}");
        }
    }
}
