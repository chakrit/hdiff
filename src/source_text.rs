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

    /// The first visible character and the rest of the line after it. The rest's
    /// sanitized text is this line's after that character, so the rest is read in the
    /// scanner state the character left, which may be inside a sequence.
    pub fn split_first(&self) -> Option<(char, SourceLine)> {
        let DecodedChar { source, character } = visible(&self.bytes).next()?;

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

/// Each visible character of some input, decoded as lossy UTF-8, with the scanner state
/// reading has reached. Terminal control sequences and control characters other than tab
/// are not visible.
struct Visible<I> {
    decoded: I,
    state: State,
}

/// The visible characters of `bytes`, read from plain text.
fn visible(bytes: &[u8]) -> Visible<impl Iterator<Item = DecodedChar>> {
    Visible {
        decoded: decoded(bytes),
        state: State::Ground,
    }
}

impl<I: Iterator<Item = DecodedChar>> Iterator for Visible<I> {
    type Item = DecodedChar;

    fn next(&mut self) -> Option<DecodedChar> {
        self.decoded.by_ref().find_map(|decoded| {
            let step = self.state.read(Class::of(decoded.character));

            match step {
                Step::Shown(next) => {
                    self.state = next;
                    Some(decoded)
                }
                Step::Hidden(next) => {
                    self.state = next;
                    None
                }
            }
        })
    }
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

/// Where the scanner is within terminal control sequences: the states of the DEC
/// terminal parser that differ in what they show, as
/// docs/vendor/terminal-control-parser.md maps them.
#[derive(Clone, Copy)]
enum State {
    /// Plain text.
    Ground,
    /// After `ESC`, before its next byte.
    Escape,
    /// After an escape's intermediate bytes, before its final byte.
    EscapeIntermediate,
    /// Inside a control sequence (CSI), before its final byte.
    ControlSequence,
    /// Inside an operating system command (OSC), before its terminator.
    OscString,
    /// Inside a device control string, or an SOS, PM or APC string, before its
    /// terminator.
    CommandString,
}

/// What reading one character does: shows it or hides it, and the state reading moves
/// the scanner to.
enum Step {
    Shown(State),
    Hidden(State),
}

/// The sequence an introducer opens.
#[derive(Clone, Copy)]
enum Sequence {
    Control,
    Osc,
    CommandString,
}

impl Sequence {
    fn state(self) -> State {
        match self {
            Self::Control => State::ControlSequence,
            Self::Osc => State::OscString,
            Self::CommandString => State::CommandString,
        }
    }
}

/// What one character is to the scanner, decided once per character.
#[derive(Clone, Copy)]
enum Class {
    /// CAN or SUB: abandons any sequence.
    Cancel,
    /// `ESC`: opens an escape anywhere.
    Escape,
    /// BEL: ends an OSC string, as xterm reads it.
    Bell,
    /// Tab: the one visible control character.
    Tab,
    /// Any other C0 control character.
    OtherC0,
    /// DEL.
    Delete,
    /// 0x20–0x2F: an escape's or control sequence's intermediate byte.
    Intermediate,
    /// 0x30–0x3F: a control sequence's parameter byte, or an escape's final byte.
    Parameter,
    /// 0x40–0x7E: a final byte, and after `ESC` the sequence it opens, if any.
    Final(Option<Sequence>),
    /// A C1 introducer: opens its sequence anywhere.
    Introducer(Sequence),
    /// ST: ends a string, and any other sequence.
    StringTerminator,
    /// Any other C1 control: ends any sequence.
    OtherC1,
    /// Printable text beyond ASCII.
    Printable,
}

impl Class {
    fn of(character: char) -> Self {
        match character {
            '\u{18}' | '\u{1a}' => Self::Cancel,
            '\u{1b}' => Self::Escape,
            '\u{7}' => Self::Bell,
            '\t' => Self::Tab,
            '\u{0}'..='\u{1f}' => Self::OtherC0,
            '\u{7f}' => Self::Delete,
            '\u{20}'..='\u{2f}' => Self::Intermediate,
            '\u{30}'..='\u{3f}' => Self::Parameter,
            '[' => Self::Final(Some(Sequence::Control)),
            ']' => Self::Final(Some(Sequence::Osc)),
            'P' | 'X' | '^' | '_' => Self::Final(Some(Sequence::CommandString)),
            '\u{40}'..='\u{7e}' => Self::Final(None),
            '\u{9b}' => Self::Introducer(Sequence::Control),
            '\u{9d}' => Self::Introducer(Sequence::Osc),
            '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => Self::Introducer(Sequence::CommandString),
            '\u{9c}' => Self::StringTerminator,
            '\u{80}'..='\u{9f}' => Self::OtherC1,
            '\u{a0}'..='\u{10ffff}' => Self::Printable,
        }
    }
}

impl State {
    /// What reading a character of `class` in this state does, as the DEC parser reads
    /// it. `ESC`, C1 introducers, CAN, SUB, ST and every other C1 control act the same in
    /// every state. Inside an escape or control sequence a C0 control executes without
    /// ending the sequence, so a tab shows there and the sequence continues. Strings hold
    /// every other character; only an OSC string also ends at BEL. A non-ASCII character,
    /// which no escape may hold, ends an escape and shows.
    fn read(self, class: Class) -> Step {
        use State::{
            CommandString, ControlSequence, Escape, EscapeIntermediate, Ground, OscString,
        };

        match (self, class) {
            (Ground, Class::Tab | Class::Intermediate | Class::Parameter) => Step::Shown(Ground),
            (Ground, Class::Final(_) | Class::Printable) => Step::Shown(Ground),
            (
                Ground,
                Class::Cancel
                | Class::Bell
                | Class::OtherC0
                | Class::Delete
                | Class::StringTerminator
                | Class::OtherC1,
            ) => Step::Hidden(Ground),
            (_, Class::Escape) => Step::Hidden(Escape),
            (_, Class::Introducer(sequence)) => Step::Hidden(sequence.state()),
            (
                Escape | EscapeIntermediate | ControlSequence | OscString | CommandString,
                Class::Cancel | Class::StringTerminator | Class::OtherC1,
            ) => Step::Hidden(Ground),
            (
                Escape | EscapeIntermediate | ControlSequence,
                Class::Bell | Class::OtherC0 | Class::Delete,
            ) => Step::Hidden(self),
            (Escape | EscapeIntermediate | ControlSequence, Class::Tab) => Step::Shown(self),
            (Escape | EscapeIntermediate, Class::Printable) => Step::Shown(Ground),
            (Escape | EscapeIntermediate, Class::Intermediate) => Step::Hidden(EscapeIntermediate),
            (Escape, Class::Final(Some(sequence))) => Step::Hidden(sequence.state()),
            (Escape, Class::Parameter | Class::Final(None)) => Step::Hidden(Ground),
            (EscapeIntermediate, Class::Parameter | Class::Final(_)) => Step::Hidden(Ground),
            (ControlSequence, Class::Intermediate | Class::Parameter | Class::Printable) => {
                Step::Hidden(ControlSequence)
            }
            (ControlSequence, Class::Final(_)) => Step::Hidden(Ground),
            (OscString, Class::Bell) => Step::Hidden(Ground),
            (CommandString, Class::Bell) => Step::Hidden(CommandString),
            (
                OscString | CommandString,
                Class::Tab
                | Class::OtherC0
                | Class::Delete
                | Class::Intermediate
                | Class::Parameter
                | Class::Final(_)
                | Class::Printable,
            ) => Step::Hidden(self),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceLine, Visible, decoded, sanitize, visible};

    /// The rest after the first visible character is the rest of the line read in the
    /// scanner state that character left, so a tab shown inside a sequence leaves the
    /// rest inside it.
    #[test]
    fn splitting_matches_decoding_the_rest_in_the_state_the_first_character_left() {
        for line in [
            b"-old\n".as_slice(),
            b"\xc2\x85+new\r\n",
            b"\x1b[31m-\x1b[0mred\n",
            b"\x1b]x\x1b\x07a\x1b\\ context\n",
            b"\xff\xfe-after invalid",
            b"\x1b\xc3\xa9-after a broken escape",
            b"\x1b\t-old",
            b"\x1b[3\t1mred",
        ] {
            let mut first = visible(line);
            first.next().expect("visible text");
            let (_, rest) = SourceLine::decode(line)
                .split_first()
                .expect("visible text");
            let suffix = &line[line.len() - rest.bytes().len()..];

            let read_on = Visible {
                decoded: decoded(suffix),
                state: first.state,
            };
            let rest_text: String = read_on.map(|decoded| decoded.character).collect();

            assert_eq!(rest.sanitized(), rest_text, "{line:?}");
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
            (b"\xc2\x9epm\xc2\x9cafter", b"\x1b^pm\x1b\\after"),
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

    /// A tab a terminal executes inside an escape or control sequence shows and leaves
    /// the sequence open, and only an OSC string ends at BEL, so the line shows exactly
    /// what a terminal shows.
    ///
    /// Rules: docs/spec/architecture.md#lossless-and-safe-rendering.
    #[test]
    fn shows_a_tab_inside_a_sequence_and_ends_only_osc_strings_at_bel() {
        for (line, shown) in [
            (b"a\x1b\t[31mred".as_slice(), "a\tred"),
            (b"\x1b(\tBred", "\tred"),
            (b"\x1b\t-old", "\tld"),
            (b"a\x1b[3\t1mred", "a\tred"),
            (b"a\x1bPq\x07sixel\x1b\\b", "ab"),
            (b"\x1bPq\x07red\x1b\\Z", "Z"),
            (b"\x1b_apc\x07red\x1b\\Z", "Z"),
            (b"\x1b]0;title\x07red", "red"),
        ] {
            assert_eq!(sanitize(line), shown, "{line:?}");
        }
    }

    /// A control character a terminal executes inside a sequence leaves the sequence
    /// open, any other C1 control ends it, and a non-ASCII character after `ESC`, which
    /// no escape may hold, ends the escape and shows, so the line loses no visible
    /// source text.
    ///
    /// Rules: docs/spec/architecture.md#lossless-and-safe-rendering.
    #[test]
    fn executes_controls_inside_sequences_and_shows_what_breaks_an_escape() {
        for (line, shown) in [
            (b"\x1b\xc3\xa9-old".as_slice(), "\u{e9}-old"),
            (b"\x1b\x07[31mred", "red"),
            (b"\x1b(\x7fBred", "red"),
            (b"\x1b[31\xc2\x9cred", "red"),
            (b"\x1b[31\xc2\x85red", "red"),
            (b"\x1b]0;t\xc2\x85red", "red"),
        ] {
            assert_eq!(sanitize(line), shown, "{line:?}");
        }
    }
}
