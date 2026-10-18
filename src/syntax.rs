use std::{fmt, ops::Range};

use tree_sitter::{InputEdit, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::{
    document::{DiffFile, Hunk, RecordMap, Side, UnifiedBody},
    headers::HeaderPath,
    measurement::{Observer, Stage, Work},
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

#[derive(Debug, PartialEq, Eq)]
pub struct SyntaxSpan {
    pub span: PayloadSpan,
    pub capture_length: usize,
    pub class: SyntaxClass,
}

/// Highlights files, keeping each language's highlighter, or the reason it failed to
/// load, across files.
pub struct SyntaxHighlighter {
    /// Each language's highlighter once first used, at its [`Language::slot`].
    languages: [Option<Result<LanguageHighlighter, LanguageError>>; Language::ALL.len()],
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self {
            languages: [const { None }; Language::ALL.len()],
        }
    }
}

impl SyntaxHighlighter {
    /// The syntax spans of each record of `file`.
    pub fn highlight_file(
        &mut self,
        file: &DiffFile,
        observer: &mut impl Observer,
    ) -> RecordMap<Vec<SyntaxSpan>> {
        file.record_map(|body, hunk| self.highlight_hunk(body, hunk, observer))
    }

    /// The syntax spans of each record of `hunk`, a hunk of `file`.
    fn highlight_hunk(
        &mut self,
        file: &UnifiedBody,
        hunk: &Hunk,
        observer: &mut impl Observer,
    ) -> Box<[Vec<SyntaxSpan>]> {
        observer.count(Work::Hunk);
        let mut spans = hunk.records.iter().map(|_| Vec::new()).collect::<Vec<_>>();
        let old_language = Language::for_header(&file.old.path);
        let new_language = Language::for_header(&file.new.path);
        // architecture.md: context stays textual when the paths select different languages.
        let shared_language = old_language.filter(|language| Some(*language) == new_language);
        let new_styles = match shared_language {
            Some(_) => Styles::ChangesAndContext,
            None => Styles::Changes,
        };
        let (old, new) = observer.measure(Stage::Projection, |observer| {
            let old = Projection::from_hunk(hunk, Side::Old, Styles::Changes);
            let new = Projection::from_hunk(hunk, Side::New, new_styles);
            for projection in [&old, &new] {
                observer.count(Work::ProjectedBytes(projection.source.len()));
            }
            (old, new)
        });

        match shared_language {
            Some(language) => {
                let base = self
                    .highlight_projection(language, &old, None, &mut spans, observer)
                    .map(|tree| (&old, tree));
                self.highlight_projection(language, &new, base, &mut spans, observer);
            }
            None => {
                for (language, projection) in [(old_language, &old), (new_language, &new)] {
                    let Some(language) = language else {
                        continue;
                    };
                    self.highlight_projection(language, projection, None, &mut spans, observer);
                }
            }
        }

        spans.into_boxed_slice()
    }

    /// Highlights `projection`, parsing it as an edit of `base`'s tree when given, and
    /// returns its tree. A projection without styled records is not parsed, and a
    /// language that failed to load highlights nothing.
    fn highlight_projection(
        &mut self,
        language: Language,
        projection: &Projection,
        base: Option<(&Projection, Tree)>,
        spans: &mut [Vec<SyntaxSpan>],
        observer: &mut impl Observer,
    ) -> Option<Tree> {
        if projection.records.is_empty() {
            return None;
        }
        let Ok(highlighter) = observer.measure(Stage::Language, |_| self.highlighter_for(language))
        else {
            // architecture.md: a language that fails to load stays textual.
            return None;
        };

        let tree = observer.measure(Stage::Parse, |observer| {
            let edited_base = base.map(|(base_projection, mut tree)| {
                tree.edit(&base_projection.edit_to(projection));
                tree
            });
            observer.count(Work::Parse);
            highlighter.parse(projection.source.as_bytes(), edited_base.as_ref())
        });
        observer.measure(Stage::Query, |observer| {
            highlighter.highlight(projection, &tree, spans, observer)
        });

        Some(tree)
    }

    /// The highlighter for `language`, loaded on first use; a language that failed to load
    /// keeps its error and is not loaded again.
    fn highlighter_for(
        &mut self,
        language: Language,
    ) -> Result<&mut LanguageHighlighter, &LanguageError> {
        self.languages[language.slot()]
            .get_or_insert_with(|| LanguageHighlighter::new(language))
            .as_mut()
            .map_err(|error| &*error)
    }
}

// Languages

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Language {
    Rust,
    JavaScript,
    Python,
    Go,
    C,
}

impl Language {
    /// Every language, in [`slot`](Self::slot) order.
    const ALL: [Self; 5] = [
        Self::Rust,
        Self::JavaScript,
        Self::Python,
        Self::Go,
        Self::C,
    ];

    /// The language's position in [`ALL`](Self::ALL).
    const fn slot(self) -> usize {
        match self {
            Self::Rust => 0,
            Self::JavaScript => 1,
            Self::Python => 2,
            Self::Go => 3,
            Self::C => 4,
        }
    }

    fn for_header(path: &HeaderPath) -> Option<Self> {
        let HeaderPath::Path(path) = path else {
            return None;
        };

        match path.rsplit_once('.')?.1 {
            "rs" => Some(Self::Rust),
            "js" | "jsx" => Some(Self::JavaScript),
            "py" => Some(Self::Python),
            "go" => Some(Self::Go),
            "c" | "h" => Some(Self::C),
            _ => None,
        }
    }
}

/// Why a bundled grammar or its highlight query could not be loaded.
#[derive(Debug)]
enum LanguageError {
    Grammar(tree_sitter::LanguageError),
    Query(tree_sitter::QueryError),
}

impl fmt::Display for LanguageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Grammar(error) => write!(formatter, "incompatible grammar: {error}"),
            Self::Query(error) => write!(formatter, "invalid highlight query: {error}"),
        }
    }
}

