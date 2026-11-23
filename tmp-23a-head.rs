
#[cfg(test)]
mod tests {
    use crate::{
        document::{ContextMarker, DiffDocument, FileBody, Hunk, Record, RecordKind},
        finite_output,
        parser::{Error, Malformed, parse_patch},
    };

    fn malformed(line: usize, reason: Malformed) -> Result<DiffDocument, Error> {
        Err(Error::Malformed { line, reason })
    }

    /// The first hunk of a document holding one unified file.
    fn first_hunk(document: &DiffDocument) -> &Hunk {
        let files = document.files().collect::<Vec<_>>();
        let [file] = files[..] else {
            panic!("fixture should hold one file: {} files", files.len());
        };
        let FileBody::Unified(body) = &file.body else {
            panic!("fixture section should be unified");
        };

        body.hunks.first()
    }

    /// The `N` records of the first hunk of a document holding one unified file.
    fn first_hunk_records<const N: usize>(document: &DiffDocument) -> &[Record; N] {
        let records = &first_hunk(document).records;

        records
            .as_ref()
            .try_into()
            .unwrap_or_else(|_| panic!("the first hunk holds {} records, not {N}", records.len()))
    }

