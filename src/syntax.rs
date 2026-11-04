//! The syntax of each displayed line, from hdiff's own scanner. Rules:
//! docs/spec/architecture.md#technology-direction.

use std::ptr;

use crate::{
    display_lines::{LineKind, RecordPart},
    document::DiffFile,
    languages,
    measurement::{Observer, Work},
    scanner::{ScanState, Spec, languages as scanner_languages},
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

/// A page's syntax read line by line in display order: each side's language and where
/// its scan stands. Each side reads its own records in order, so a region an earlier
/// record opens carries into the next; a hunk starts both sides on open ground.
pub struct DisplayScan {
    languages: BySide<Option<&'static Spec>>,
    /// Whether both sides read one language, so context takes the new side's syntax;
    /// otherwise context stays textual.
    shared: bool,
    states: BySide<ScanState>,
}

impl DisplayScan {
    /// The scan of `file`'s lines, each side in its path's language.
    pub fn of_file(file: &DiffFile) -> Self {
        Self::of_file_types(languages::of_file(file))
    }

    /// The scan of a file whose sides select `file_types`, each side in its file type's
    /// language.
    pub fn of_file_types(file_types: BySide<Option<&'static str>>) -> Self {
        let languages =
            file_types.map(|file_type| file_type.and_then(scanner_languages::of_file_type));
        let shared = languages
            .merge(|old, new| old.zip(new))
            .is_some_and(|(old, new)| ptr::eq(old, new));

        Self {
            languages,
            shared,
            states: BySide::default(),
        }
    }

    /// The scan of lines in no language, such as surrounding text.
    pub fn textual() -> Self {
        Self {
            languages: BySide::default(),
            shared: false,
            states: BySide::default(),
        }
    }

    /// Gives `emit` the syntax spans of the next displayed line, of `kind` with
    /// `payload`, in order, and advances the scan past it.
    pub fn line(
        &mut self,
        kind: LineKind,
        payload: &str,
        observer: &mut impl Observer,
        mut emit: impl FnMut(PayloadSpan, SyntaxClass),
    ) {
        match kind {
            LineKind::HunkHeader => {
                observer.count(Work::Hunk);
                self.states = BySide::default();
            }
            LineKind::Record {
                kind: record_kind,
                part: RecordPart::Payload,
                ..
            } => match record_kind.changed_side() {
                Some(side) => self.read(side, payload, observer, &mut emit),
                None => self.context(payload, observer, &mut emit),
            },
            LineKind::Record {
                part: RecordPart::NoNewlineMarker,
                ..
            }
            | LineKind::Text
            | LineKind::Metadata
            | LineKind::FileHeader
            | LineKind::Failure => {}
        }
    }

    /// Reads a context line on both sides; it takes the new side's spans when both read
    /// one language. When both sides stand in the same state, which is usual, it is read
    /// once.
    fn context(
        &mut self,
        payload: &str,
        observer: &mut impl Observer,
        emit: &mut impl FnMut(PayloadSpan, SyntaxClass),
    ) {
        if self.shared && self.states.old == self.states.new {
            self.read(Side::New, payload, observer, emit);
            self.states.old = self.states.new;
            return;
        }

        self.advance(Side::Old, payload, observer);
        if self.shared {
            self.read(Side::New, payload, observer, emit);
        } else {
            self.advance(Side::New, payload, observer);
        }
    }

    /// Reads `payload` as the next line of `side` only to keep the side's state; its
    /// spans go nowhere.
    fn advance(&mut self, side: Side, payload: &str, observer: &mut impl Observer) {
        self.scan_side(side, payload, observer, |_, _| {});
    }

    /// Reads `payload` as the next line of `side`, giving `emit` its spans; a side
    /// without a language reads nothing.
    fn read(
        &mut self,
        side: Side,
        payload: &str,
        observer: &mut impl Observer,
        emit: &mut impl FnMut(PayloadSpan, SyntaxClass),
    ) {
        let mut segments = 0;
        self.scan_side(side, payload, observer, |span, class| {
            segments += 1;
            emit(span, class);
        });

        observer.count(Work::Segments(segments));
    }

    /// Scans `payload` as the next line of `side` from the side's state, giving `spans`
    /// each span; a side without a language reads nothing.
    fn scan_side(
        &mut self,
        side: Side,
        payload: &str,
        observer: &mut impl Observer,
        mut spans: impl FnMut(PayloadSpan, SyntaxClass),
    ) {
        let Some(spec) = self.languages.get(side) else {
            return;
        };

        let state = self.states.get_mut(side);
        *state = spec.scan(*state, payload, |bytes, class| {
            spans(PayloadSpan::within(payload, bytes), class);
        });
        observer.count(Work::ScannedBytes(payload.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::{DisplayScan, SyntaxClass};
    use crate::{
        measurement::{Profile, Unobserved},
        page_rows::PageRows,
        parser::parse_patch,
    };

    /// Each displayed line of `patch`'s only file page, as the text and class of each
    /// syntax span, in display order.
    fn line_pieces(patch: &str) -> Vec<Vec<(String, SyntaxClass)>> {
        let document = parse_patch(patch.as_bytes()).expect("valid diff");
        let views = document.file_views().collect::<Vec<_>>();
        let [view] = views.as_slice() else {
            panic!("the patch holds one file, not {}", views.len());
        };
        let rows = PageRows::of_file_view(*view);
        let mut scan = DisplayScan::of_file(view.file);

        rows.lines()
            .map(|(_, line)| {
                let payload = line.payload();
                let mut pieces = Vec::new();
                scan.line(line.kind(), payload, &mut Unobserved, |span, class| {
                    pieces.push((payload[span.start()..span.end()].to_owned(), class));
                });
                pieces
            })
            .collect()
    }

    /// `pieces` with owned text, for comparing with [`line_pieces`].
    fn owned(pieces: &[(&str, SyntaxClass)]) -> Vec<(String, SyntaxClass)> {
        pieces
            .iter()
            .map(|(text, class)| ((*text).to_owned(), *class))
            .collect()
    }

    /// docs/spec/architecture.md#technology-direction: a side's records are read in
    /// order, so a comment an added line opens covers the next added line up to its
    /// close, and context takes the new side's syntax.
    #[test]
    fn highlights_go_across_a_side_s_records() {
        let lines = line_pieces(
            "--- a/main.go\n+++ b/main.go\n@@ -1,2 +1,3 @@\n-x := \"old\"\n+/* note\n+end */ y := 1\n return len(y)\n",
        );

        assert_eq!(
            lines,
            [
                vec![],
                vec![],
                vec![],
                owned(&[
                    ("x", SyntaxClass::Variable),
                    (":=", SyntaxClass::Operator),
                    ("\"old\"", SyntaxClass::String),
                ]),
                owned(&[("/* note", SyntaxClass::Comment)]),
                owned(&[
                    ("end */", SyntaxClass::Comment),
                    ("y", SyntaxClass::Variable),
                    (":=", SyntaxClass::Operator),
                    ("1", SyntaxClass::Number),
                ]),
                owned(&[
                    ("return", SyntaxClass::Keyword),
                    ("len", SyntaxClass::Function),
                    ("y", SyntaxClass::Variable),
                ]),
            ]
        );
    }

    /// docs/spec/architecture.md#technology-direction: matching languages take context
    /// styling from the new side, while the old side keeps its own state.
    #[test]
    fn context_takes_the_new_side_s_syntax_when_the_old_side_is_in_a_comment() {
        let lines = line_pieces(
            "--- a/source.rs\n+++ b/source.rs\n@@ -1,3 +1,3 @@\n-/*\n+// before\n let value = 1;\n-*/\n+// after\n",
        );

        assert_eq!(
            lines[3..],
            [
                owned(&[("/*", SyntaxClass::Comment)]),
                owned(&[("// before", SyntaxClass::Comment)]),
                owned(&[
                    ("let", SyntaxClass::Keyword),
                    ("=", SyntaxClass::Operator),
                    ("1", SyntaxClass::Number),
                ]),
                owned(&[("*/", SyntaxClass::Comment)]),
                owned(&[("// after", SyntaxClass::Comment)]),
            ]
        );
    }

    /// docs/spec/architecture.md#technology-direction: context remains textual when the
    /// paths select different languages.
    #[test]
    fn context_stays_textual_when_the_paths_select_different_languages() {
        let lines = line_pieces(
            "--- a/source.py\n+++ b/source.rs\n@@ -1,2 +1,2 @@\n fn kept() {}\n-def old(): pass\n+fn new() {}\n",
        );

        assert_eq!(
            lines[3..],
            [
                vec![],
                owned(&[
                    ("def", SyntaxClass::Keyword),
                    ("old", SyntaxClass::Function),
                    ("pass", SyntaxClass::Keyword),
                ]),
                owned(&[("fn", SyntaxClass::Keyword), ("new", SyntaxClass::Function)]),
            ]
        );
    }

    /// docs/spec/architecture.md#technology-direction: each hunk starts on open ground.
    #[test]
    fn a_hunk_header_ends_a_region_an_earlier_hunk_left_open() {
        let lines =
            line_pieces("--- a/a.go\n+++ b/a.go\n@@ -1 +1 @@\n-x\n+/* open\n@@ -9 +9 @@\n-y\n+z\n");

        assert_eq!(lines[7], owned(&[("z", SyntaxClass::Variable)]));
    }

    /// docs/spec/performance.md#stages: segments count the spans given to styled
    /// records, not the spans of a side read only to keep its state.
    #[test]
    fn counts_only_the_segments_given_to_styled_records() {
        let document = parse_patch(b"--- a/a.go\n+++ b/a.go\n@@ -1,2 +1,1 @@\n-/* x\n y\n")
            .expect("valid diff");
        let views = document.file_views().collect::<Vec<_>>();
        let rows = PageRows::of_file_view(views[0]);
        let mut scan = DisplayScan::of_file(views[0].file);
        let mut profile = Profile::default();
        let mut given = 0;

        for (_, line) in rows.lines() {
            scan.line(line.kind(), line.payload(), &mut profile, |_, _| given += 1);
        }

        assert_eq!(given, 2);
        assert_eq!(profile.work().segments, given);
    }

    #[test]
    fn counts_hunks_scanned_bytes_and_segments() {
        let document =
            parse_patch(b"--- a/a.go\n+++ b/a.go\n@@ -1 +1 @@\n-x := 1\n+y\n").expect("valid diff");
        let views = document.file_views().collect::<Vec<_>>();
        let rows = PageRows::of_file_view(views[0]);
        let mut scan = DisplayScan::of_file(views[0].file);
        let mut profile = Profile::default();

        for (_, line) in rows.lines() {
            scan.line(line.kind(), line.payload(), &mut profile, |_, _| {});
        }

        assert_eq!(profile.work().hunks, 1);
        assert_eq!(profile.work().scanned_bytes, "x := 1".len() + "y".len());
        assert_eq!(profile.work().segments, 4);
    }
}
