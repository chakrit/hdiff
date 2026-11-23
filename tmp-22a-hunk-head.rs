//! A unified hunk: its `@@` header and the counts it declares, then the records and
//! no-newline markers inside those counts. Rules:
//! docs/spec/architecture.md#input-contract.

use super::{Error, Malformed, SourceLines, reject_unsupported_header};
use crate::{
    document::{ContextMarker, Hunk, Record, RecordKind},
    headers::{file_header_field, git_header_names},
    side::{BySide, Side},
    source_text::SourceLine,
};

