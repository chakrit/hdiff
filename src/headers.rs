//! Diff header lines and the fields they name: `diff --git` sections, `---` / `+++`
//! headers, and Git's C-style quoting of paths. Contract:
//! docs/spec/architecture.md#core-model.

use std::str::CharIndices;

use crate::source_text::SourceLine;

/// A `diff --git` line and the extended header lines after it.
#[derive(Debug, PartialEq, Eq)]
pub struct GitHeader {
    pub line: SourceLine,
    pub metadata: Box<[SourceLine]>,
}

/// A `---` or `+++` line and the path it names.
#[derive(Debug, PartialEq, Eq)]
pub struct FileHeader {
    pub line: SourceLine,
    pub path: HeaderPath,
}

/// The path a `---` / `+++` header names, decoded, with its comparison-root prefix.
#[derive(Debug, PartialEq, Eq)]
pub enum HeaderPath {
    DevNull,
    Path(Box<str>),
    Undecodable,
}

impl HeaderPath {
    /// Reads a header's field after `--- ` or `+++ `, without its timestamp.
    pub fn of_field(field: &str) -> Self {
        // Git escapes tabs inside quotes, so the first tab ends a quoted path too.
        let without_timestamp = field.split_once('\t').map_or(field, |(path, _)| path);

        let Some(path) = quoted_or_plain(without_timestamp) else {
            return Self::Undecodable;
        };

        match &*path == "/dev/null" {
            true => Self::DevNull,
            false => Self::Path(path),
        }
    }
}

// Git's C-style quoting

/// A field that Git may have quoted, decoded.
pub fn quoted_or_plain(field: &str) -> Option<Box<str>> {
    match field.starts_with('"') {
        true => unquote(field).map(|(path, _)| path),
        false => Some(field.into()),
    }
}

/// Decodes Git's C-style quoting at the start of `field`, returning the decoded text
/// (bytes as lossy UTF-8) and what follows the closing quote.
pub fn unquote(field: &str) -> Option<(Box<str>, &str)> {
    let body = field.strip_prefix('"')?;
    let mut bytes = Vec::new();
    let mut characters = body.char_indices();

    while let Some((index, character)) = characters.next() {
        match character {
            '"' => {
                let decoded = String::from_utf8_lossy(&bytes).into();
                return Some((decoded, &body[index + 1..]));
            }
            '\\' => bytes.push(escaped_byte(&mut characters)?),
            _ => bytes.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }

    None
}

fn escaped_byte(characters: &mut CharIndices) -> Option<u8> {
    let (_, escape) = characters.next()?;

    match escape {
        'a' => Some(0x07),
        'b' => Some(0x08),
        't' => Some(b'\t'),
        'n' => Some(b'\n'),
        'v' => Some(0x0b),
        'f' => Some(0x0c),
        'r' => Some(b'\r'),
        '"' => Some(b'"'),
        '\\' => Some(b'\\'),
        '0'..='3' => {
            let high = octal_digit(escape)?;
            let middle = octal_digit(characters.next()?.1)?;
            let low = octal_digit(characters.next()?.1)?;
            Some(high << 6 | middle << 3 | low)
        }
        _ => None,
    }
}

fn octal_digit(character: char) -> Option<u8> {
    character
        .to_digit(8)
        .and_then(|digit| u8::try_from(digit).ok())
}

#[cfg(test)]
mod tests {
    use super::HeaderPath;

    #[test]
    fn header_paths_drop_timestamps_and_keep_their_prefix() {
        assert_eq!(
            HeaderPath::of_field("b/x.rs\t2026-09-27 23:45:10"),
            HeaderPath::Path("b/x.rs".into())
        );
        assert_eq!(HeaderPath::of_field("/dev/null"), HeaderPath::DevNull);
        assert_eq!(HeaderPath::of_field("\"b/open"), HeaderPath::Undecodable);
    }
}
