//! The syntax spans of each record, from hdiff's own scanner. Rules:
//! docs/spec/architecture.md#technology-direction.

use crate::{
    document::{DiffFile, HunkRecords, Record, RecordEntry, RecordMap},
    headers::{FileHeader, HeaderPath},
    measurement::{Observer, Stage, Work},
    scanner::{ScanState, Spec, c::C, go::GO, javascript::JAVASCRIPT, python::PYTHON, rust::RUST},
    side::{BySide, Side},
    span::PayloadSpan,
};

/// The kind of syntax a span marks, which picks its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxClass {
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Type,
    Constant,
    Operator,
    Property,
    Variable,
}

/// One syntax class over part of a record's payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxSpan {
    /// The payload bytes the class covers.
    pub bytes: PayloadSpan,
    /// The kind of syntax the span marks.
    pub class: SyntaxClass,
}

/// The syntax spans of each record of `file`.
pub fn highlight_file(file: &DiffFile, observer: &mut impl Observer) -> RecordMap<Vec<SyntaxSpan>> {
    file.record_map(|headers, hunk| highlight_hunk(headers, hunk, observer))
}

/// The syntax spans of `hunk`, a hunk of the file with `headers`, each with the record it
/// styles. Each side reads in its path's language; context takes the new side's spans,
/// and stays textual when the paths select different languages.
fn highlight_hunk<'h>(
    headers: &BySide<FileHeader>,
    hunk: HunkRecords<'_, 'h>,
    observer: &mut impl Observer,
) -> Vec<RecordEntry<'h, SyntaxSpan>> {
    observer.count(Work::Hunk);
    let languages = headers
        .as_ref()
        .map(|header| Language::of_header(&header.path));
    let shared_language = languages.merge(|old, new| old.filter(|language| new == Some(*language)));
    let styled_records = BySide {
        old: StyledRecords::Changes,
        new: match shared_language {
            Some(_) => StyledRecords::ChangesAndContext,
            None => StyledRecords::Changes,
        },
    };

    let side_spans = observer.measure(Stage::Scan, |observer| {
        languages
            .zip(styled_records)
            .map_sides(|side, (language, styled)| {
                language.map_or_else(Vec::new, |language| {
                    scan_side(language.spec(), hunk, side, styled, observer)
                })
            })
    });

    side_spans.into_iter().flatten().collect()
}

/// The spans the scanner gives `side`'s `styled` records of `hunk`, reading every record
/// of the side in order from open ground so a region carries across records.
fn scan_side<'h>(
    spec: &Spec,
    hunk: HunkRecords<'_, 'h>,
    side: Side,
    styled: StyledRecords,
    observer: &mut impl Observer,
) -> Vec<RecordEntry<'h, SyntaxSpan>> {
    let mut spans = Vec::new();
    let side_records = hunk
        .positioned()
        .filter(|entry| entry.value.kind.on_side(side));

    side_records.fold(ScanState::default(), |state, entry| {
        let payload = entry.value.payload.sanitized();
        let shows = styled.includes(entry.value);
        observer.count(Work::ScannedBytes(payload.len()));
        spec.scan(state, payload, |bytes, class| {
            if shows {
                spans.push(RecordEntry {
                    position: entry.position,
                    value: SyntaxSpan {
                        bytes: PayloadSpan::within(payload, bytes),
                        class,
                    },
                });
            }
        })
    });

    observer.count(Work::Segments(spans.len()));
    spans
}

/// Which records on a side take its syntax spans.
#[derive(Clone, Copy)]
enum StyledRecords {
    /// Only the side's own changes.
    Changes,
    /// The side's changes and the context it shares with the other side.
    ChangesAndContext,
}

impl StyledRecords {
    /// Whether `record`, a record on the side, takes the side's spans.
    fn includes(self, record: &Record) -> bool {
        match self {
            Self::Changes => record.kind.changed_side().is_some(),
            Self::ChangesAndContext => true,
        }
    }
}

/// A language hdiff's scanner reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    JavaScript,
    Python,
    Go,
    C,
}

impl Language {
    /// The language a header path's extension selects; a path without a known
    /// extension, or a header without a path, selects none.
    fn of_header(header_path: &HeaderPath) -> Option<Self> {
        match header_path {
            HeaderPath::Path(path) => Self::of_path(path),
            HeaderPath::DevNull | HeaderPath::Undecodable => None,
        }
    }

    /// The language a path's extension selects; a path without a known extension
    /// selects none.
    fn of_path(path: &str) -> Option<Self> {
        let (_, extension) = path.rsplit_once('.')?;
        match extension {
            "rs" => Some(Self::Rust),
            "js" | "jsx" => Some(Self::JavaScript),
            "py" => Some(Self::Python),
            "go" => Some(Self::Go),
            "c" | "h" => Some(Self::C),
            _ => None,
        }
    }

