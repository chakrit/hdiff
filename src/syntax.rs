use tree_sitter::{InputEdit, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::{
    document::{DiffFile, RecordKind, UnifiedDiffFile},
    measurement::{Observer, Stage, Unobserved, Work},
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

#[derive(Debug, PartialEq, Eq)]
pub struct SyntaxSpan {
    pub start: usize,
    pub end: usize,
    pub class: SyntaxClass,
}

#[derive(Default)]
pub struct SyntaxHighlighter {
    languages: Vec<LanguageHighlighter>,
}

impl SyntaxHighlighter {
    pub fn highlight_file(&mut self, file: &DiffFile) -> Vec<Vec<SyntaxSpan>> {
        self.highlight_file_observed(file, &mut Unobserved)
    }

    pub fn highlight_file_observed(
        &mut self,
        file: &DiffFile,
        observer: &mut impl Observer,
    ) -> Vec<Vec<SyntaxSpan>> {
        match file {
            DiffFile::Metadata { lines } => (0..lines.len()).map(|_| Vec::new()).collect(),
            DiffFile::Unified(file) => {
                let mut rows = (0..file.metadata.len() + 2)
                    .map(|_| Vec::new())
                    .collect::<Vec<_>>();
                for hunk_index in 0..file.hunks.len() {
                    rows.push(Vec::new());
                    rows.extend(self.highlight_hunk_observed(file, hunk_index, observer));
                }
                rows
            }
        }
    }

    pub fn highlight_hunk(
        &mut self,
        file: &UnifiedDiffFile,
        hunk_index: usize,
    ) -> Vec<Vec<SyntaxSpan>> {
        self.highlight_hunk_observed(file, hunk_index, &mut Unobserved)
    }

    fn highlight_hunk_observed(
        &mut self,
        file: &UnifiedDiffFile,
        hunk_index: usize,
        observer: &mut impl Observer,
    ) -> Vec<Vec<SyntaxSpan>> {
        let Some(hunk) = file.hunks.get(hunk_index) else {
            return Vec::new();
        };
        observer.count(Work::Hunk);
        let mut spans = (0..hunk.records.len())
            .map(|_| Vec::new())
            .collect::<Vec<_>>();
        let old_language = Language::for_path(&file.old_path);
        let new_language = Language::for_path(&file.new_path);
        let (old_projection, new_projection) = observer.measure(Stage::Projection, |observer| {
            let old = Projection::from_hunk(file, hunk_index, Side::Old);
            let new = Projection::from_hunk(file, hunk_index, Side::New);
            for projection in old.iter().chain(&new) {
                observer.count(Work::ProjectedBytes(projection.source.len()));
            }
            (old, new)
        });

        match (
            old_language,
            new_language,
            old_projection.as_ref(),
            new_projection.as_ref(),
        ) {
            (Some(language), Some(other), Some(old), Some(new)) if language == other => {
                self.highlight_pair(language, old, new, &mut spans, observer);
            }
            _ => {
                if let (Some(language), Some(projection)) = (old_language, old_projection.as_ref())
                {
                    self.highlight_projection(language, projection, &mut spans, observer);
                }
                if let (Some(language), Some(projection)) = (new_language, new_projection.as_ref())
                {
                    self.highlight_projection(language, projection, &mut spans, observer);
                }
            }
        }

        if old_language != new_language {
            for (record, record_spans) in hunk.records.iter().zip(&mut spans) {
                if record.kind == RecordKind::Context {
                    record_spans.clear();
                }
            }
        }

        spans
    }

    fn highlight_pair(
        &mut self,
        language: Language,
        old: &Projection,
        new: &Projection,
        spans: &mut [Vec<SyntaxSpan>],
        observer: &mut impl Observer,
    ) {
        let Some(highlighter) = observer.measure(Stage::Language, |_| self.language(language))
        else {
            return;
        };
        let Some(mut tree) = observer.measure(Stage::Parse, |observer| {
            observer.count(Work::Parse);
            highlighter.parse(&old.source, None)
        }) else {
            return;
        };
        observer.measure(Stage::Query, |observer| {
            highlighter.highlight(old, &tree, spans, observer)
        });

        let Some(tree) = observer.measure(Stage::Parse, |observer| {
            tree.edit(&old.edit_to(new));
            observer.count(Work::Parse);
            highlighter.parse(&new.source, Some(&tree))
        }) else {
            return;
        };
        observer.measure(Stage::Query, |observer| {
            highlighter.highlight(new, &tree, spans, observer)
        });
    }

    fn highlight_projection(
        &mut self,
        language: Language,
        projection: &Projection,
        spans: &mut [Vec<SyntaxSpan>],
        observer: &mut impl Observer,
    ) {
        let Some(highlighter) = observer.measure(Stage::Language, |_| self.language(language))
        else {
            return;
        };
        let Some(tree) = observer.measure(Stage::Parse, |observer| {
            observer.count(Work::Parse);
            highlighter.parse(&projection.source, None)
        }) else {
            return;
        };
        observer.measure(Stage::Query, |observer| {
            highlighter.highlight(projection, &tree, spans, observer)
        });
    }

    fn language(&mut self, language: Language) -> Option<&mut LanguageHighlighter> {
        if let Some(index) = self
            .languages
            .iter()
            .position(|entry| entry.language == language)
        {
            return self.languages.get_mut(index);
        }
        self.languages.push(LanguageHighlighter::new(language)?);
        self.languages.last_mut()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Old,
    New,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    JavaScript,
    Python,
    Go,
    C,
}

impl Language {
    fn for_path(path: &str) -> Option<Self> {
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

struct LanguageHighlighter {
    language: Language,
    parser: Parser,
    query: Query,
    cursor: QueryCursor,
}

impl LanguageHighlighter {
    fn new(language: Language) -> Option<Self> {
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
        parser.set_language(&grammar).ok()?;

        Some(Self {
            language,
            parser,
            query: Query::new(&grammar, highlights).ok()?,
            cursor: QueryCursor::new(),
        })
    }

    fn parse(&mut self, source: &[u8], old_tree: Option<&Tree>) -> Option<Tree> {
        self.parser.parse(source, old_tree)
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
                .captures(&self.query, tree.root_node(), projection.source.as_slice());

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

struct Projection {
    source: Vec<u8>,
    records: Vec<(usize, std::ops::Range<usize>)>,
}

impl Projection {
    fn from_hunk(file: &UnifiedDiffFile, hunk_index: usize, side: Side) -> Option<Self> {
        let hunk = file.hunks.get(hunk_index)?;
        let mut source = Vec::new();
        let mut records = Vec::new();
        for (record_index, record) in hunk.records.iter().enumerate() {
            let belongs = match side {
                Side::Old => record.kind != RecordKind::Addition && record.kind != RecordKind::Raw,
                Side::New => record.kind != RecordKind::Deletion && record.kind != RecordKind::Raw,
            };
            if belongs {
                let start = source.len();
                source.extend_from_slice(record.payload.structural_text.as_bytes());
                records.push((record_index, start..source.len()));
                source.push(b'\n');
            }
        }
        (!source.is_empty()).then_some(Self { source, records })
    }

    fn map_span(
        &self,
        start: usize,
        end: usize,
        class: SyntaxClass,
        spans: &mut [Vec<SyntaxSpan>],
    ) {
        let first_record = self.first_overlapping_record(start);
        for (record_index, range) in self
            .records
            .iter()
            .skip(first_record)
            .take_while(|(_, range)| range.start < end)
        {
            let mapped_start = start.max(range.start);
            let mapped_end = end.min(range.end);
            if mapped_start < mapped_end {
                spans[*record_index].push(SyntaxSpan {
                    start: mapped_start - range.start,
                    end: mapped_end - range.start,
                    class,
                });
            }
        }
    }

    fn edit_to(&self, newer: &Self) -> InputEdit {
        let common_prefix = self
            .source
            .iter()
            .zip(&newer.source)
            .take_while(|(old, new)| old == new)
            .count();
        let shared_limit = self.source.len().min(newer.source.len()) - common_prefix;
        let common_suffix = self
            .source
            .iter()
            .rev()
            .zip(newer.source.iter().rev())
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
            start_position: point_at(&self.source, start_byte),
            old_end_position: point_at(&self.source, old_end_byte),
            new_end_position: point_at(&newer.source, new_end_byte),
        }
    }

    fn first_overlapping_record(&self, start: usize) -> usize {
        self.records
            .partition_point(|(_, range)| range.end <= start)
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

#[cfg(test)]
mod tests {
    use super::{Projection, SyntaxClass, SyntaxHighlighter};
    use crate::{document::DiffFile, parser::parse_unified_diff};
    use tree_sitter::{InputEdit, Point};

    #[test]
    fn describes_the_changed_projection_range_for_incremental_parsing() {
        let old = Projection {
            source: b"let old = 1;\n".to_vec(),
            records: Vec::new(),
        };
        let new = Projection {
            source: b"let new = 2;\n".to_vec(),
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
        let document = parse_unified_diff(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-old\n+fn added() {}\n",
        )
        .expect("valid Rust diff");
        let DiffFile::Unified(file) = document.file(0).expect("first file") else {
            panic!("fixture should contain a unified file");
        };
        let mut highlighter = SyntaxHighlighter::default();

        let highlights = highlighter.highlight_hunk(file, 0);

        assert_eq!(highlights[1][0].class, SyntaxClass::Keyword);
        assert_eq!(highlights[1][0].start, 0);
        assert_eq!(highlights[1][0].end, 2);
    }

    #[test]
    fn incremental_new_projection_matches_a_full_query_pass() {
        let document = parse_unified_diff(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n",
        )
        .expect("valid Rust diff");
        let DiffFile::Unified(file) = document.file(0).expect("first file") else {
            panic!("fixture should contain a unified file");
        };
        let new_projection =
            Projection::from_hunk(file, 0, super::Side::New).expect("new projection");
        let mut incremental = SyntaxHighlighter::default();
        let incremental_spans = incremental.highlight_hunk(file, 0);
        let mut full = SyntaxHighlighter::default();
        let mut full_spans = (0..file.hunks[0].records.len())
            .map(|_| Vec::new())
            .collect::<Vec<_>>();

        full.highlight_projection(
            super::Language::Rust,
            &new_projection,
            &mut full_spans,
            &mut crate::measurement::Unobserved,
        );

        assert_eq!(incremental_spans[1], full_spans[1]);
        assert_eq!(incremental_spans[1][0].class, SyntaxClass::Keyword);
    }

    #[test]
    fn finds_the_first_record_overlapping_a_source_span() {
        let projection = Projection {
            source: Vec::new(),
            records: vec![(0, 0..4), (1, 5..9), (2, 10..14)],
        };

        assert_eq!(projection.first_overlapping_record(0), 0);
        assert_eq!(projection.first_overlapping_record(4), 1);
        assert_eq!(projection.first_overlapping_record(9), 2);
    }

    #[test]
    fn maps_only_records_overlapping_a_source_span() {
        let projection = Projection {
            source: Vec::new(),
            records: vec![(0, 0..4), (1, 5..9), (2, 10..14)],
        };
        let mut spans = vec![Vec::new(), Vec::new(), Vec::new()];

        projection.map_span(2, 12, SyntaxClass::Keyword, &mut spans);

        assert_eq!(spans[0][0].start, 2);
        assert_eq!(spans[0][0].end, 4);
        assert_eq!(spans[1][0].start, 0);
        assert_eq!(spans[1][0].end, 4);
        assert_eq!(spans[2][0].start, 0);
        assert_eq!(spans[2][0].end, 2);
    }

    #[test]
    fn does_not_map_a_source_span_in_a_record_separator() {
        let projection = Projection {
            source: Vec::new(),
            records: vec![(0, 0..4), (1, 5..9)],
        };
        let mut spans = vec![Vec::new(), Vec::new()];

        projection.map_span(4, 5, SyntaxClass::Keyword, &mut spans);

        assert!(spans.iter().all(Vec::is_empty));
    }
}
