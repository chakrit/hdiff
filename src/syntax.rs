use std::ops::Range;

use tree_sitter::{InputEdit, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::{
    document::{DiffFile, HunkRecords, RecordEntry, RecordMap, RecordPosition},
    headers::{FileHeader, HeaderPath},
    measurement::{Observer, Stage, Work},
    side::{BySide, Side},
    span::PayloadSpan,
};

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

impl SyntaxClass {
    fn from_capture_name(name: &str) -> Option<Self> {
        match name.split('.').next()? {
            "keyword" => Some(Self::Keyword),
            "string" => Some(Self::String),
            "comment" => Some(Self::Comment),
            "number" => Some(Self::Number),
            "function" => Some(Self::Function),
            "type" => Some(Self::Type),
            "constant" => Some(Self::Constant),
            "operator" => Some(Self::Operator),
            "property" => Some(Self::Property),
            "variable" => Some(Self::Variable),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxSpan {
    /// The payload bytes the class covers.
    pub bytes: PayloadSpan,
    pub capture_length: usize,
    pub class: SyntaxClass,
}

/// Highlights files, keeping each language's highlighter across files.
#[derive(Default)]
pub struct SyntaxHighlighter {
    /// Each language's highlighter once first used.
    languages: ByLanguage<Option<LanguageHighlighter>>,
}

impl SyntaxHighlighter {
    /// The syntax spans of each record of `file`.
    pub fn highlight_file(
        &mut self,
        file: &DiffFile,
        observer: &mut impl Observer,
    ) -> RecordMap<Vec<SyntaxSpan>> {
        file.record_map(|headers, hunk| self.highlight_hunk(headers, hunk, observer))
    }

    /// The syntax spans of `hunk`, a hunk of the file with `headers`, each with the
    /// record it styles. Context stays textual when the paths select different languages.
    /// Rules: docs/spec/architecture.md#technology-direction.
    fn highlight_hunk<'h>(
        &mut self,
        headers: &BySide<FileHeader>,
        hunk: HunkRecords<'_, 'h>,
        observer: &mut impl Observer,
    ) -> Vec<RecordEntry<'h, SyntaxSpan>> {
        observer.count(Work::Hunk);
        let languages = headers
            .as_ref()
            .map(|header| Language::for_header(&header.path));
        let shared_language = languages
            .old
            .filter(|language| Some(*language) == languages.new);
        let styles = BySide {
            old: Styles::Changes,
            new: match shared_language {
                Some(_) => Styles::ChangesAndContext,
                None => Styles::Changes,
            },
        };
        let projections = observer.measure(Stage::Projection, |observer| {
            let projections =
                styles.map_sides(|side, styles| Projection::from_hunk(hunk, side, styles));
            for projection in projections.iter() {
                observer.count(Work::ProjectedBytes(projection.source.len()));
            }
            projections
        });

        let passes = match shared_language {
            Some(language) => {
                let old_pass =
                    self.highlight_projection(language, &projections.old, None, observer);
                let (old_spans, base) = match old_pass {
                    Some(pass) => {
                        let base = ParseBase {
                            projection: &projections.old,
                            tree: pass.tree,
                        };
                        (pass.spans, Some(base))
                    }
                    None => (Vec::new(), None),
                };
                let new_pass =
                    self.highlight_projection(language, &projections.new, base, observer);
                BySide {
                    old: old_spans,
                    new: new_pass.map_or_else(Vec::new, |pass| pass.spans),
                }
            }
            None => languages
                .zip(projections.as_ref())
                .map(|(selected, projection)| {
                    selected
                        .and_then(|language| {
                            self.highlight_projection(language, projection, None, observer)
                        })
                        .map_or_else(Vec::new, |pass| pass.spans)
                }),
        };

        passes.into_iter().flatten().collect()
    }

    /// Highlights `projection`, parsing it as an edit of `base`'s tree when given. A
    /// projection without styled records is not parsed.
    fn highlight_projection<'h>(
        &mut self,
        language: Language,
        projection: &Projection<'h>,
        base: Option<ParseBase<'_, 'h>>,
        observer: &mut impl Observer,
    ) -> Option<Highlighted<'h>> {
        if projection.records.is_empty() {
            return None;
        }
        let highlighter = observer.measure(Stage::Language, |_| self.highlighter_for(language));

        let tree = observer.measure(Stage::Parse, |observer| {
            let edited_base = base.map(
                |ParseBase {
                     projection: from,
                     mut tree,
                 }| {
                    tree.edit(&from.edit_to(projection));
                    tree
                },
            );
            observer.count(Work::ParseCall);
            highlighter.parse(projection.source.as_bytes(), edited_base.as_ref())
        });
        let spans = observer.measure(Stage::Query, |observer| {
            highlighter.highlight(projection, &tree, observer)
        });

        Some(Highlighted { tree, spans })
    }

    /// The highlighter for `language`, loaded on first use.
    fn highlighter_for(&mut self, language: Language) -> &mut LanguageHighlighter {
        self.languages
            .get_mut(language)
            .get_or_insert_with(|| LanguageHighlighter::new(language))
    }
}

