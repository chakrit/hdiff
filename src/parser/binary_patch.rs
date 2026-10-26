//! Git's binary patch: the `GIT binary patch` line, its `literal` and `delta` blocks, and
//! the base-85 payload lines each block holds. Rules:
//! docs/spec/architecture.md#input-contract.

use std::iter;

use super::{SourceLines, header_number, starts_section};
use crate::{
    document::{BinaryBlock, BinaryPatch, Failure},
    malformed::Malformed,
    non_empty::NonEmpty,
    source_text::SourceLine,
};

/// A binary patch that broke: every line of it read up to the failure, the failure, and
/// the failing line when the reader already took it.
pub(super) struct BrokenBinaryPatch {
    pub read: Vec<SourceLine>,
    pub failure: Failure,
    pub failing: Option<SourceLine>,
}

/// Whether `text` is the `GIT binary patch` line that opens a binary patch's blocks.
pub(super) fn is_binary_patch_header(text: &str) -> bool {
    text == "GIT binary patch"
}

/// The binary patch the `GIT binary patch` line `header` opens: its `literal` and `delta`
/// blocks, each with its payload and the blank line that ends it.
pub(super) fn parse_binary_patch(
    header: SourceLine,
    lines: &mut SourceLines<'_>,
) -> Result<BinaryPatch, BrokenBinaryPatch> {
    let mut read_blocks = Vec::new();
    while let Some(block_header) = lines.next_if(starts_binary_block) {
        match parse_binary_block(block_header, lines) {
            Ok(block) => read_blocks.push(block),
            Err(broken) => {
                let read = patch_lines(header, read_blocks)
                    .chain(broken.read)
                    .collect();
                return Err(BrokenBinaryPatch { read, ..broken });
            }
        }
    }

    let Some(blocks) = NonEmpty::of(read_blocks) else {
        return Err(BrokenBinaryPatch {
            read: vec![header],
            failure: lines.next_line_failure(Malformed::MissingBinaryPatchBlock),
            failing: None,
        });
    };
    Ok(BinaryPatch { header, blocks })
}

/// The lines of a patch's `header` and its complete `blocks`, in input order.
fn patch_lines(header: SourceLine, blocks: Vec<BinaryBlock>) -> impl Iterator<Item = SourceLine> {
    let block_lines = blocks.into_iter().flat_map(|block| {
        let BinaryBlock {
            header: block_header,
            payload,
            end,
        } = block;
        iter::once(block_header)
            .chain(payload)
            .chain(iter::once(end))
    });

    iter::once(header).chain(block_lines)
}

/// The block a `literal` or `delta` line `header` opens: its payload lines, then the
/// blank line that ends it. A line that starts a section cuts the block short.
fn parse_binary_block(
    header: SourceLine,
    lines: &mut SourceLines<'_>,
) -> Result<BinaryBlock, BrokenBinaryPatch> {
    if let Err(reason) = check_binary_block_size(header.sanitized()) {
        return Err(BrokenBinaryPatch {
            read: Vec::new(),
            failure: lines.last_line_failure(reason),
            failing: Some(header),
        });
    }

    let mut payload_lines = Vec::new();
    while let Some(line) = lines.next_if(|text| !text.is_empty() && !starts_section(text)) {
        if !is_payload_line(line.sanitized()) {
            return Err(BrokenBinaryPatch {
                read: block_lines(header, payload_lines),
                failure: lines.last_line_failure(Malformed::InvalidBinaryPatchPayload),
                failing: Some(line),
            });
        }
        payload_lines.push(line);
    }
    let Some(payload) = NonEmpty::of(payload_lines) else {
        return Err(BrokenBinaryPatch {
            read: vec![header],
            failure: lines.next_line_failure(Malformed::MissingBinaryPatchPayload),
            failing: None,
        });
    };
    let Some(end) = lines.next_if(str::is_empty) else {
        return Err(BrokenBinaryPatch {
            read: block_lines(header, payload),
            failure: lines.next_line_failure(Malformed::TruncatedBinaryPatch),
            failing: None,
        });
    };

    Ok(BinaryBlock {
        header,
        payload,
        end,
    })
}

