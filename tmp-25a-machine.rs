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