/// One projection's parse tree and the spans its captures give its records.
struct Highlighted<'h> {
    tree: Tree,
    spans: Vec<RecordEntry<'h, SyntaxSpan>>,
}

/// The tree an edited parse starts from, with the projection it was parsed from.
struct ParseBase<'p, 'h> {
    projection: &'p Projection<'h>,
    tree: Tree,
}

// Languages

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    JavaScript,
    Python,
    Go,
    C,
}

impl Language {
    fn for_header(path: &HeaderPath) -> Option<Self> {
        let HeaderPath::Path(text) = path else {
            return None;
        };

        match text.rsplit_once('.')?.1 {
            "rs" => Some(Self::Rust),
            "js" | "jsx" => Some(Self::JavaScript),
            "py" => Some(Self::Python),
            "go" => Some(Self::Go),
            "c" | "h" => Some(Self::C),
            _ => None,
        }
    }
}

/// One value for each language.
#[derive(Default)]
struct ByLanguage<T> {
    rust: T,
    javascript: T,
    python: T,
    go: T,
    c: T,
}

impl<T> ByLanguage<T> {
    /// A table holding `value` of every language; naming each field means a new
    /// language cannot be left out.
    #[cfg(test)]
    fn from_fn(mut value: impl FnMut(Language) -> T) -> Self {
        Self {
            rust: value(Language::Rust),
            javascript: value(Language::JavaScript),
            python: value(Language::Python),
            go: value(Language::Go),
            c: value(Language::C),
        }
    }

    fn get_mut(&mut self, language: Language) -> &mut T {
        match language {
            Language::Rust => &mut self.rust,
            Language::JavaScript => &mut self.javascript,
            Language::Python => &mut self.python,
            Language::Go => &mut self.go,
            Language::C => &mut self.c,
        }
    }
}

struct LanguageHighlighter {
    parser: Parser,
    query: Query,
    cursor: QueryCursor,
}

impl LanguageHighlighter {
    /// The highlighter of a bundled grammar and its highlight query.
    ///
    /// Panics when the bundled grammar or query does not load, which the bundled-grammar
    /// test rules out for every language.
    fn new(language: Language) -> Self {
        let (grammar, highlights) = match language {
            Language::Rust => (
                tree_sitter_rust::LANGUAGE.into(),
                tree_sitter_rust::HIGHLIGHTS_QUERY,
            ),
            Language::JavaScript => (
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::HIGHLIGHT_QUERY,
            ),
            Language::Python => (
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::HIGHLIGHTS_QUERY,
            ),
            Language::Go => (
                tree_sitter_go::LANGUAGE.into(),
                tree_sitter_go::HIGHLIGHTS_QUERY,
            ),
            Language::C => (
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::HIGHLIGHT_QUERY,
            ),
        };

        let mut parser = Parser::new();
        parser
            .set_language(&grammar)
            .expect("a bundled grammar matches the linked tree-sitter");
        let query = Query::new(&grammar, highlights).expect("a bundled highlight query is valid");

        Self {
            parser,
            query,
            cursor: QueryCursor::new(),
        }
    }

    fn parse(&mut self, source: &[u8], old_tree: Option<&Tree>) -> Tree {
        self.parser
            .parse(source, old_tree)
            .expect("a parser with a language, no timeout, and no cancellation returns a tree")
    }

