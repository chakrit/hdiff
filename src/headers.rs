//! Diff header lines and the fields they name: `diff --git` sections, `---` / `+++`
//! headers, and Git's C-style quoting of paths. Rules:
//! docs/spec/architecture.md#core-model.

use std::str::CharIndices;

use crate::{
    side::{BySide, Side},
    source_text::SourceLine,
};

/// A `diff --git` line and the extended header lines after it.
#[derive(Debug, PartialEq, Eq)]
pub struct GitHeader {
    pub line: SourceLine,
    pub metadata: Box<[SourceLine]>,
}

/// The names after a `diff --git ` line's prefix; any other line has none.
pub fn git_header_names(text: &str) -> Option<&str> {
    text.strip_prefix("diff --git ")
}

/// Whether `text` opens a combined diff's file section: `diff --cc` or `diff --combined`.
pub fn is_combined_diff_header(text: &str) -> bool {
    text.starts_with("diff --cc ") || text.starts_with("diff --combined ")
}

/// The field after a `---` (old) or `+++` (new) file header line's prefix; any other
/// line has none.
pub fn file_header_field(text: &str, side: Side) -> Option<&str> {
    let prefix = match side {
        Side::Old => "--- ",
        Side::New => "+++ ",
    };

    text.strip_prefix(prefix)
}

/// One of Git's extended header lines after `diff --git`, as far as naming a file needs.
pub enum GitMetadata<'a> {
    Added,
    Deleted,
    /// A rename's destination field, as written.
    RenameTo(&'a str),
    /// A copy's destination field, as written.
    CopyTo(&'a str),
    /// Mode, source, similarity, index, and binary-difference lines.
    Other,
}

impl<'a> GitMetadata<'a> {
    /// The extended header `text` is; a line that is not one has none.
    pub fn of(text: &'a str) -> Option<Self> {
        const OTHER_PREFIXES: [&str; 7] = [
            "old mode ",
            "new mode ",
            "copy from ",
            "rename from ",
            "similarity index ",
            "dissimilarity index ",
            "index ",
        ];

        if text.starts_with("new file mode ") {
            return Some(Self::Added);
        }
        if text.starts_with("deleted file mode ") {
            return Some(Self::Deleted);
        }
        if let Some(field) = text.strip_prefix("rename to ") {
            return Some(Self::RenameTo(field));
        }
        if let Some(field) = text.strip_prefix("copy to ") {
            return Some(Self::CopyTo(field));
        }

        let other = OTHER_PREFIXES.iter().any(|prefix| text.starts_with(prefix))
            || (text.starts_with("Binary files ") && text.ends_with(" differ"));
        other.then_some(Self::Other)
    }
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
    /// The path a `---` (old) or `+++` (new) file header line names; any other line names
    /// none.
    pub fn of_header(text: &str, side: Side) -> Option<Self> {
        file_header_field(text, side).map(Self::of_field)
    }

    /// Reads a header's field after `--- ` or `+++ `, without its timestamp. A field that
    /// names no path cannot be decoded into one.
    pub fn of_field(field: &str) -> Self {
        // Git escapes tabs inside quotes, so the first tab ends a quoted path too.
        let without_timestamp = field.split_once('\t').map_or(field, |(path, _)| path);

        let Some(path) = quoted_or_plain(without_timestamp) else {
            return Self::Undecodable;
        };

        match &*path {
            "" => Self::Undecodable,
            "/dev/null" => Self::DevNull,
            _ => Self::Path(path),
        }
    }
}

// Git's C-style quoting

/// A whole field that Git may have quoted, decoded. A quoted field must end at its
/// closing quote.
pub fn quoted_or_plain(field: &str) -> Option<Box<str>> {
    if !field.starts_with('"') {
        return Some(field.into());
    }

    match unquote(field)? {
        Unquoted { text, rest: "" } => Some(text),
        Unquoted { .. } => None,
    }
}

/// The two names after `diff --git `, as far as Git's quoting separates them.
pub enum GitNames<'a> {
    /// A quoted old name ends at its closing quote, so both names are known, decoded.
    Split(BySide<Box<str>>),
    /// Unquoted names may hold spaces, so where they divide is not yet known.
    Unsplit(&'a str),
}

impl<'a> GitNames<'a> {
    /// The names in a `diff --git` line's `names` field; quoting that does not decode, or
    /// a quoted old name not followed by a space and the new name, names none.
    pub fn of(names: &'a str) -> Option<Self> {
        if !names.starts_with('"') {
            return Some(Self::Unsplit(names));
        }

        let Unquoted { text: old, rest } = unquote(names)?;
        let new = quoted_or_plain(rest.strip_prefix(' ')?)?;
        Some(Self::Split(BySide { old, new }))
    }
}

/// A quoted field's decoded text and what follows its closing quote.
struct Unquoted<'a> {
    text: Box<str>,
    rest: &'a str,
}

/// Decodes Git's C-style quoting at the start of `field` (bytes as lossy UTF-8).
fn unquote(field: &str) -> Option<Unquoted<'_>> {
    let body = field.strip_prefix('"')?;
    let mut bytes = Vec::new();
    let mut characters = body.char_indices();

    while let Some((index, character)) = characters.next() {
        match character {
            '"' => {
                let text = String::from_utf8_lossy(&bytes).into();
                let rest = &body[index + 1..];
                return Some(Unquoted { text, rest });
            }
            '\\' => bytes.push(escaped_byte(&mut characters)?),
            _ => bytes.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }

    None
}

/// The byte a Git quoted-path escape stands for, read after its backslash; an escape Git
/// does not write is none.
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

/// The value of an octal digit.
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

    #[test]
    fn quoted_fields_reject_text_after_the_closing_quote() {
        assert_eq!(
            HeaderPath::of_field("\"b/x\\tz.rs\""),
            HeaderPath::Path("b/x\tz.rs".into())
        );
        assert_eq!(
            HeaderPath::of_field("\"b/x.rs\" junk"),
            HeaderPath::Undecodable
        );
        assert_eq!(super::quoted_or_plain("\"x\" junk"), None);
    }
}
