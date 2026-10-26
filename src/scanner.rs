//! hdiff's own highlighter: one byte scanner, with each language described by a
//! [`Spec`]. Rules: docs/spec/architecture.md#technology-direction.

pub mod go;

use std::ops::Range;

use crate::syntax::SyntaxClass;

/// One language's lexical pieces, which drive the scanner.
pub struct Spec {
    /// Markers that make the rest of the line one class, such as `//`.
    pub line_markers: &'static [Marker],
    /// Regions between an opening and a closing delimiter, such as strings.
    pub delimited: &'static [Pair],
    /// Word lists, each sorted, with the class of its words.
    pub words: &'static [(&'static [&'static str], SyntaxClass)],
    /// The class of the identifier that follows a given token.
    pub after: &'static [(&'static str, SyntaxClass)],
}

/// A marker that makes the rest of its line one class.
pub struct Marker {
    pub text: &'static str,
    pub class: SyntaxClass,
}

/// A region of one class between an opening and a closing delimiter.
pub struct Pair {
    pub open: &'static str,
    pub close: &'static str,
    pub body: Body,
    pub class: SyntaxClass,
}

/// How a delimited region's body reads.
pub enum Body {
    /// Raw text that can span lines; only the closing delimiter ends it.
    Raw,
    /// One line, where a backslash escapes the character after it.
    Escaped,
}

impl Pair {
    /// A region from `open` to `close` that can span lines.
    pub const fn spanning(open: &'static str, close: &'static str, class: SyntaxClass) -> Self {
        Self {
            open,
            close,
            body: Body::Raw,
            class,
        }
    }

    /// A one-line region quoted by `quote` on both ends, with backslash escapes.
    pub const fn escaped(quote: &'static str, class: SyntaxClass) -> Self {
        Self {
            open: quote,
            close: quote,
            body: Body::Escaped,
            class,
        }
    }
}

/// Where a line starts: on open ground, or inside the delimited region at an index of
/// the spec's [`Spec::delimited`] that an earlier line opened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanState(Option<usize>);

impl Spec {
    /// Scans `line` starting in `state`, giving `emit` each classified byte range in
    /// order, and returns the state the next line starts in.
    pub fn scan(
        &self,
        state: ScanState,
        line: &str,
        emit: impl FnMut(Range<usize>, SyntaxClass),
    ) -> ScanState {
        LineScan {
            spec: self,
            line,
            at: 0,
            previous: "",
            emit,
        }
        .run(state)
    }

    /// The class of the word `text`, preceded by the token `previous` and followed on its
    /// line by the byte `next`.
    fn word_class(&self, text: &str, previous: &str, next: Option<u8>) -> SyntaxClass {
        let listed = self
            .words
            .iter()
            .find(|(words, _)| words.binary_search(&text).is_ok());
        let after = self.after.iter().find(|(token, _)| *token == previous);

        match (listed, next, after) {
            (Some((_, class)), _, _) => *class,
            (None, Some(b'('), _) => SyntaxClass::Function,
            (None, _, Some((_, class))) => *class,
            (None, _, None) => SyntaxClass::Variable,
        }
    }
}

/// The bytes that make up operators. A lone `:` is a label or case separator, not an
/// operator.
const OPERATOR_BYTES: &[u8] = b"+-*/%&|^<>=!~:?";

/// What opens at a position on open ground: a line marker, or the delimited region at an
/// index of the spec's [`Spec::delimited`].
enum Opener<'s> {
    Marker(&'s Marker),
    Region(usize),
}

impl Opener<'_> {
    fn text(&self, spec: &Spec) -> &'static str {
        match self {
            Self::Marker(marker) => marker.text,
            Self::Region(index) => spec.delimited[*index].open,
        }
    }
}

/// One line's scan: where it has reached, the token before that, and where its pieces
/// go.
struct LineScan<'a, E> {
    spec: &'a Spec,
    line: &'a str,
    at: usize,
    /// The last token read on open ground, for [`Spec::after`].
    previous: &'a str,
    emit: E,
}

impl<'a, E: FnMut(Range<usize>, SyntaxClass)> LineScan<'a, E> {
    /// Scans the whole line from `state` and returns the state the next line starts in.
    fn run(mut self, state: ScanState) -> ScanState {
        if let ScanState(Some(region)) = state
            && let Some(unclosed) = self.region(region, 0)
        {
            return unclosed;
        }

        while self.at < self.line.len() {
            if let Some(unclosed) = self.token() {
                return unclosed;
            }
        }

        ScanState::default()
    }

    /// Reads one token on open ground; returns the next line's state when the token
    /// opens a region the line does not close.
    fn token(&mut self) -> Option<ScanState> {
        let start = self.at;
        let byte = self.line.as_bytes()[start];

        if byte.is_ascii_whitespace() {
            self.at += 1;
            return None;
        }

        match self.opener() {
            Some(Opener::Marker(marker)) => {
                self.at = self.line.len();
                (self.emit)(start..self.at, marker.class);
                None
            }
            Some(Opener::Region(region)) => {
                self.at += self.spec.delimited[region].open.len();
                self.previous = "";
                self.region(region, start)
            }
            None => {
                self.unopened(byte);
                None
            }
        }
    }