    /// The spans the query's captures over `tree` give `projection`'s records.
    fn highlight<'h>(
        &mut self,
        projection: &Projection<'h>,
        tree: &Tree,
        observer: &mut impl Observer,
    ) -> Vec<RecordEntry<'h, SyntaxSpan>> {
        let capture_names = self.query.capture_names();
        let mut captures =
            self.cursor
                .captures(&self.query, tree.root_node(), projection.source.as_bytes());
        let mut spans = Vec::new();

        while let Some((query_match, index)) = captures.next() {
            observer.count(Work::Capture);
            let capture = query_match.captures()[*index];
            let name_index = usize::try_from(capture.index).expect("capture indexes fit in usize");
            let Some(class) = SyntaxClass::from_capture_name(capture_names[name_index]) else {
                continue;
            };
            let node = capture.node;
            spans.extend(projection.map_span(node.byte_range(), class));
        }
        spans
    }
}

// Projections

/// Which records on a projection's side take its syntax spans.
#[derive(Clone, Copy)]
enum Styles {
    /// Only the side's own changes.
    Changes,
    /// The side's changes and the context it shares with the other side.
    ChangesAndContext,
}

/// One side's records of a hunk joined into a virtual source buffer.
struct Projection<'h> {
    source: String,
    /// The records that take this projection's spans, in source order.
    records: Vec<ProjectedRecord<'h>>,
}

/// A record's payload range in its projection's source.
struct ProjectedRecord<'h> {
    position: RecordPosition<'h>,
    source: Range<usize>,
}

impl<'h> Projection<'h> {
    /// `side`'s records of `hunk`; a side without records projects no source.
    fn from_hunk(hunk: HunkRecords<'_, 'h>, side: Side, styles: Styles) -> Self {
        let mut source = String::new();
        let mut records = Vec::new();
        let side_records = hunk
            .positioned()
            .filter(|(_, record)| record.kind.on_side(side));

        for (position, record) in side_records {
            let start = source.len();
            source.push_str(record.payload.sanitized());
            let styled = match styles {
                Styles::Changes => record.kind.changed_side().is_some(),
                Styles::ChangesAndContext => true,
            };
            if styled {
                records.push(ProjectedRecord {
                    position,
                    source: start..source.len(),
                });
            }
            source.push('\n');
        }

        Self { source, records }
    }

    /// The spans the `source` range gives the records it overlaps, clipped to each
    /// record's payload.
    fn map_span(
        &self,
        source: Range<usize>,
        class: SyntaxClass,
    ) -> impl Iterator<Item = RecordEntry<'h, SyntaxSpan>> + '_ {
        let first_record = self.first_overlapping_record(source.start);
        let capture_end = source.end;
        let capture_length = source.len();

        self.records
            .iter()
            .skip(first_record)
            .take_while(move |record| record.source.start < capture_end)
            .filter_map(move |record| {
                let offset = record.source.start;
                let clipped_start = source.start.max(offset) - offset;
                let clipped_end = source.end.min(record.source.end) - offset;
                let clipped = clipped_start..clipped_end;
                if clipped.is_empty() {
                    return None;
                }

                let payload = &self.source[record.source.clone()];
                let span = SyntaxSpan {
                    bytes: PayloadSpan::within(payload, clipped),
                    capture_length,
                    class,
                };
                Some(RecordEntry {
                    position: record.position,
                    value: span,
                })
            })
    }

    fn edit_to(&self, newer: &Self) -> InputEdit {
        let common_prefix = self
            .source
            .bytes()
            .zip(newer.source.bytes())
            .take_while(|(old, new)| old == new)
            .count();
        let shared_limit = self.source.len().min(newer.source.len()) - common_prefix;
        let common_suffix = self
            .source
            .bytes()
            .rev()
            .zip(newer.source.bytes().rev())
            .take(shared_limit)
            .take_while(|(old, new)| old == new)
            .count();
        let start_byte = common_prefix;
        let old_end_byte = self.source.len() - common_suffix;
        let new_end_byte = newer.source.len() - common_suffix;

        InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: point_at(self.source.as_bytes(), start_byte),
            old_end_position: point_at(self.source.as_bytes(), old_end_byte),
            new_end_position: point_at(newer.source.as_bytes(), new_end_byte),
        }
    }

    fn first_overlapping_record(&self, start: usize) -> usize {
        self.records
            .partition_point(|record| record.source.end <= start)
    }
}

