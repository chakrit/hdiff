//! Spelling program arguments as POSIX shell words, so a printed command passes each
//! argument unchanged.

use std::borrow::Cow;

/// `words` as one shell command line that passes every word through unchanged.
pub fn command_line(words: &[&str]) -> String {
    words
        .iter()
        .map(|word| quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `word` as a single shell word: unchanged when no character is special to the shell,
/// otherwise single-quoted.
pub fn quote(word: &str) -> Cow<'_, str> {
    let inert = !word.is_empty() && word.chars().all(is_inert);

    match inert {
        true => Cow::Borrowed(word),
        false => Cow::Owned(format!("'{}'", word.replace('\'', r"'\''"))),
    }
}

/// Characters that mean nothing to a POSIX shell outside a command's first word.
fn is_inert(character: char) -> bool {
    character.is_ascii_alphanumeric() || "%+,-./:=@_".contains(character)
}

#[cfg(test)]
mod tests {
    use super::{command_line, quote};

    #[test]
    fn quotes_only_words_the_shell_would_change() {
        assert_eq!(quote("/usr/bin/hdiff"), "/usr/bin/hdiff");
        assert_eq!(quote("/a b/hdiff"), "'/a b/hdiff'");
        assert_eq!(quote(""), "''");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote("$HOME"), "'$HOME'");
    }

    #[test]
    fn joins_quoted_words_with_spaces() {
        assert_eq!(command_line(&["cp", "a b", "c"]), "cp 'a b' c");
    }
}
