//! Git's binary patch: the `GIT binary patch` line, its `literal` and `delta` blocks, and
//! the base-85 payload lines each block holds. Rules:
//! docs/spec/architecture.md#input-contract.

use super::{Error, LineStart, Malformed, SourceLines, is_decimal, line_start};
use crate::{
    document::{BinaryBlock, BinaryPatch},
    non_empty::NonEmpty,
    source_text::SourceLine,
};

/// Whether `text` is the `GIT binary patch` line that opens a binary patch's blocks.
pub(super) fn is_binary_patch_header(text: &str) -> bool {
    text == "GIT binary patch"
}

/// The binary patch the `GIT binary patch` line `header` opens: its `literal` and `delta`
/// blocks, each with its payload and the blank line that ends it.
pub(super) fn parse_binary_patch(
    header: SourceLine,
    lines: &mut SourceLines<'_>,
) -> Result<BinaryPatch, Error> {
    let mut blocks = Vec::new();
    while let Some(block_header) = lines.next_if(starts_binary_block) {
        blocks.push(parse_binary_block(block_header, lines)?);
    }

    let Some(blocks) = NonEmpty::of(blocks) else {
        return Err(lines.next_line_error(Malformed::MissingBinaryPatchBlock));
    };
    Ok(BinaryPatch { header, blocks })
}

/// The block a `literal` or `delta` line `header` opens: its payload lines, then the
/// blank line that ends it.
fn parse_binary_block(
    header: SourceLine,
    lines: &mut SourceLines<'_>,
) -> Result<BinaryBlock, Error> {
    lines.at_last_line(check_binary_block_size(header.sanitized()))?;

    let mut payload = Vec::new();
    while let Some(line) = lines.next_if(|text| !text.is_empty()) {
        lines.at_last_line(check_payload_line(line.sanitized()))?;
        payload.push(line);
    }
    let Some(payload) = NonEmpty::of(payload) else {
        return Err(lines.next_line_error(Malformed::MissingBinaryPatchPayload));
    };
    let Some(end) = lines.next_if(str::is_empty) else {
        return Err(lines.next_line_error(Malformed::TruncatedBinaryPatch));
    };

    Ok(BinaryBlock {
        header,
        payload,
        end,
    })
}

/// Whether `text` starts a `literal` or `delta` block, well-formed or not.
fn starts_binary_block(text: &str) -> bool {
    binary_block_field(text).is_some()
}

/// The size field after a `literal ` or `delta ` binary block header's prefix; any other
/// line has none.
fn binary_block_field(text: &str) -> Option<&str> {
    let literal = text.strip_prefix("literal ");
    let delta = text.strip_prefix("delta ");

    literal.or(delta)
}

/// Checks that a `literal` or `delta` block header states its size as a count.
fn check_binary_block_size(header: &str) -> Result<(), Malformed> {
    let stated = binary_block_field(header).is_some_and(is_decimal);

    match stated {
        true => Ok(()),
        false => Err(Malformed::InvalidBinaryPatchHeader),
    }
}

/// Checks a binary block payload line: a line that starts a section means the block was
/// cut short, and any other line must be in Git's payload form.
fn check_payload_line(text: &str) -> Result<(), Malformed> {
    if line_start(text)? != LineStart::Text {
        return Err(Malformed::TruncatedBinaryPatch);
    }

    match is_payload_line(text) {
        true => Ok(()),
        false => Err(Malformed::InvalidBinaryPatchPayload),
    }
}

/// Whether `text` is a binary block payload line in Git's form: a character stating how
/// many bytes the line holds (`A`–`Z` for 1–26, `a`–`z` for 27–52), then base-85 text in
/// groups of five characters, each group holding four bytes, with no more than three
/// bytes of filler.
fn is_payload_line(text: &str) -> bool {
    let Some((stated, encoded)) = text.split_at_checked(1) else {
        return false;
    };
    let Some(stated) = payload_byte_count(stated) else {
        return false;
    };

    let groups = encoded.len() / BASE85_GROUP;
    let whole_groups = encoded.len() % BASE85_GROUP == 0;
    let held = groups * BASE85_GROUP_BYTES;
    let fits = groups > 0 && whole_groups && stated <= held && stated > held - BASE85_GROUP_BYTES;

    fits && encoded.bytes().all(|byte| BASE85.contains(&byte))
}