fn point_at(source: &[u8], offset: usize) -> Point {
    let prefix = &source[..offset];
    let row = prefix.iter().filter(|&&b| b == b'\n').count();
    let column = prefix
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(offset, |newline| offset - newline - 1);

    Point::new(row, column)
}

#[cfg(test)]
mod tests {
    use tree_sitter::{InputEdit, Point};

    use super::{
        ByLanguage, Language, LanguageHighlighter, Projection, Styles, SyntaxClass,
        SyntaxHighlighter, SyntaxSpan,
    };
    use crate::{
        document::{DiffFile, FileBody, HunkRecords, RecordEntry, UnifiedBody},
        measurement::{Observer, Profile, Unobserved},
        parser::parse_patch,
        side::Side,
        span::PayloadSpan,
    };

    /// The classes of `spans`, in order.
    fn classes(spans: &[SyntaxSpan]) -> Vec<SyntaxClass> {
        spans.iter().map(|span| span.class).collect()
    }

    /// The syntax spans of each record of the first hunk of `patch`'s first file, in
    /// record order.
    fn highlights_of(patch: &[u8]) -> Vec<Vec<SyntaxSpan>> {
        observed_highlights_of(patch, &mut Unobserved)
    }

    fn observed_highlights_of(patch: &[u8], observer: &mut impl Observer) -> Vec<Vec<SyntaxSpan>> {
        with_first_unified(patch, |file, body| {
            let highlights = SyntaxHighlighter::default().highlight_file(file, observer);

            body.addressed_hunks()
                .next()
                .expect("first hunk")
                .records()
                .map(|(address, _)| highlights.get(address).clone())
                .collect()
        })
    }

    /// `test` applied to the records of the first hunk of `patch`'s first file, which
    /// must be unified.
    fn with_first_hunk<T>(patch: &[u8], test: impl for<'h> FnOnce(HunkRecords<'_, 'h>) -> T) -> T {
        with_first_unified(patch, |_, body| body.hunks.first().with_records(test))
    }

    /// `test` applied to `patch`'s first file and its body, which must be unified.
    fn with_first_unified<T>(patch: &[u8], test: impl FnOnce(&DiffFile, &UnifiedBody) -> T) -> T {
        let document = parse_patch(patch).expect("valid diff");
        let file = document.files().next().expect("first file");
        let FileBody::Unified(body) = &file.body else {
            panic!("fixture should contain a unified file");
        };

        test(file, body)
    }

    /// `test` applied to the new-side projection of a hunk adding `lines`.
    fn with_added_projection<T>(lines: &[&str], test: impl FnOnce(&Projection) -> T) -> T {
        let added = lines
            .iter()
            .map(|line| format!("+{line}\n"))
            .collect::<String>();
        let patch = format!("--- a/x\n+++ b/x\n@@ -0,0 +1,{} @@\n{added}", lines.len());

        with_first_hunk(patch.as_bytes(), |hunk| {
            test(&Projection::from_hunk(hunk, Side::New, Styles::Changes))
        })
    }

    #[test]
    fn describes_the_changed_projection_range_for_incremental_parsing() {
        let old = Projection {
            source: "let old = 1;\n".to_owned(),
            records: Vec::new(),
        };
        let new = Projection {
            source: "let new = 2;\n".to_owned(),
            records: Vec::new(),
        };

        assert_eq!(
            old.edit_to(&new),
            InputEdit {
                start_byte: 4,
                old_end_byte: 11,
                new_end_byte: 11,
                start_position: Point::new(0, 4),
                old_end_position: Point::new(0, 11),
                new_end_position: Point::new(0, 11),
            }
        );
    }