    /// The spec the scanner reads the language with.
    fn spec(self) -> &'static Spec {
        match self {
            Self::Rust => &RUST,
            Self::JavaScript => &JAVASCRIPT,
            Self::Python => &PYTHON,
            Self::Go => &GO,
            Self::C => &C,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SyntaxClass, SyntaxSpan, highlight_file};
    use crate::{
        document::{AddressedHunk, FileBody},
        measurement::{Profile, Unobserved},
        parser::parse_patch,
    };

    /// The syntax spans of each of the `N` records of the first hunk of `patch`'s only
    /// file, in record order.
    fn highlights_of<const N: usize>(patch: &[u8]) -> [Vec<SyntaxSpan>; N] {
        let document = parse_patch(patch).expect("valid diff");
        let files = document.files().collect::<Vec<_>>();
        let [file] = files.as_slice() else {
            panic!("the patch holds one file, not {}", files.len());
        };
        let FileBody::Unified(body) = &file.body else {
            panic!("fixture should contain a unified file");
        };
        let highlights = highlight_file(file, &mut Unobserved);

        body.addressed_hunks()
            .take(1)
            .flat_map(AddressedHunk::records)
            .map(|record| highlights.get(record.address).clone())
            .collect::<Vec<_>>()
            .try_into()
            .expect("the hunk holds the expected record count")
    }

    /// Each span of a record's `spans` as the text it covers in `payload` and its class.
    fn pieces<'a>(payload: &'a str, spans: &[SyntaxSpan]) -> Vec<(&'a str, SyntaxClass)> {
        spans
            .iter()
            .map(|span| (&payload[span.bytes.start()..span.bytes.end()], span.class))
            .collect()
    }

    /// docs/spec/architecture.md#technology-direction: a side's records are read in
    /// order, so a comment an added line opens covers the next added line up to its
    /// close, and context takes the new side's syntax.
    #[test]
    fn highlights_go_across_a_side_s_records() {
        let [deleted, opening, closing, context] = highlights_of(
            b"--- a/main.go\n+++ b/main.go\n@@ -1,2 +1,3 @@\n-x := \"old\"\n+/* note\n+end */ y := 1\n return len(y)\n",
        );

        assert_eq!(
            pieces("x := \"old\"", &deleted),
            [
                ("x", SyntaxClass::Variable),
                (":=", SyntaxClass::Operator),
                ("\"old\"", SyntaxClass::String),
            ]
        );
        assert_eq!(
            pieces("/* note", &opening),
            [("/* note", SyntaxClass::Comment)]
        );
        assert_eq!(
            pieces("end */ y := 1", &closing),
            [
                ("end */", SyntaxClass::Comment),
                ("y", SyntaxClass::Variable),
                (":=", SyntaxClass::Operator),
                ("1", SyntaxClass::Number),
            ]
        );
        assert_eq!(
            pieces("return len(y)", &context),
            [
                ("return", SyntaxClass::Keyword),
                ("len", SyntaxClass::Function),
                ("y", SyntaxClass::Variable),
            ]
        );
    }

    /// docs/spec/architecture.md#technology-direction: matching languages take context
    /// styling from the new side.
    #[test]
    fn context_takes_the_new_side_s_syntax_when_the_old_side_is_in_a_comment() {
        let [opening, _, context, _, _] = highlights_of(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1,3 +1,3 @@\n-/*\n+// before\n let value = 1;\n-*/\n+// after\n",
        );

        assert_eq!(pieces("/*", &opening), [("/*", SyntaxClass::Comment)]);
        assert_eq!(
            pieces("let value = 1;", &context),
            [
                ("let", SyntaxClass::Keyword),
                ("=", SyntaxClass::Operator),
                ("1", SyntaxClass::Number),
            ]
        );
    }

    /// docs/spec/architecture.md#technology-direction: context remains textual when the
    /// paths select different languages.
    #[test]
    fn context_stays_textual_when_the_paths_select_different_languages() {
        let [context, deleted, added] = highlights_of(
            b"--- a/source.py\n+++ b/source.rs\n@@ -1,2 +1,2 @@\n fn kept() {}\n-def old(): pass\n+fn new() {}\n",
        );

        assert_eq!(context, []);
        assert_eq!(
            pieces("def old(): pass", &deleted),
            [
                ("def", SyntaxClass::Keyword),
                ("old", SyntaxClass::Function),
                ("pass", SyntaxClass::Keyword),
            ]
        );
        assert_eq!(
            pieces("fn new() {}", &added),
            [("fn", SyntaxClass::Keyword), ("new", SyntaxClass::Function)]
        );
    }

    #[test]
    fn counts_the_bytes_scanned_and_the_segments_given() {
        let mut profile = Profile::default();
        let document =
            parse_patch(b"--- a/a.go\n+++ b/a.go\n@@ -1 +1 @@\n-x := 1\n+y\n").expect("valid diff");
        let files = document.files().collect::<Vec<_>>();

        highlight_file(files[0], &mut profile);

        assert_eq!(profile.work().hunks, 1);
        assert_eq!(profile.work().scanned_bytes, "x := 1".len() + "y".len());
        assert_eq!(profile.work().segments, 4);
    }
}
