//! Input text decoded once: each line's exact bytes, its sanitized display text and its
//! line ending, read through one escape-sequence scanner. Rules:
//! docs/spec/architecture.md#lossless-and-safe-rendering.

use std::ops::Range;

/// An input line: its exact bytes, including any line ending, the sanitized text parsing
/// and display read, and the line ending.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceLine {
    bytes: Box<[u8]>,
    sanitized: Box<str>,
    line_ending: LineEnding,
}

impl SourceLine {
    /// The line read from `bytes`, which keeps them exactly and reads its sanitized text
    /// and line ending through the scanner once.
    pub fn decode(bytes: &[u8]) -> Self {
        let read = ReadLine::of(bytes);

        Self {
            bytes: bytes.into(),
            sanitized: read.sanitized.into_boxed_str(),
            line_ending: read.ending.line_ending(),
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

    /// The line ending the line ends with, as the scanner reads it: control sequences
    /// between a CR and the final LF do not break the CRLF.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// The first visible character and the rest of the line after it, when that character
    /// returns the scanner to plain text, as every record marker does. The rest is then
    /// read from plain text, so it is the line its own bytes decode to, ending as this
    /// line ends. A first character shown inside a sequence, such as a tab after `ESC`,
    /// does not split the line.
    pub fn split_plain_first(&self) -> Option<(char, SourceLine)> {
        let mut characters = visible(&self.bytes);
        let DecodedChar { source, character } = characters.next()?;
        if !characters.scan.state.is_plain_text() {
            return None;
        }

        let rest = Self {
            bytes: self.bytes[source.end..].into(),
            sanitized: self.sanitized[character.len_utf8()..].into(),
            line_ending: self.line_ending,
        };
        Some((character, rest))
    }
}

/// The line ending a source line ends with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    None,
    Lf,
    CrLf,
}

impl LineEnding {
    /// The ending as written.
    pub fn text(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

/// Some bytes read through the scanner: the one fold that collects their visible text,
/// and how their line ending stands.
struct ReadLine {
    sanitized: String,
    ending: EndingSoFar,
}

impl ReadLine {
    /// `bytes` read from plain text. Characters that show in plain text and leave the
    /// scanner there read as themselves, so the leading run of them is taken whole and
    /// only the bytes after it go through the scanner.
    fn of(bytes: &[u8]) -> Self {
        let valid = bytes.utf8_chunks().next().map_or("", |chunk| chunk.valid());
        let plain_end = valid
            .find(|character| !stays_plain(character))
            .unwrap_or(valid.len());

        let plain_run = Self {
            sanitized: valid[..plain_end].to_owned(),
            ending: EndingSoFar::Open,
        };
        scan(&bytes[plain_end..]).fold(plain_run, Self::after)
    }

    /// The line after reading one more character.
    fn after(mut self, read: Read) -> Self {
        match read.step {
            Step::Shown(_) => self.sanitized.push(read.decoded.character),
            Step::Hidden(_) => {}
        }
        self.ending = self.ending.after(&read);
        self
    }
}

/// How a line's ending stands after the characters read so far.
#[derive(Clone, Copy)]
enum EndingSoFar {
    /// No ending yet: the line has no characters, or more came after its last LF.
    Open,
    /// A CR read in plain text, followed so far only by hidden characters.
    CarriageReturn,
    /// An LF read last, after a pending CR or not.
    Closed(LineEnding),
}

impl EndingSoFar {
    /// The ending after `read`. Hidden characters between a plain CR and the LF, such as
    /// the SGR codes Git paints around a CR, leave the CR pending.
    fn after(self, read: &Read) -> Self {
        match (EndingInput::of(read), self) {
            (EndingInput::LineFeed, Self::CarriageReturn) => Self::Closed(LineEnding::CrLf),
            (EndingInput::LineFeed, Self::Open | Self::Closed(_)) => Self::Closed(LineEnding::Lf),
            (EndingInput::PlainCarriageReturn, Self::Open | Self::CarriageReturn)
            | (EndingInput::PlainCarriageReturn, Self::Closed(_)) => Self::CarriageReturn,
            (EndingInput::Shown, Self::Open | Self::CarriageReturn | Self::Closed(_))
            | (EndingInput::Hidden, Self::Closed(_)) => Self::Open,
            (EndingInput::Hidden, Self::Open | Self::CarriageReturn) => self,
        }
    }

    /// The line's ending once every character is read: only an LF read last ends it.
    fn line_ending(self) -> LineEnding {
        match self {
            Self::Closed(ending) => ending,
            Self::Open | Self::CarriageReturn => LineEnding::None,
        }
    }
}

/// What one read character means for a line's ending.
#[derive(Clone, Copy)]
enum EndingInput {
    /// An LF, which ends a line.
    LineFeed,
    /// A CR read in plain text, which may start a CRLF ending.
    PlainCarriageReturn,
    /// Any other character the scanner shows.
    Shown,
    /// Any other character the scanner hides, a CR inside a sequence included.
    Hidden,
}

impl EndingInput {
    /// What `read` means for the ending. The scanner hides every CR and LF, so their
    /// class alone decides them.
    fn of(read: &Read) -> Self {
        match read.class {
            Class::LineFeed => Self::LineFeed,
            Class::CarriageReturn => match read.read_in {
                State::Ground => Self::PlainCarriageReturn,
                State::Escape
                | State::EscapeIntermediate
                | State::ControlSequence
                | State::OscString
                | State::CommandString => Self::Hidden,
            },
            Class::Cancel
            | Class::Escape
            | Class::Bell
            | Class::Tab
            | Class::OtherC0
            | Class::Delete
            | Class::Intermediate
            | Class::Parameter
            | Class::Final(_)
            | Class::Introducer(_)
            | Class::StringTerminator
            | Class::OtherC1
            | Class::NonAscii => match read.step {
                Step::Shown(_) => Self::Shown,
                Step::Hidden(_) => Self::Hidden,
            },
        }
    }
}

/// Whether `character` shows when read in plain text and leaves the scanner there.
fn stays_plain(character: char) -> bool {
    match State::Ground.read(Class::of(character)) {
        Step::Shown(next) => next.is_plain_text(),
        Step::Hidden(_) => false,
    }
}

/// The visible text of `bytes`, decoded as lossy UTF-8.
pub fn sanitize(bytes: &[u8]) -> String {
    ReadLine::of(bytes).sanitized
}

/// A character decoded from input, with the source bytes it was decoded from.
struct DecodedChar {
    source: Range<usize>,
    character: char,
}

/// One character as the scanner read it: its class, the state it was read in, and what
/// reading it did.
struct Read {
    decoded: DecodedChar,
    class: Class,
    read_in: State,
    step: Step,
}

/// Each character of some input, decoded as lossy UTF-8, as the scanner reads it, with
/// the state reading has reached.
struct Scan<I> {
    decoded: I,
    state: State,
}

/// The characters of `bytes` as the scanner reads them from plain text.
fn scan(bytes: &[u8]) -> Scan<impl Iterator<Item = DecodedChar>> {
    Scan {
        decoded: decoded(bytes),
        state: State::Ground,
    }
}

impl<I: Iterator<Item = DecodedChar>> Iterator for Scan<I> {
    type Item = Read;

    fn next(&mut self) -> Option<Read> {
        let decoded = self.decoded.next()?;
        let class = Class::of(decoded.character);
        let read_in = self.state;
        let step = read_in.read(class);

        self.state = match step {
            Step::Shown(next) | Step::Hidden(next) => next,
        };
        Some(Read {
            decoded,
            class,
            read_in,
            step,
        })
    }
}

/// Each visible character of some input: terminal control sequences and control
/// characters other than tab are not visible.
struct Visible<I> {
    scan: Scan<I>,
}

/// The visible characters of `bytes`, read from plain text.
fn visible(bytes: &[u8]) -> Visible<impl Iterator<Item = DecodedChar>> {
    Visible { scan: scan(bytes) }
}

impl<I: Iterator<Item = DecodedChar>> Iterator for Visible<I> {
    type Item = DecodedChar;

    fn next(&mut self) -> Option<DecodedChar> {
        self.scan.by_ref().find_map(|read| match read.step {
            Step::Shown(_) => Some(read.decoded),
            Step::Hidden(_) => None,
        })
    }
}

/// Lossy UTF-8 decoding of `bytes`; an invalid sequence decodes to one replacement
/// character.
fn decoded(bytes: &[u8]) -> impl Iterator<Item = DecodedChar> {
    let chunks_with_starts = bytes.utf8_chunks().scan(0, |start, chunk| {
        let valid_start = *start;
        *start += chunk.valid().len() + chunk.invalid().len();
        Some((valid_start, chunk))
    });

    chunks_with_starts.flat_map(|(valid_start, chunk)| {
        let invalid_start = valid_start + chunk.valid().len();
        let invalid_end = invalid_start + chunk.invalid().len();

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
            source: invalid_start..invalid_end,
            character: char::REPLACEMENT_CHARACTER,
        });

        valid.chain(invalid)
    })
}

/// Where the scanner is within terminal control sequences. The DEC terminal parser
/// (docs/vendor/terminal-control-parser.md) has fourteen states; of its actions only
/// print and execute decide what shows, so its states that differ only in other actions
/// fold into one state here, as each variant names.
#[derive(Clone, Copy)]
enum State {
    /// Plain text: the reference's ground.
    Ground,
    /// After `ESC`, before its next byte: the reference's escape.
    Escape,
    /// After an escape's intermediate bytes, before its final byte: the reference's
    /// escape_intermediate.
    EscapeIntermediate,
    /// Inside a control sequence (CSI), before its final byte: the reference's
    /// csi_entry, csi_param, csi_intermediate and csi_ignore.
    ControlSequence,
    /// Inside an operating system command (OSC), before its terminator: the reference's
    /// osc_string.
    OscString,
    /// Inside a device control string, or an SOS, PM or APC string, before its
    /// terminator: the reference's dcs_entry, dcs_param, dcs_intermediate,
    /// dcs_passthrough, dcs_ignore and sos_pm_apc_string.
    CommandString,
}

/// What reading one character does: shows it or hides it, and the state reading moves
/// the scanner to.
#[derive(Clone, Copy)]
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
    /// The scanner state the sequence's introducer moves to.
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
    /// CR: a hidden control, and in plain text the start of a CRLF ending.
    CarriageReturn,
    /// LF: a hidden control, and the end of a line.
    LineFeed,
    /// Any other C0 control character.
    OtherC0,
    /// DEL.
    Delete,
    /// 0x20–0x2F: plain text in ground, such as a space or `-`, or an escape's or control
    /// sequence's intermediate byte.
    Intermediate,
    /// 0x30–0x3F: plain text in ground, a control sequence's parameter byte, or an
    /// escape's final byte.
    Parameter,
    /// 0x40–0x7E: plain text in ground, or a final byte, and after `ESC` the sequence it
    /// opens, if any.
    Final(Option<Sequence>),
    /// A C1 introducer: opens its sequence anywhere.
    Introducer(Sequence),
    /// ST: ends a string, and any other sequence.
    StringTerminator,
    /// Any other C1 control: ends any sequence.
    OtherC1,
    /// A character beyond ASCII, printable unless a sequence holds it.
    NonAscii,
}