    #[test]
    fn highlights_added_rust_keywords_in_the_new_side() {
        let highlights =
            highlights_of(b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-old\n+fn added() {}\n");

        assert_eq!(highlights[1][0].class, SyntaxClass::Keyword);
        assert_eq!(
            highlights[1][0].bytes,
            PayloadSpan::within("fn added() {}", 0..2)
        );
    }

    /// docs/spec/architecture.md#technology-direction: matching languages use the new
    /// projection for context styling.
    #[test]
    fn context_uses_new_projection_syntax_when_old_context_is_a_comment() {
        let highlights = highlights_of(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1,3 +1,3 @@\n-/*\n+// before\n let value = 1;\n-*/\n+// after\n",
        );

        let context = classes(&highlights[2]);
        assert!(context.contains(&SyntaxClass::Keyword), "{context:?}");
        assert!(!context.contains(&SyntaxClass::Comment), "{context:?}");
        let deleted = classes(&highlights[0]);
        assert!(deleted.contains(&SyntaxClass::Comment), "{deleted:?}");
        let added = classes(&highlights[3]);
        assert!(added.contains(&SyntaxClass::Comment), "{added:?}");
    }

    #[test]
    fn incremental_new_projection_matches_a_full_query_pass() {
        let patch = b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
        let incremental_spans = highlights_of(patch);

        let full_spans = with_first_hunk(patch, |hunk| {
            let (addition, _) = hunk.positioned().nth(1).expect("the added record");
            let new_projection = Projection::from_hunk(hunk, Side::New, Styles::ChangesAndContext);
            let full_pass = SyntaxHighlighter::default()
                .highlight_projection(Language::Rust, &new_projection, None, &mut Unobserved)
                .expect("the new side has styled records");
            full_pass
                .spans
                .into_iter()
                .filter(|entry| entry.position == addition)
                .map(|entry| entry.value)
                .collect::<Vec<_>>()
        });

        assert_eq!(incremental_spans[1], full_spans);
        assert_eq!(incremental_spans[1][0].class, SyntaxClass::Keyword);
    }

    #[test]
    fn finds_the_first_record_overlapping_a_source_span() {
        with_added_projection(&["aaaa", "bbbb", "cccc"], |projection| {
            assert_eq!(projection.source, "aaaa\nbbbb\ncccc\n");
            assert_eq!(projection.first_overlapping_record(0), 0);
            assert_eq!(projection.first_overlapping_record(4), 1);
            assert_eq!(projection.first_overlapping_record(9), 2);
        });
    }

    #[test]
    fn maps_only_records_overlapping_a_source_span() {
        let spans = with_added_projection(&["aaaa", "bbbb", "cccc"], |projection| {
            projection
                .map_span(2..12, SyntaxClass::Keyword)
                .map(
                    |RecordEntry {
                         position,
                         value: span,
                     }| {
                        let projected_index = projection
                            .records
                            .iter()
                            .position(|projected| projected.position == position)
                            .expect("a mapped span names a projected record");
                        (projected_index, span.bytes)
                    },
                )
                .collect::<Vec<_>>()
        });

        assert_eq!(
            spans,
            [
                (0, PayloadSpan::within("aaaa", 2..4)),
                (1, PayloadSpan::within("bbbb", 0..4)),
                (2, PayloadSpan::within("cccc", 0..2)),
            ]
        );
    }

    #[test]
    fn does_not_map_a_source_span_in_a_record_separator() {
        with_added_projection(&["aaaa", "bbbb"], |projection| {
            assert_eq!(projection.map_span(4..5, SyntaxClass::Keyword).count(), 0);
        });
    }

    /// docs/spec/architecture.md#technology-direction: context remains textual when the
    /// paths select different languages.
    #[test]
    fn context_stays_textual_when_the_paths_select_different_languages() {
        let highlights = highlights_of(
            b"--- a/source.py\n+++ b/source.rs\n@@ -1,2 +1,2 @@\n fn kept() {}\n-def old(): pass\n+fn new() {}\n",
        );

        assert_eq!(highlights[0], []);
        let deleted = classes(&highlights[1]);
        assert!(deleted.contains(&SyntaxClass::Keyword), "{deleted:?}");
        let added = classes(&highlights[2]);
        assert!(added.contains(&SyntaxClass::Keyword), "{added:?}");
    }

    #[test]
    fn every_bundled_language_loads_its_grammar_and_highlight_query() {
        ByLanguage::from_fn(LanguageHighlighter::new);
    }

    #[test]
    fn a_side_with_only_context_is_not_parsed() {
        let mut profile = Profile::default();

        observed_highlights_of(
            b"--- a/source.py\n+++ b/source.rs\n@@ -1 +1,2 @@\n fn kept() {}\n+fn new() {}\n",
            &mut profile,
        );

        assert_eq!(profile.work().parse_calls, 1);
    }
}