/// A block's `header` line, then its `payload` lines.
fn block_lines(
    header: SourceLine,
    payload: impl IntoIterator<Item = SourceLine>,
) -> Vec<SourceLine> {
    iter::once(header).chain(payload).collect()
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
    let stated = binary_block_field(header).and_then(header_number).is_some();

    match stated {
        true => Ok(()),
        false => Err(Malformed::InvalidBinaryPatchHeader),
    }
}

/// Whether `text` is a binary block payload line in Git's form: a character stating how
/// many bytes the line holds (`A`–`Z` for 1–26, `a`–`z` for 27–52), then base-85 text in
/// groups of five characters, each group holding four bytes, with no more than three
/// bytes of filler.
fn is_payload_line(text: &str) -> bool {
    let Some((&count_byte, encoded)) = text.as_bytes().split_first() else {
        return false;
    };
    let Some(byte_count) = payload_byte_count(count_byte) else {
        return false;
    };

    let groups = encoded.len() / BASE85_GROUP_CHARACTERS;
    let whole_groups = encoded.len() % BASE85_GROUP_CHARACTERS == 0;
    let held = groups * BASE85_GROUP_BYTES;
    let fits =
        groups > 0 && whole_groups && byte_count <= held && byte_count > held - BASE85_GROUP_BYTES;

    fits && encoded.iter().all(|byte| BASE85.contains(byte))
}

/// The characters in one base-85 group.
const BASE85_GROUP_CHARACTERS: usize = 5;

/// The bytes one base-85 group holds.
const BASE85_GROUP_BYTES: usize = 4;

/// Git's base-85 alphabet.
const BASE85: &[u8] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";

/// The byte count a payload line's first byte states; none for any other byte.
fn payload_byte_count(stated: u8) -> Option<usize> {
    match stated {
        b'A'..=b'Z' => Some(usize::from(stated - b'A') + 1),
        b'a'..=b'z' => Some(usize::from(stated - b'a') + 27),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        document::FileBody,
        finite_output,
        malformed::Malformed,
        parser::{failures, parse_patch},
    };

    fn malformed(line: usize, reason: Malformed) -> Vec<(usize, Malformed)> {
        vec![(line, reason)]
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
            failures(parse_patch(binary.as_bytes())),
            malformed(4, Malformed::InvalidBinaryPatchHeader)
        );
    }

    /// docs/spec/architecture.md#input-contract: a count too large for hdiff to hold
    /// makes the header invalid.
    #[test]
    fn rejects_a_block_size_too_large_to_hold() {
        let binary = format!(
            "diff --git a/image b/image\nindex 1..2\nGIT binary patch\nliteral {}\npayload\n\n",
            "9".repeat(30)
        );

        assert_eq!(
            failures(parse_patch(binary.as_bytes())),
            malformed(4, Malformed::InvalidBinaryPatchHeader)
        );
    }

    #[test]
    fn keeps_binary_payload_and_surrounding_text_in_order() {
        let input = b"message\ndiff --git a/image b/image\nindex 1..2 100644\nGIT binary patch\nliteral 3\nKcmZQzWC8#H2LJ>B\n\nliteral 0\nHcmV?d00001\n\nfooter\n";
        let document = parse_patch(input).expect("binary diff with text");
        let views = document.file_views().collect::<Vec<_>>();
        let [view] = views.as_slice() else {
            panic!("the patch holds one file: {} views", views.len());
        };
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
    fn a_section_start_inside_a_binary_block_cuts_it_short_and_starts_the_next_section() {
        for (section, next_failure) in [
            ("diff --git a/next b/next", Malformed::MissingGitMetadata),
            ("--- a/next", Malformed::ExpectedNewFileHeader),
        ] {
            let input = binary_patch(section);

            assert_eq!(
                failures(parse_patch(input.as_bytes())),
                [(5, Malformed::MissingBinaryPatchPayload), (6, next_failure)],
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
                failures(parse_patch(input.as_bytes())),
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
            failures(parse_patch(input.as_bytes())),
            malformed(6, Malformed::TruncatedBinaryPatch)
        );
    }
}