impl std::error::Error for LanguageError {}

struct LanguageHighlighter {
    parser: Parser,
    query: Query,
    cursor: QueryCursor,
}

impl LanguageHighlighter {
    fn new(language: Language) -> Result<Self, LanguageError> {
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
            .map_err(LanguageError::Grammar)?;
        let query = Query::new(&grammar, highlights).map_err(LanguageError::Query)?;

        Ok(Self {
            parser,
            query,
            cursor: QueryCursor::new(),
        })
    }

    fn parse(&mut self, source: &[u8], old_tree: Option<&Tree>) -> Tree {
        self.parser
            .parse(source, old_tree)
            .expect("a parser with a language, no timeout, and no cancellation returns a tree")
    }

    fn highlight(
        &mut self,
        projection: &Projection,
        tree: &Tree,
        spans: &mut [Vec<SyntaxSpan>],
        observer: &mut impl Observer,
    ) {
        let capture_names = self.query.capture_names();
        let mut captures =
            self.cursor
                .captures(&self.query, tree.root_node(), projection.source.as_bytes());

        while let Some((query_match, index)) = captures.next() {
            observer.count(Work::Capture);
            let capture = query_match.captures()[*index];
            let Some(class) = SyntaxClass::from_capture_name(capture_names[capture.index as usize])
            else {
                continue;
            };
            projection.map_span(
                capture.node.start_byte(),
                capture.node.end_byte(),
                class,
                spans,
            );
        }
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
struct Projection {
    source: String,
    /// The records that take this projection's spans, in source order.
    records: Vec<ProjectedRecord>,
}

/// A record's payload range in its projection's source.
struct ProjectedRecord {
    record_index: usize,
    source: Range<usize>,
}

impl Projection {
    /// `side`'s records of `hunk`; a side without records projects no source.
    fn from_hunk(hunk: &Hunk, side: Side, styles: Styles) -> Self {
        let mut source = String::new();
        let mut records = Vec::new();
        let side_records = hunk
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| record.kind.on_side(side));

        for (record_index, record) in side_records {
            let start = source.len();
            source.push_str(record.payload.sanitized());
            let styled = match styles {
                Styles::Changes => record.kind.changed_side().is_some(),
                Styles::ChangesAndContext => true,
            };
            if styled {
                records.push(ProjectedRecord {
                    record_index,
                    source: start..source.len(),
                });
            }
            source.push('\n');
        }

        Self { source, records }
    }

    fn map_span(
        &self,
        start: usize,
        end: usize,
        class: SyntaxClass,
        spans: &mut [Vec<SyntaxSpan>],
    ) {
        let first_record = self.first_overlapping_record(start);
        for record in self
            .records
            .iter()
            .skip(first_record)
            .take_while(|record| record.source.start < end)
        {
            let mapped_start = start.max(record.source.start);
            let mapped_end = end.min(record.source.end);
            if mapped_start < mapped_end {
                let payload = &self.source[record.source.clone()];
                spans[record.record_index].push(SyntaxSpan {
                    span: PayloadSpan::within(
                        payload,
                        mapped_start - record.source.start,
                        mapped_end - record.source.start,
                    ),
                    capture_length: end - start,
                    class,
                });
            }
        }
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

fn point_at(source: &[u8], byte: usize) -> Point {
    let prefix = &source[..byte];
    let row = prefix.iter().filter(|byte| **byte == b'\n').count();
    let column = prefix
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(byte, |newline| byte - newline - 1);

    Point::new(row, column)
}

#[cfg(test)]
mod tests {
    use super::{
        Language, LanguageHighlighter, ProjectedRecord, Projection, Styles, SyntaxClass,
        SyntaxHighlighter,
    };
    use crate::{
        document::{FileBody, Side},
        measurement::{Profile, Unobserved},
        parser::parse_patch,
    };
    use tree_sitter::{InputEdit, Point};

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
        let document =
            parse_patch(b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-old\n+fn added() {}\n")
                .expect("valid Rust diff");
        let FileBody::Unified(file) = &document.files().next().expect("first file").body else {
            panic!("fixture should contain a unified file");
        };
        let mut highlighter = SyntaxHighlighter::default();

        let highlights = highlighter.highlight_hunk(file, &file.hunks[0], &mut Unobserved);

        assert_eq!(highlights[1][0].class, SyntaxClass::Keyword);
        assert_eq!(highlights[1][0].span.start(), 0);
        assert_eq!(highlights[1][0].span.end(), 2);
    }

    #[test]
    fn context_uses_new_projection_syntax_when_old_context_is_a_comment() {
        let document = parse_patch(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1,3 +1,3 @@\n-/*\n+// before\n let value = 1;\n-*/\n+// after\n",
        )
        .expect("valid Rust diff with changing comment context");
        let FileBody::Unified(file) = &document.files().next().expect("first file").body else {
            panic!("fixture should contain a unified file");
        };
        let mut highlighter = SyntaxHighlighter::default();

        let highlights = highlighter.highlight_hunk(file, &file.hunks[0], &mut Unobserved);

        // architecture.md: matching languages use the new projection for context styling.
        assert!(
            highlights[2]
                .iter()
                .any(|span| span.class == SyntaxClass::Keyword)
        );
        assert!(
            highlights[2]
                .iter()
                .all(|span| span.class != SyntaxClass::Comment)
        );
        assert!(
            highlights[0]
                .iter()
                .any(|span| span.class == SyntaxClass::Comment)
        );
        assert!(
            highlights[3]
                .iter()
                .any(|span| span.class == SyntaxClass::Comment)
        );
    }

    #[test]
    fn incremental_new_projection_matches_a_full_query_pass() {
        let document = parse_patch(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n",
        )
        .expect("valid Rust diff");
        let FileBody::Unified(file) = &document.files().next().expect("first file").body else {
            panic!("fixture should contain a unified file");
        };
        let new_projection =
            Projection::from_hunk(&file.hunks[0], Side::New, Styles::ChangesAndContext);
        let mut incremental = SyntaxHighlighter::default();
        let incremental_spans = incremental.highlight_hunk(file, &file.hunks[0], &mut Unobserved);
        let mut full = SyntaxHighlighter::default();
        let mut full_spans = (0..file.hunks[0].records.len())
            .map(|_| Vec::new())
            .collect::<Vec<_>>();

        full.highlight_projection(
            Language::Rust,
            &new_projection,
            None,
            &mut full_spans,
            &mut Unobserved,
        );

        assert_eq!(incremental_spans[1], full_spans[1]);
        assert_eq!(incremental_spans[1][0].class, SyntaxClass::Keyword);
    }

    #[test]
    fn finds_the_first_record_overlapping_a_source_span() {
        let projection = Projection {
            source: "aaaa\nbbbb\ncccc\n".to_owned(),
            records: projected(&[0..4, 5..9, 10..14]),
        };

        assert_eq!(projection.first_overlapping_record(0), 0);
        assert_eq!(projection.first_overlapping_record(4), 1);
        assert_eq!(projection.first_overlapping_record(9), 2);
    }

    #[test]
    fn maps_only_records_overlapping_a_source_span() {
        let projection = Projection {
            source: "aaaa\nbbbb\ncccc\n".to_owned(),
            records: projected(&[0..4, 5..9, 10..14]),
        };
        let mut spans = vec![Vec::new(), Vec::new(), Vec::new()];

        projection.map_span(2, 12, SyntaxClass::Keyword, &mut spans);

        assert_eq!(spans[0][0].span.start(), 2);
        assert_eq!(spans[0][0].span.end(), 4);
        assert_eq!(spans[1][0].span.start(), 0);
        assert_eq!(spans[1][0].span.end(), 4);
        assert_eq!(spans[2][0].span.start(), 0);
        assert_eq!(spans[2][0].span.end(), 2);
    }

    #[test]
    fn does_not_map_a_source_span_in_a_record_separator() {
        let projection = Projection {
            source: "aaaa\nbbbb\n".to_owned(),
            records: projected(&[0..4, 5..9]),
        };
        let mut spans = vec![Vec::new(), Vec::new()];

        projection.map_span(4, 5, SyntaxClass::Keyword, &mut spans);

        assert!(spans.iter().all(Vec::is_empty));
    }

    #[test]
    fn context_stays_textual_when_the_paths_select_different_languages() {
        let document = parse_patch(
            b"--- a/source.py\n+++ b/source.rs\n@@ -1,2 +1,2 @@\n fn kept() {}\n-def old(): pass\n+fn new() {}\n",
        )
        .expect("valid diff renaming Python to Rust");
        let FileBody::Unified(file) = &document.files().next().expect("first file").body else {
            panic!("fixture should contain a unified file");
        };
        let mut highlighter = SyntaxHighlighter::default();

        let highlights = highlighter.highlight_hunk(file, &file.hunks[0], &mut Unobserved);

        // architecture.md: context remains textual when the paths select different languages.
        assert!(highlights[0].is_empty());
        assert!(
            highlights[1]
                .iter()
                .any(|span| span.class == SyntaxClass::Keyword)
        );
        assert!(
            highlights[2]
                .iter()
                .any(|span| span.class == SyntaxClass::Keyword)
        );
    }

    #[test]
    fn every_bundled_language_loads_its_grammar_and_highlight_query() {
        for language in Language::ALL {
            LanguageHighlighter::new(language)
                .unwrap_or_else(|error| panic!("{language:?} failed to load: {error}"));
        }
    }

    #[test]
    fn each_language_keeps_its_highlighter_in_its_own_slot() {
        for (position, language) in Language::ALL.into_iter().enumerate() {
            assert_eq!(language.slot(), position, "{language:?}");
        }
    }

    #[test]
    fn a_side_with_only_context_is_not_parsed() {
        let document = parse_patch(
            b"--- a/source.py\n+++ b/source.rs\n@@ -1 +1,2 @@\n fn kept() {}\n+fn new() {}\n",
        )
        .expect("valid diff renaming Python to Rust");
        let FileBody::Unified(file) = &document.files().next().expect("first file").body else {
            panic!("fixture should contain a unified file");
        };
        let mut profile = Profile::default();

        SyntaxHighlighter::default().highlight_hunk(file, &file.hunks[0], &mut profile);

        assert_eq!(profile.work().parse_calls, 1);
    }

    fn projected(ranges: &[std::ops::Range<usize>]) -> Vec<ProjectedRecord> {
        ranges
            .iter()
            .enumerate()
            .map(|(record_index, source)| ProjectedRecord {
                record_index,
                source: source.clone(),
            })
            .collect()
    }
}