impl Class {
    /// The class `character` reads as in every scanner state; the scanner decides it
    /// once per character.
    fn of(character: char) -> Self {
        match character {
            '\u{18}' | '\u{1a}' => Self::Cancel,
            '\u{1b}' => Self::Escape,
            '\u{7}' => Self::Bell,
            '\t' => Self::Tab,
            '\r' => Self::CarriageReturn,
            '\n' => Self::LineFeed,
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
            '\u{a0}'..='\u{10ffff}' => Self::NonAscii,
        }
    }
}

impl State {
    /// Whether the scanner reads plain text rather than holding an open escape, control
    /// sequence or string.
    fn is_plain_text(self) -> bool {
        match self {
            Self::Ground => true,
            Self::Escape
            | Self::EscapeIntermediate
            | Self::ControlSequence
            | Self::OscString
            | Self::CommandString => false,
        }
    }

    /// What reading a character of `class` in this state does, as the DEC parser reads
    /// it. `ESC`, C1 introducers, CAN, SUB, ST and every other C1 control act the same in
    /// every state. Outside a string a C0 control executes in place: a tab shows, other
    /// controls hide, and plain text or an open sequence continues. Strings hold
    /// every other character; only an OSC string also ends at BEL. A non-ASCII character,
    /// which no escape may hold, ends an escape and shows.
    fn read(self, class: Class) -> Step {
        use State::{
            CommandString, ControlSequence, Escape, EscapeIntermediate, Ground, OscString,
        };

        match (self, class) {
            (
                Ground,
                Class::Intermediate | Class::Parameter | Class::Final(_) | Class::NonAscii,
            ) => Step::Shown(Ground),
            (_, Class::Escape) => Step::Hidden(Escape),
            (_, Class::Introducer(sequence)) => Step::Hidden(sequence.state()),
            (_, Class::Cancel | Class::StringTerminator | Class::OtherC1) => Step::Hidden(Ground),
            (Ground | Escape | EscapeIntermediate | ControlSequence, Class::Tab) => {
                Step::Shown(self)
            }
            (
                Ground | Escape | EscapeIntermediate | ControlSequence,
                Class::Bell
                | Class::CarriageReturn
                | Class::LineFeed
                | Class::OtherC0
                | Class::Delete,
            ) => Step::Hidden(self),
            (Escape | EscapeIntermediate, Class::NonAscii) => Step::Shown(Ground),
            (Escape | EscapeIntermediate, Class::Intermediate) => Step::Hidden(EscapeIntermediate),
            (Escape, Class::Final(Some(sequence))) => Step::Hidden(sequence.state()),
            (Escape, Class::Parameter | Class::Final(None)) => Step::Hidden(Ground),
            (EscapeIntermediate, Class::Parameter | Class::Final(_)) => Step::Hidden(Ground),
            (ControlSequence, Class::Intermediate | Class::Parameter | Class::NonAscii) => {
                Step::Hidden(ControlSequence)
            }
            (ControlSequence, Class::Final(_)) => Step::Hidden(Ground),
            (OscString, Class::Bell) => Step::Hidden(Ground),
            (CommandString, Class::Bell) => Step::Hidden(CommandString),
            (
                OscString | CommandString,
                Class::Tab
                | Class::CarriageReturn
                | Class::LineFeed
                | Class::OtherC0
                | Class::Delete
                | Class::Intermediate
                | Class::Parameter
                | Class::Final(_)
                | Class::NonAscii,
            ) => Step::Hidden(self),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LineEnding, SourceLine, sanitize};

    /// The rest after a first character that returns the scanner to plain text is the
    /// line its own bytes decode to.
    #[test]
    fn splitting_leaves_the_rest_its_own_bytes_decode_to() {
        for line in [
            b"-old\n".as_slice(),
            b"\xc2\x85+new\r\n",
            b"\x1b[31m-\x1b[0mred\n",
            b"\x1b]x\x1b\x07a\x1b\\ context\n",
            b"\xff\xfe-after invalid",
            b"\x1b\xc3\xa9-after a broken escape",
        ] {
            let (_, rest) = SourceLine::decode(line)
                .split_plain_first()
                .expect("visible text");

            assert_eq!(rest, SourceLine::decode(rest.bytes()), "{line:?}");
        }
    }

    /// A first character shown inside a sequence leaves the scanner inside it, so the
    /// rest is not a line of its own and the line does not split.
    #[test]
    fn does_not_split_after_a_character_shown_inside_a_sequence() {
        for line in [b"\x1b\t-old".as_slice(), b"\x1b[3\t1mred"] {
            assert_eq!(
                SourceLine::decode(line).split_plain_first(),
                None,
                "{line:?}"
            );
        }
    }

    #[test]
    fn splits_after_the_first_visible_character() {
        let line = SourceLine::decode(b"\xc2\x85\x1b[1m-old\n");

        let (first, rest) = line.split_plain_first().expect("visible text");

        assert_eq!(first, '-');
        assert_eq!(rest.bytes(), b"old\n");
        assert_eq!(rest.sanitized(), "old");
    }

    /// docs/spec/architecture.md#lossless-and-safe-rendering: a CR read in plain text
    /// with only hidden characters between it and the final LF ends the line in CRLF;
    /// one shown after it, or one inside a sequence, does not.
    #[test]
    fn reads_the_line_ending_through_the_scanner() {
        for (line, ending) in [
            (b"a\r\n".as_slice(), LineEnding::CrLf),
            (b"+\x1b[32mb\x1b[m\x1b[41m\r\x1b[m\n", LineEnding::CrLf),
            (b" a\x1b[m\x1b[41m\r\x1b[m\n", LineEnding::CrLf),
            (b"a\rb\n", LineEnding::Lf),
            (b"a\x1b[3\r1m\n", LineEnding::Lf),
            (b"a\n", LineEnding::Lf),
            (b"a", LineEnding::None),
            (b"a\r", LineEnding::None),
            (b"a\n\x1b[m", LineEnding::None),
        ] {
            assert_eq!(SourceLine::decode(line).line_ending(), ending, "{line:?}");
        }
    }

    #[test]
    fn lines_without_visible_text_do_not_split() {
        for line in [b"".as_slice(), b"\n", b"\x1b]8;;https://x\x07\r\n"] {
            assert_eq!(
                SourceLine::decode(line).split_plain_first(),
                None,
                "{line:?}"
            );
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

    /// Plain text before the first invalid byte or control character reads as the
    /// scanner reads it, and what follows still decodes and ends the line.
    #[test]
    fn reads_plain_text_up_to_an_invalid_byte_or_control_as_the_scanner_does() {
        for (line, shown, ending) in [
            (b"\tif x {\n".as_slice(), "\tif x {", LineEnding::Lf),
            (
                b"caf\xc3\xa9 \xffend\r\n",
                "caf\u{e9} \u{fffd}end",
                LineEnding::CrLf,
            ),
            (b"a b\x1b[31mred\x07\n", "a bred", LineEnding::Lf),
            (b"x\ry", "xy", LineEnding::None),
        ] {
            let decoded = SourceLine::decode(line);

            assert_eq!(decoded.sanitized(), shown, "{line:?}");
            assert_eq!(decoded.line_ending(), ending, "{line:?}");
        }
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

    /// docs/spec/architecture.md#lossless-and-safe-rendering: an introducer inside a
    /// string sequence ends the string and opens its own sequence, and CAN or SUB
    /// abandons any sequence, as a terminal reads them; the text after shows, so the
    /// line loses no visible source text.
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

    /// docs/spec/architecture.md#lossless-and-safe-rendering: a tab a terminal executes
    /// inside an escape or control sequence shows and leaves the sequence open, and only
    /// an OSC string ends at BEL, so the line shows exactly what a terminal shows.
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

    /// docs/spec/architecture.md#lossless-and-safe-rendering: a control character a
    /// terminal executes inside a sequence leaves the sequence open, any other C1 control
    /// ends it, and a non-ASCII character after `ESC`, which no escape may hold, ends the
    /// escape and shows, so the line loses no visible source text.
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
