//! hdiff's own highlighter: one byte scanner, with each language described by a
//! [`Spec`]. Rules: docs/spec/architecture.md#technology-direction.

pub mod asm;
pub mod c;
pub mod clojure;
pub mod cmake;
pub mod cpp;
pub mod crystal;
pub mod csharp;
pub mod css;
pub mod cuda;
pub mod dart;
pub mod dockerfile;
pub mod elixir;
pub mod erlang;
pub mod fsharp;
pub mod gleam;
pub mod go;
pub mod graphql;
pub mod groovy;
pub mod haskell;
pub mod hcl;
pub mod html;
pub mod ini;
pub mod java;
pub mod javascript;
pub mod json;
pub mod julia;
pub mod kotlin;
pub mod languages;
pub mod less;
pub mod lua;
pub mod makefile;
pub mod markdown;
pub mod nim;
pub mod objc;
pub mod ocaml;
pub mod perl;
pub mod php;
pub mod powershell;
pub mod proto;
pub mod python;
pub mod r;
pub mod ruby;
pub mod rust;
pub mod scala;
pub mod scss;
pub mod shell;
pub mod sql;
pub mod swift;
pub mod toml;
pub mod vb;
pub mod xml;
pub mod yaml;
pub mod zig;

use std::ops::Range;

use crate::syntax::SyntaxClass;

/// One language's lexical pieces, which drive the scanner.
pub struct Spec {
    /// Markers that make the rest of the line one class, such as `//`.
    pub line_markers: &'static [Marker],
    /// Regions between an opening and a closing delimiter, such as strings.
    pub delimited: &'static [Pair],
    /// The bytes words hold: ASCII letters, digits and `_`, and any the language adds.
    pub word_bytes: WordBytes,
    /// Word lists, each sorted, with the class of its words.
    pub words: &'static [(&'static [&'static str], SyntaxClass)],
    /// The class of a word that the given text follows on its line, after any spaces,
    /// such as a call's `(`.
    pub before: &'static [(&'static str, SyntaxClass)],
    /// The class of the word that follows a given token.
    pub after: &'static [(&'static str, SyntaxClass)],
    /// The class of an unlisted ALL_CAPS word of two or more bytes, if any.
    pub upper_case: Option<SyntaxClass>,
    /// The class of another unlisted word that starts with a capital letter, if any.
    pub capitalized: Option<SyntaxClass>,
    /// The class of every other word, if any.
    pub other_words: Option<SyntaxClass>,
    /// Bytes that may end a word, such as Ruby's `?` and `!`, when no `=` follows them.
    pub word_suffixes: &'static [u8],
    /// Whether the word lists match words in any case.
    pub word_case: WordCase,
}

/// Whether a spec's word lists match words as written or in any case.
pub enum WordCase {
    Sensitive,
    /// Lists hold lowercase words, matched whatever the case of the word read.
    Insensitive,
}

/// Which bytes can be part of a word, looked up by byte.
pub struct WordBytes([bool; 256]);

impl WordBytes {
    /// ASCII letters, digits and `_`, with the `extra` bytes a language adds, such as `$`.
    pub const fn with(extra: &[u8]) -> Self {
        let mut table = [false; 256];
        let mut byte = 0;
        while byte < 256 {
            let ascii = byte as u8;
            table[byte] = ascii.is_ascii_alphanumeric() || ascii == b'_';
            byte += 1;
        }
        let mut index = 0;
        while index < extra.len() {
            table[extra[index] as usize] = true;
            index += 1;
        }

        Self(table)
    }
}

/// A marker that makes the rest of its line one class.
pub struct Marker {
    pub text: &'static str,
    pub class: SyntaxClass,
    pub placement: Placement,
}

/// Where on a line a marker counts.
pub enum Placement {
    Anywhere,
    /// Only as the first text of the line, after any leading whitespace.
    LineStart,
}

impl Marker {
    /// A marker that counts anywhere on its line, such as `//`.
    pub const fn anywhere(text: &'static str, class: SyntaxClass) -> Self {
        Self {
            text,
            class,
            placement: Placement::Anywhere,
        }
    }

    /// A marker that counts only at the start of its line, such as a Markdown `#`.
    pub const fn at_line_start(text: &'static str, class: SyntaxClass) -> Self {
        Self {
            text,
            class,
            placement: Placement::LineStart,
        }
    }
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
    EscapedLine,
    /// Text that can span lines, where a backslash escapes the character after it.
    EscapedBlock,
    /// Raw text that can span lines, where each opener inside opens a nested region the
    /// next closer ends.
    Nested,
    /// One character or one escape sequence; an opener not closed right after it is no
    /// region, as with a Rust lifetime.
    Character,
}

