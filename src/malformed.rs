//! Why a recognized diff section fails to parse. Rules:
//! docs/spec/architecture.md#input-contract.

/// How a recognized diff section breaks the unified diff or Git patch format, grouped by
/// the part of the input it breaks: section starts, file headers, hunks, Git metadata,
/// then binary patches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Malformed {
    CombinedDiff,
    UnexpectedDiffHeader,

    MissingNewFileHeader,
    ExpectedNewFileHeader,
    FileWithoutHunks,

    InvalidHunkHeader,
    TruncatedHunk,
    ExcessRecords,
    UnexpectedGitHeaderInHunk,
    UnmarkedHunkLine,
    MisplacedNoNewlineMarker,
    RecordAfterNoNewlineMarker,

    MissingGitMetadata,

    InvalidBinaryPatchHeader,
    MissingBinaryPatchBlock,
    MissingBinaryPatchPayload,
    InvalidBinaryPatchPayload,
    TruncatedBinaryPatch,
}

impl Malformed {
    /// The reason as error messages state it.
    pub fn label(self) -> &'static str {
        match self {
            Self::CombinedDiff => "combined diffs are not supported",
            Self::UnexpectedDiffHeader => "unexpected diff header",

            Self::MissingNewFileHeader => "missing new file header",
            Self::ExpectedNewFileHeader => "expected new file header",
            Self::FileWithoutHunks => "file has no hunks",

            Self::InvalidHunkHeader => "invalid hunk header",
            Self::TruncatedHunk => "truncated hunk",
            Self::ExcessRecords => "hunk records exceed declared counts",
            Self::UnexpectedGitHeaderInHunk => "unexpected Git header in hunk",
            Self::UnmarkedHunkLine => "unmarked line in hunk",
            Self::MisplacedNoNewlineMarker => "no-newline marker does not follow a record",
            Self::RecordAfterNoNewlineMarker => {
                "record follows the no-newline marker that ended its side"
            }

            Self::MissingGitMetadata => "Git header has no extended header or file headers",

            Self::InvalidBinaryPatchHeader => "invalid binary patch header",
            Self::MissingBinaryPatchBlock => "missing binary patch block",
            Self::MissingBinaryPatchPayload => "missing binary patch payload",
            Self::InvalidBinaryPatchPayload => "invalid binary patch payload line",
            Self::TruncatedBinaryPatch => "truncated binary patch",
        }
    }
}
