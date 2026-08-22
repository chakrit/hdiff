#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffDocument {
    pub files: Vec<DiffFile>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SourceLine {
    pub bytes: Vec<u8>,
    pub text: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DiffFile {
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