impl Pair {
    /// A region from `open` to `close` that can span lines.
    pub const fn spanning(open: &'static str, close: &'static str, class: SyntaxClass) -> Self {
        Self::of(open, close, Body::Raw, class)
    }

    /// A one-line region quoted by `quote` on both ends, with backslash escapes.
    pub const fn escaped(quote: &'static str, class: SyntaxClass) -> Self {
        Self::of(quote, quote, Body::EscapedLine, class)
    }

    /// A region from `open` to `close` that can span lines, with backslash escapes.
    pub const fn escaped_spanning(
        open: &'static str,
        close: &'static str,
        class: SyntaxClass,
    ) -> Self {
        Self::of(open, close, Body::EscapedBlock, class)
    }

    /// A region from `open` to `close` that can span lines and nests.
    pub const fn nesting(open: &'static str, close: &'static str, class: SyntaxClass) -> Self {
        Self::of(open, close, Body::Nested, class)
    }

    /// One character or escape sequence quoted by `quote` on both ends.
    pub const fn character(quote: &'static str, class: SyntaxClass) -> Self {
        Self::of(quote, quote, Body::Character, class)
    }

    const fn of(open: &'static str, close: &'static str, body: Body, class: SyntaxClass) -> Self {
        Self {
            open,
            close,
            body,
            class,
        }
    }
}

/// Where a line starts: on open ground, or inside a delimited region an earlier line
/// opened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanState(Option<Inside>);

/// An open region: its index in the spec's [`Spec::delimited`], and how many of its
/// openers are still open, which only a nesting region counts past one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Inside {
    region: usize,
    depth: usize,
}

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
    /// line by `rest`; a word no rule classifies has none.
    fn word_class(&self, text: &str, previous: &str, rest: &str) -> Option<SyntaxClass> {
        self.words
            .iter()
            .find(|(words, _)| match self.word_case {
                WordCase::Sensitive => words.binary_search(&text).is_ok(),
                WordCase::Insensitive => words
                    .binary_search_by(|word| {
                        word.bytes()
                            .cmp(text.bytes().map(|byte| byte.to_ascii_lowercase()))
                    })
                    .is_ok(),
            })
            .map(|(_, class)| *class)
            .or_else(|| {
                class_of(self.before, |token| {
                    rest.trim_ascii_start().starts_with(token)
                })
            })
            .or_else(|| class_of(self.after, |token| token == previous))
            .or_else(|| self.capitals_class(text))
            .or(self.other_words)
    }

    /// The class `word`'s capitals give it: ALL_CAPS, or a leading capital.
    fn capitals_class(&self, word: &str) -> Option<SyntaxClass> {
        if !word.starts_with(|first: char| first.is_ascii_uppercase()) {
            return None;
        }

        if is_upper_case(word) {
            self.upper_case
        } else {
            self.capitalized
        }
    }

    /// Whether `byte` can be part of a word.
    fn in_word(&self, byte: u8) -> bool {
        self.word_bytes.0[usize::from(byte)]
    }
}