/// The characters in one base-85 group.
const BASE85_GROUP: usize = 5;

/// The bytes one base-85 group holds.
const BASE85_GROUP_BYTES: usize = 4;

/// Git's base-85 alphabet.
const BASE85: &[u8] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";

/// The byte count a payload line's first character states; none for any other character.
fn payload_byte_count(stated: &str) -> Option<usize> {
    let [byte] = stated.as_bytes() else {
        return None;
    };

    match byte {
        b'A'..=b'Z' => Some(usize::from(byte - b'A') + 1),
        b'a'..=b'z' => Some(usize::from(byte - b'a') + 27),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        document::{DiffDocument, FileBody},
        finite_output,
        parser::{Error, Malformed, parse_patch},
    };

    fn malformed(line: usize, reason: Malformed) -> Result<DiffDocument, Error> {
        Err(Error::Malformed { line, reason })
    }

    /// A binary patch whose one `literal` block holds `payload` and its blank line.
    fn binary_patch(payload: &str) -> String {
        format!(
            "diff --git a/image b/image\nindex 1..2 100644\nGIT binary patch\nliteral 3\n{payload}\n\n"
        )
    }

    /// docs/spec/architecture.md#input-contract: structural parsing is strict, so a
    /// block size is ASCII digits and nothing else.
    #[test]
    fn rejects_a_signed_block_size() {
        let binary =
            "diff --git a/image b/image\nindex 1..2\nGIT binary patch\nliteral +12\npayload\n\n";

        assert_eq!(
            parse_patch(binary.as_bytes()),
            malformed(4, Malformed::InvalidBinaryPatchHeader)
        );
    }

    #[test]
    fn keeps_binary_payload_and_surrounding_text_in_order() {
        let input = b"message\ndiff --git a/image b/image\nindex 1..2 100644\nGIT binary patch\nliteral 3\nKcmZQzWC8#H2LJ>B\n\nliteral 0\nHcmV?d00001\n\nfooter\n";
        let document = parse_patch(input).expect("binary diff with text");
        let view = document.file_views().next().expect("binary file");
        let FileBody::Metadata(body) = &view.file.body else {
            panic!("binary diff is metadata");
        };

        let metadata = body
            .git
            .extended_headers
            .iter()
            .map(|line| line.sanitized())
            .collect::<Vec<_>>();
        let payloads = body
            .binary_patch
            .as_deref()
            .expect("the diff holds a binary patch")
            .blocks
            .iter()
            .map(|block| block.payload.first().sanitized())
            .collect::<Vec<_>>();
        let [footer] = view.trailing else {
            panic!("one line follows the file: {:?}", view.trailing);
        };

        assert_eq!(metadata, ["index 1..2 100644"]);
        assert_eq!(payloads, ["KcmZQzWC8#H2LJ>B", "HcmV?d00001"]);
        assert_eq!(footer.sanitized(), "footer");
        assert_eq!(finite_output::of_document(&document), input);
    }

    #[test]
    fn reads_a_section_start_inside_a_binary_payload_as_truncation() {
        for section in ["diff --git a/next b/next", "--- a/next"] {
            let input = binary_patch(section);

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(5, Malformed::TruncatedBinaryPatch),
                "{section}"
            );
        }
    }

    /// docs/spec/architecture.md#input-contract: a payload line is Git's length character
    /// then base-85 text in groups of five that hold that many bytes.
    #[test]
    fn rejects_binary_payload_lines_outside_gits_form() {
        for payload in [
            "diff --stat",
            "!!!!",
            "K",
            "KcmZQzWC8#H2LJ>",
            "AcmZQzWC8#H2LJ>B",
            "Kcm\"QzWC8#H2LJ>B",
        ] {
            let input = binary_patch(payload);

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(5, Malformed::InvalidBinaryPatchPayload),
                "{payload}"
            );
        }
    }

    /// docs/spec/architecture.md#input-contract: Git ends every binary block with a blank
    /// line, so a block that runs to the end of input is truncated.
    #[test]
    fn rejects_a_binary_block_without_its_closing_blank_line() {
        let input = "diff --git a/image b/image\nindex 1..2 100644\nGIT binary patch\nliteral 3\nKcmZQzWC8#H2LJ>B\n";

        assert_eq!(
            parse_patch(input.as_bytes()),
            malformed(6, Malformed::TruncatedBinaryPatch)
        );
    }
}
