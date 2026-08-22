#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffDocument {
    pub files: Vec<DiffFile>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DiffFile {
    pub old_path: String,
    pub new_path: String,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Hunk {
    pub header: String,
    pub records: Vec<Record>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub kind: RecordKind,
    pub payload: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecordKind {
    Context,
    Addition,
    Deletion,
    Raw,
}