/// The class of the first of `rules` whose token `matches`.
fn class_of(
    rules: &[(&'static str, SyntaxClass)],
    matches: impl Fn(&str) -> bool,
) -> Option<SyntaxClass> {
    rules
        .iter()
        .find(|(token, _)| matches(token))
        .map(|(_, class)| *class)
}

/// Whether `word`, which starts with a capital, is ALL_CAPS: two or more bytes of
/// capitals, digits and `_`.
fn is_upper_case(word: &str) -> bool {
    word.len() >= 2
        && word
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
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
        if let ScanState(Some(Inside { region, depth })) = state
            && let Some(unclosed) = self.region(region, depth, 0)
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

        match self.opener_at(start) {
            Some(Opener::Marker(marker)) => {
                self.at = self.line.len();
                (self.emit)(start..self.at, marker.class);
                None
            }
            Some(Opener::Region(region)) => {
                self.at += self.spec.delimited[region].open.len();
                self.previous = "";
                self.region(region, 1, start)
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
        let class = if self.spec.in_word(byte) && !byte.is_ascii_digit() {
            self.at = self.end_while(start, |byte| self.spec.in_word(byte));
            self.at += usize::from(self.takes_suffix());
            let word = &self.line[start..self.at];
            self.spec
                .word_class(word, self.previous, &self.line[self.at..])
        } else if byte.is_ascii_digit() || self.starts_fraction(start) {
            self.at = self.number_end(start);
            Some(SyntaxClass::Number)
        } else if OPERATOR_BYTES.contains(&byte) {
            self.at = self.operator_end(start);
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

    /// The longest line marker or region opener at byte `at`.
    fn opener_at(&self, at: usize) -> Option<Opener<'a>> {
        let markers = self.spec.line_markers.iter().map(Opener::Marker);
        let regions = (0..self.spec.delimited.len()).map(Opener::Region);

        markers
            .chain(regions)
            .filter(|opener| self.opens(opener, at))
            .max_by_key(|opener| opener.text(self.spec).len())
    }

    /// Whether `opener` opens at byte `at`: a character region opens only when it closes
    /// right after one character or escape.
    fn opens(&self, opener: &Opener<'_>, at: usize) -> bool {
        let text = opener.text(self.spec);
        if !self.line.as_bytes()[at..].starts_with(text.as_bytes()) {
            return false;
        }

        match opener {
            Opener::Marker(marker) => match marker.placement {
                Placement::Anywhere => true,
                Placement::LineStart => self.line.as_bytes()[..at]
                    .iter()
                    .all(u8::is_ascii_whitespace),
            },
            Opener::Region(region) => {
                let pair = &self.spec.delimited[*region];
                match pair.body {
                    Body::Raw | Body::Nested | Body::EscapedLine | Body::EscapedBlock => true,
                    Body::Character => self.character_close(at + text.len(), pair.close).is_some(),
                }
            }
        }
    }

    /// Reads the body of `region`, which opened at `start` and has `depth` openers open,
    /// through its closing delimiter or the line's end; returns the next line's state
    /// when the line ends first.
    fn region(&mut self, region: usize, depth: usize, start: usize) -> Option<ScanState> {
        let pair = &self.spec.delimited[region];
        let closed = match pair.body {
            Body::Raw => self.line[self.at..]
                .find(pair.close)
                .map(|offset| self.at + offset + pair.close.len())
                .ok_or(depth),
            Body::Nested => self.nested_close(pair, depth),
            Body::EscapedLine | Body::EscapedBlock => {
                self.escaped_close(self.at, pair.close).ok_or(depth)
            }
            Body::Character => self.character_close(self.at, pair.close).ok_or(depth),
        };

        self.at = closed.unwrap_or(self.line.len());
        if start < self.at {
            (self.emit)(start..self.at, pair.class);
        }
        match (closed, &pair.body) {
            (Ok(_), _) => None,
            (Err(depth), Body::Raw | Body::Nested | Body::EscapedBlock) => {
                Some(ScanState(Some(Inside { region, depth })))
            }
            (Err(_), Body::EscapedLine | Body::Character) => Some(ScanState::default()),
        }
    }

    /// The end of the closer that closes the nesting `pair` with `depth` openers open,
    /// counting each opener and closer after the scan position; the openers still open
    /// when the line ends first.
    fn nested_close(&self, pair: &Pair, depth: usize) -> Result<usize, usize> {
        let bytes = self.line.as_bytes();
        let mut at = self.at;
        let mut open = depth;

        while at < bytes.len() {
            if bytes[at..].starts_with(pair.close.as_bytes()) {
                at += pair.close.len();
                open -= 1;
                if open == 0 {
                    return Ok(at);
                }
            } else if bytes[at..].starts_with(pair.open.as_bytes()) {
                at += pair.open.len();
                open += 1;
            } else {
                at += 1;
            }
        }

        Err(open)
    }

    /// The end of `close` from byte `from` on, skipping each backslash-escaped character.
    fn escaped_close(&self, from: usize, close: &str) -> Option<usize> {
        let bytes = self.line.as_bytes();
        let mut at = from;

        while at < bytes.len() {
            if bytes[at..].starts_with(close.as_bytes()) {
                return Some(at + close.len());
            }
            at += if bytes[at] == b'\\' { 2 } else { 1 };
        }

        None
    }

    /// The end of `close` right after the one character or backslash escape at byte
    /// `from`.
    fn character_close(&self, from: usize, close: &str) -> Option<usize> {
        let rest = &self.line[from..];
        let body = match rest.chars().next()? {
            '\\' => return self.escaped_close(from, close),
            single => single.len_utf8(),
        };

        rest[body..]
            .starts_with(close)
            .then_some(from + body + close.len())
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

    /// Whether a number like `.5` starts at `start`: a `.` before a digit, with no operand
    /// such as `s` or `)` right before it, where it would be a field access like `s.0`.
    fn starts_fraction(&self, start: usize) -> bool {
        let bytes = self.line.as_bytes();
        let after_operand = start.checked_sub(1).is_some_and(|before| {
            self.spec.in_word(bytes[before]) || b")]".contains(&bytes[before])
        });

        bytes[start] == b'.' && self.next_is_digit(start + 1) && !after_operand
    }

    /// Whether the word ending at the scan position takes the suffix byte after it: one of
    /// the spec's [`Spec::word_suffixes`], with no `=` after it.
    fn takes_suffix(&self) -> bool {
        let bytes = self.line.as_bytes();
        let suffix = bytes
            .get(self.at)
            .is_some_and(|byte| self.spec.word_suffixes.contains(byte));

        suffix && bytes.get(self.at + 1) != Some(&b'=')
    }

    /// The end of the run of operator bytes from `start`, which stops where a marker or
    /// region opens, such as the `/*` in `=/*`.
    fn operator_end(&self, start: usize) -> usize {
        let bytes = self.line.as_bytes();
        let mut at = start + 1;

        while at < bytes.len()
            && OPERATOR_BYTES.contains(&bytes[at])
            && self.opener_at(at).is_none()
        {
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

/// Each classified piece of `line` scanned from `state`, as text, and the next state.
#[cfg(test)]
pub(crate) fn scanned<'a>(
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

/// Each classified piece of one line of `spec` scanned on open ground.
#[cfg(test)]
pub(crate) fn ground<'a>(spec: &Spec, line: &'a str) -> Vec<(&'a str, SyntaxClass)> {
    let (pieces, next) = scanned(spec, ScanState::default(), line);
    assert_eq!(next, ScanState::default(), "{line:?} ends on open ground");
    pieces
}

#[cfg(test)]
mod tests {
    use super::{Marker, Pair, ScanState, Spec, WordBytes, WordCase, go::GO, ground, scanned};
    use crate::syntax::SyntaxClass;

    #[test]
    fn classifies_go_keywords_functions_and_variables() {
        assert_eq!(
            ground(&GO, "func main() { return value }"),
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
            ground(&GO, "type Pod struct { spec.Name = fmt.Sprintf(x) }"),
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
            ground(&GO, "var n int = 0x1F + 2.5e-3 != nil"),
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
    fn an_operator_run_stops_where_a_comment_or_string_opens() {
        assert_eq!(
            ground(&GO, "a=/*x*/b+\"s\""),
            [
                ("a", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                ("/*x*/", SyntaxClass::Comment),
                ("b", SyntaxClass::Variable),
                ("+", SyntaxClass::Operator),
                ("\"s\"", SyntaxClass::String),
            ]
        );
    }

    #[test]
    fn a_lone_colon_is_not_an_operator() {
        assert_eq!(
            ground(&GO, "case x: y := 1"),
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
            ground(&GO, r#"s := "a \"b\" c" + 'x' // done "here""#),
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
            ground(&GO, r#"x := "open"#),
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
            ground(&GO, "naïve := \"ü\""),
            [
                ("na", SyntaxClass::Variable),
                ("ve", SyntaxClass::Variable),
                (":=", SyntaxClass::Operator),
                ("\"ü\"", SyntaxClass::String),
            ]
        );
    }

    /// A spec exercising each scanner rule apart from any language's table.
    static RULES: Spec = Spec {
        line_markers: &[Marker::anywhere("#", SyntaxClass::Comment)],
        delimited: &[
            Pair::escaped_spanning("\"\"\"", "\"\"\"", SyntaxClass::String),
            Pair::escaped("\"", SyntaxClass::String),
            Pair::character("'", SyntaxClass::String),
        ],
        word_bytes: WordBytes::with(b"$"),
        words: &[(&["let"], SyntaxClass::Keyword)],
        before: &[("(", SyntaxClass::Function), ("!(", SyntaxClass::Function)],
        after: &[(".", SyntaxClass::Property)],
        upper_case: Some(SyntaxClass::Constant),
        capitalized: Some(SyntaxClass::Type),
        other_words: Some(SyntaxClass::Variable),
        word_suffixes: b"",
        word_case: WordCase::Sensitive,
    };

    #[test]
    fn an_escaped_block_carries_across_lines_past_an_escaped_closer() {
        let (first, inside) = scanned(&RULES, ScanState::default(), r#"x = """one \""""#);
        let (second, after) = scanned(&RULES, inside, r#"two""" y"#);

        assert_eq!(
            first,
            [
                ("x", SyntaxClass::Variable),
                ("=", SyntaxClass::Operator),
                (r#""""one \""""#, SyntaxClass::String),
            ]
        );
        assert_eq!(
            second,
            [
                (r#"two""""#, SyntaxClass::String),
                ("y", SyntaxClass::Variable)
            ]
        );
        assert_eq!(after, ScanState::default());
    }

    #[test]
    fn a_character_quote_not_closed_after_one_character_is_no_region() {
        assert_eq!(
            ground(&RULES, r"'a' '\n' 'b x"),
            [
                ("'a'", SyntaxClass::String),
                (r"'\n'", SyntaxClass::String),
                ("b", SyntaxClass::Variable),
                ("x", SyntaxClass::Variable),
            ]
        );
    }

    #[test]
    fn words_hold_the_spec_s_extra_bytes() {
        assert_eq!(
            ground(&RULES, "$el.val"),
            [
                ("$el", SyntaxClass::Variable),
                ("val", SyntaxClass::Property)
            ]
        );
    }

    #[test]
    fn the_text_after_a_word_can_classify_it() {
        assert_eq!(
            ground(&RULES, "show!(x) f (y) a != b"),
            [
                ("show", SyntaxClass::Function),
                ("!", SyntaxClass::Operator),
                ("x", SyntaxClass::Variable),
                ("f", SyntaxClass::Function),
                ("y", SyntaxClass::Variable),
                ("a", SyntaxClass::Variable),
                ("!=", SyntaxClass::Operator),
                ("b", SyntaxClass::Variable),
            ]
        );
    }

    #[test]
    fn capitals_classify_unlisted_words() {
        assert_eq!(
            ground(&RULES, "MAX_LEN Vec X let"),
            [
                ("MAX_LEN", SyntaxClass::Constant),
                ("Vec", SyntaxClass::Type),
                ("X", SyntaxClass::Type),
                ("let", SyntaxClass::Keyword),
            ]
        );
    }

    /// A spec exercising the rules few languages need: line-start markers, nesting
    /// regions, word suffixes, and case-insensitive words.
    static FORMS: Spec = Spec {
        line_markers: &[Marker::at_line_start("#", SyntaxClass::Comment)],
        delimited: &[Pair::nesting("{-", "-}", SyntaxClass::Comment)],
        word_bytes: WordBytes::with(b""),
        words: &[(&["select", "where"], SyntaxClass::Keyword)],
        before: &[],
        after: &[],
        upper_case: None,
        capitalized: None,
        other_words: Some(SyntaxClass::Variable),
        word_suffixes: b"?!",
        word_case: WordCase::Insensitive,
    };

    #[test]
    fn a_line_start_marker_counts_only_at_the_start_of_a_line() {
        assert_eq!(
            ground(&FORMS, "  # note"),
            [("# note", SyntaxClass::Comment)]
        );
        assert_eq!(
            ground(&FORMS, "x # y"),
            [("x", SyntaxClass::Variable), ("y", SyntaxClass::Variable)]
        );
    }

    #[test]
    fn a_nesting_region_closes_only_at_its_own_depth() {
        assert_eq!(
            ground(&FORMS, "{- a {- b -} c -} d"),
            [
                ("{- a {- b -} c -}", SyntaxClass::Comment),
                ("d", SyntaxClass::Variable)
            ]
        );

        let (first, deep) = scanned(&FORMS, ScanState::default(), "x {- a {-");
        let (second, shallow) = scanned(&FORMS, deep, "-} b");
        let (third, after) = scanned(&FORMS, shallow, "-} c");

        assert_eq!(
            first,
            [
                ("x", SyntaxClass::Variable),
                ("{- a {-", SyntaxClass::Comment)
            ]
        );
        assert_eq!(second, [("-} b", SyntaxClass::Comment)]);
        assert_eq!(
            third,
            [("-}", SyntaxClass::Comment), ("c", SyntaxClass::Variable)]
        );
        assert_eq!(after, ScanState::default());
    }

    #[test]
    fn a_word_takes_a_suffix_byte_unless_an_operator_follows() {
        assert_eq!(
            ground(&FORMS, "empty? save! a != b"),
            [
                ("empty?", SyntaxClass::Variable),
                ("save!", SyntaxClass::Variable),
                ("a", SyntaxClass::Variable),
                ("!=", SyntaxClass::Operator),
                ("b", SyntaxClass::Variable),
            ]
        );
    }

    #[test]
    fn case_insensitive_words_match_in_any_case() {
        assert_eq!(
            ground(&FORMS, "SELECT x Where"),
            [
                ("SELECT", SyntaxClass::Keyword),
                ("x", SyntaxClass::Variable),
                ("Where", SyntaxClass::Keyword),
            ]
        );
    }
}
