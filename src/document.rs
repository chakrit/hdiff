#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffDocument {
    pub files: Vec<DiffFile>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SourceLine {
    pub bytes: Vec<u8>,
    pub text: String,
    pub structural_text: String,
}

impl SourceLine {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let without_line_feed = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        let content = without_line_feed
            .strip_suffix(b"\r")
            .unwrap_or(without_line_feed);
        let text = String::from_utf8_lossy(content).into_owned();

        Self {
            bytes: bytes.to_vec(),
            structural_text: sanitize(&text),
            text,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DiffFile {
    Metadata { lines: Vec<SourceLine> },
    Unified(Box<UnifiedDiffFile>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnifiedDiffFile {
    pub metadata: Vec<SourceLine>,
    pub old_path: String,
    pub new_path: String,
    pub old_header: SourceLine,
    pub new_header: SourceLine,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Hunk {
    pub header: SourceLine,
    pub records: Vec<Record>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub kind: RecordKind,
    pub payload: SourceLine,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecordKind {
    Context,
    Addition,
    Deletion,
    Raw,
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