    /// Reads a word, number, operator, or other character at a position where nothing
    /// opens.
    fn unopened(&mut self, byte: u8) {
        let start = self.at;
        let class = if byte.is_ascii_alphabetic() || byte == b'_' {
            self.at = self.end_while(start, |byte| byte.is_ascii_alphanumeric() || byte == b'_');
            let next = self.line.as_bytes()[self.at..]
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace());
            Some(
                self.spec
                    .word_class(&self.line[start..self.at], self.previous, next),
            )
        } else if byte.is_ascii_digit() || (byte == b'.' && self.next_is_digit(start + 1)) {
            self.at = self.number_end(start);
            Some(SyntaxClass::Number)
        } else if OPERATOR_BYTES.contains(&byte) {
            self.at = self.end_while(start, |byte| OPERATOR_BYTES.contains(&byte));
            (&self.line[start..self.at] != ":").then_some(SyntaxClass::Operator)
        } else {
            self.at += self.line[start..].chars().next().map_or(1, char::len_utf8);
            None
        };

        self.previous = &self.line[start..self.at];
        if let Some(class) = class {
            (self.emit)(start..self.at, class);
        }
    }

    /// The longest line marker or region opener at the scan position.
    fn opener(&self) -> Option<Opener<'a>> {
        let rest = &self.line[self.at..];
        let markers = self.spec.line_markers.iter().map(Opener::Marker);
        let regions = (0..self.spec.delimited.len()).map(Opener::Region);

        markers
            .chain(regions)
            .filter(|opener| rest.starts_with(opener.text(self.spec)))
            .max_by_key(|opener| opener.text(self.spec).len())
    }

    /// Reads the body of `region`, which opened at `start`, through its closing
    /// delimiter or the line's end; returns the next line's state when the line ends
    /// first.
    fn region(&mut self, region: usize, start: usize) -> Option<ScanState> {
        let pair = &self.spec.delimited[region];
        let closed = match pair.body {
            Body::Raw => self.line[self.at..]
                .find(pair.close)
                .map(|offset| self.at + offset + pair.close.len()),
            Body::Escaped => self.escaped_close(pair.close),
        };

        self.at = closed.unwrap_or(self.line.len());
        if start < self.at {
            (self.emit)(start..self.at, pair.class);
        }
        match (closed, &pair.body) {
            (Some(_), _) => None,
            (None, Body::Raw) => Some(ScanState(Some(region))),
            (None, Body::Escaped) => Some(ScanState::default()),
        }
    }

    /// The end of `close` after the scan position, skipping each backslash-escaped
    /// character.
    fn escaped_close(&self, close: &str) -> Option<usize> {
        let bytes = self.line.as_bytes();
        let mut at = self.at;

        while at < bytes.len() {
            if bytes[at..].starts_with(close.as_bytes()) {
                return Some(at + close.len());
            }
            at += if bytes[at] == b'\\' { 2 } else { 1 };
        }

        None
    }

    /// The end of the number starting at `start`: digits, letters, `_`, a `.` before a
    /// digit, and the sign of an exponent (`e` in decimal, `p` in hexadecimal).
    fn number_end(&self, start: usize) -> usize {
        let bytes = self.line.as_bytes();
        let exponent =
            if self.line[start..].starts_with("0x") || self.line[start..].starts_with("0X") {
                b'p'
            } else {
                b'e'
            };
        let mut at = start + 1;

        while at < bytes.len() {
            let byte = bytes[at];
            let signed =
                matches!(byte, b'+' | b'-') && bytes[at - 1].to_ascii_lowercase() == exponent;
            let fraction = byte == b'.' && self.next_is_digit(at + 1);
            if !(byte.is_ascii_alphanumeric() || byte == b'_' || signed || fraction) {
                break;
            }
            at += 1;
        }

        at
    }

    /// Whether the byte at `at` is a digit.
    fn next_is_digit(&self, at: usize) -> bool {
        self.line.as_bytes().get(at).is_some_and(u8::is_ascii_digit)
    }

    /// The end of the run of bytes from `start` that `keeps` accepts.
    fn end_while(&self, start: usize, keeps: impl Fn(u8) -> bool) -> usize {
        let bytes = self.line.as_bytes();

        bytes[start..]
            .iter()
            .position(|byte| !keeps(*byte))
            .map_or(bytes.len(), |offset| start + offset)
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::{ScanState, Spec, go::GO};
    use crate::syntax::SyntaxClass;

    /// Each classified piece of `line` scanned from `state`, as text, and the next state.
    fn scanned<'a>(
        spec: &Spec,
        state: ScanState,
        line: &'a str,
    ) -> (Vec<(&'a str, SyntaxClass)>, ScanState) {
        let mut pieces = Vec::new();
        let next = spec.scan(state, line, |bytes: Range<usize>, class| {
            pieces.push((&line[bytes], class));
        });

        (pieces, next)
    }

    /// Each classified piece of one Go line scanned on open ground.
    fn go_line(line: &str) -> Vec<(&str, SyntaxClass)> {
        let (pieces, next) = scanned(&GO, ScanState::default(), line);
        assert_eq!(next, ScanState::default(), "{line:?} ends on open ground");
        pieces
    }

    #[test]
    fn classifies_go_keywords_functions_and_variables() {
        assert_eq!(
            go_line("func main() { return value }"),
            [
                ("func", SyntaxClass::Keyword),
                ("main", SyntaxClass::Function),
                ("return", SyntaxClass::Keyword),
                ("value", SyntaxClass::Variable),
            ]
        );
    }

    #[test]
    fn classifies_go_calls_properties_and_types() {
        assert_eq!(
            go_line("type Pod struct { spec.Name = fmt.Sprintf(x) }"),
            [
                ("type", SyntaxClass::Keyword),
                ("Pod", SyntaxClass::Type),
                ("struct", SyntaxClass::Keyword),
                ("spec", SyntaxClass::Variable),
                ("Name", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("fmt", SyntaxClass::Variable),
                ("Sprintf", SyntaxClass::Function),
                ("x", SyntaxClass::Variable),
            ]
        );
    }

    #[test]
    fn classifies_go_builtin_types_constants_numbers_and_operators() {
        assert_eq!(
            go_line("var n int = 0x1F + 2.5e-3 != nil"),
            [
                ("var", SyntaxClass::Keyword),
                ("n", SyntaxClass::Variable),
                ("int", SyntaxClass::Type),
                ("=", SyntaxClass::Operator),
                ("0x1F", SyntaxClass::Number),
                ("+", SyntaxClass::Operator),
                ("2.5e-3", SyntaxClass::Number),
                ("!=", SyntaxClass::Operator),
                ("nil", SyntaxClass::Constant),
            ]
        );
    }

    #[test]
    fn a_lone_colon_is_not_an_operator() {
        assert_eq!(
            go_line("case x: y := 1"),
            [
                ("case", SyntaxClass::Keyword),
                ("x", SyntaxClass::Variable),
                ("y", SyntaxClass::Variable),
                (":=", SyntaxClass::Operator),
                ("1", SyntaxClass::Number),
            ]
        );
    }

    #[test]
    fn strings_keep_escaped_quotes_and_comments_run_to_the_line_end() {
        assert_eq!(
            go_line(r#"s := "a \"b\" c" + 'x' // done "here""#),
            [
                ("s", SyntaxClass::Variable),
                (":=", SyntaxClass::Operator),
                (r#""a \"b\" c""#, SyntaxClass::String),
                ("+", SyntaxClass::Operator),
                ("'x'", SyntaxClass::String),
                (r#"// done "here""#, SyntaxClass::Comment),
            ]
        );
    }

    #[test]
    fn an_unterminated_escaped_string_ends_with_its_line() {
        assert_eq!(
            go_line(r#"x := "open"#),
            [
                ("x", SyntaxClass::Variable),
                (":=", SyntaxClass::Operator),
                (r#""open"#, SyntaxClass::String),
            ]
        );
    }

    #[test]
    fn a_block_comment_carries_across_lines() {
        let (first, inside) = scanned(&GO, ScanState::default(), "x /* one");
        let (second, still_inside) = scanned(&GO, inside, "two");
        let (third, after) = scanned(&GO, still_inside, "three */ y");

        assert_eq!(
            first,
            [
                ("x", SyntaxClass::Variable),
                ("/* one", SyntaxClass::Comment)
            ]
        );
        assert_ne!(inside, ScanState::default());
        assert_eq!(second, [("two", SyntaxClass::Comment)]);
        assert_eq!(
            third,
            [
                ("three */", SyntaxClass::Comment),
                ("y", SyntaxClass::Variable)
            ]
        );
        assert_eq!(after, ScanState::default());
    }

    #[test]
    fn a_raw_string_carries_across_lines() {
        let (first, inside) = scanned(&GO, ScanState::default(), "s := `one \\");
        let (second, after) = scanned(&GO, inside, "two` + t");

        assert_eq!(
            first,
            [
                ("s", SyntaxClass::Variable),
                (":=", SyntaxClass::Operator),
                ("`one \\", SyntaxClass::String),
            ]
        );
        assert_eq!(
            second,
            [
                ("two`", SyntaxClass::String),
                ("+", SyntaxClass::Operator),
                ("t", SyntaxClass::Variable),
            ]
        );
        assert_eq!(after, ScanState::default());
    }

    #[test]
    fn non_ascii_text_splits_identifiers_on_character_boundaries() {
        assert_eq!(
            go_line("naïve := \"ü\""),
            [
                ("na", SyntaxClass::Variable),
                ("ve", SyntaxClass::Variable),
                (":=", SyntaxClass::Operator),
                ("\"ü\"", SyntaxClass::String),
            ]
        );
    }

    #[test]
    fn every_go_word_list_is_sorted_for_lookup() {
        for (words, class) in GO.words {
            assert!(words.is_sorted(), "{class:?} words are sorted: {words:?}");
        }
    }
}
