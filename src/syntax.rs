use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

use crate::document::{DiffFile, RecordKind, UnifiedDiffFile};

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
        match file {
            DiffFile::Metadata { lines } => (0..lines.len()).map(|_| Vec::new()).collect(),
            DiffFile::Unified(file) => {
                let mut rows = (0..file.metadata.len() + 2)
                    .map(|_| Vec::new())
                    .collect::<Vec<_>>();
                for hunk_index in 0..file.hunks.len() {
                    rows.push(Vec::new());
                    rows.extend(self.highlight_hunk(file, hunk_index));
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
        let Some(hunk) = file.hunks.get(hunk_index) else {
            return Vec::new();
        };
        let mut spans = (0..hunk.records.len())
            .map(|_| Vec::new())
            .collect::<Vec<_>>();
        let old_language = Language::for_path(&file.old_path);
        let new_language = Language::for_path(&file.new_path);

        if let Some(language) = old_language {
            self.highlight_side(file, hunk_index, Side::Old, language, &mut spans);
        }
        if let Some(language) = new_language {
            self.highlight_side(file, hunk_index, Side::New, language, &mut spans);
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

    fn highlight_side(
        &mut self,
        file: &UnifiedDiffFile,
        hunk_index: usize,
        side: Side,
        language: Language,
        spans: &mut [Vec<SyntaxSpan>],
    ) {
        let projection = Projection::from_hunk(file, hunk_index, side);
        let Some(projection) = projection else {
            return;
        };
        let Some(highlighter) = self.language(language) else {
            return;
        };
        let LanguageHighlighter {
            configuration,
            highlighter,
            ..
        } = highlighter;
        let Ok(events) = highlighter.highlight(configuration, &projection.source, None, |_| None)
        else {
            return;
        };
        let mut classes = Vec::new();
        for event in events.flatten() {
            match event {
                HighlightEvent::HighlightStart(highlight) => {
                    classes.push(SyntaxClass::from_index(highlight.0))
                }
                HighlightEvent::HighlightEnd => {
                    classes.pop();
                }
                HighlightEvent::Source { start, end } => {
                    if let Some(class) = classes.last().copied() {
                        projection.map_span(start, end, class, spans);
                    }
                }
            }
        }
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
    configuration: HighlightConfiguration,
    highlighter: Highlighter,
}

impl LanguageHighlighter {
    fn new(language: Language) -> Option<Self> {
        let (grammar, name, highlights, injections, locals) = match language {
            Language::Rust => (
                tree_sitter_rust::LANGUAGE.into(),
                "rust",
                tree_sitter_rust::HIGHLIGHTS_QUERY,
                tree_sitter_rust::INJECTIONS_QUERY,
                "",
            ),
            Language::JavaScript => (
                tree_sitter_javascript::LANGUAGE.into(),
                "javascript",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::INJECTIONS_QUERY,
                tree_sitter_javascript::LOCALS_QUERY,
            ),
            Language::Python => (
                tree_sitter_python::LANGUAGE.into(),
                "python",
                tree_sitter_python::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            Language::Go => (
                tree_sitter_go::LANGUAGE.into(),
                "go",
                tree_sitter_go::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            Language::C => (
                tree_sitter_c::LANGUAGE.into(),
                "c",
                tree_sitter_c::HIGHLIGHT_QUERY,
                "",
                "",
            ),
        };
        let mut configuration =
            HighlightConfiguration::new(grammar, name, highlights, injections, locals).ok()?;
        configuration.configure(&[
            "keyword", "string", "comment", "number", "function", "type", "constant", "operator",
            "property", "variable",
        ]);
        Some(Self {
            language,
            configuration,
            highlighter: Highlighter::new(),
        })
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
        for (record_index, range) in &self.records {
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
}

impl SyntaxClass {
    fn from_index(index: usize) -> Self {
        [
            Self::Keyword,
            Self::String,
            Self::Comment,
            Self::Number,
            Self::Function,
            Self::Type,
            Self::Constant,
            Self::Operator,
            Self::Property,
            Self::Variable,
        ]
        .get(index)
        .copied()
        .unwrap_or(Self::Variable)
    }
}

#[cfg(test)]
mod tests {
    use super::{SyntaxClass, SyntaxHighlighter};
    use crate::{document::DiffFile, parser::parse_unified_diff};

    #[test]
    fn highlights_added_rust_keywords_in_the_new_side() {
        let document = parse_unified_diff(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-old\n+fn added() {}\n",
        )
        .expect("valid Rust diff");
        let DiffFile::Unified(file) = &document.files[0] else {
            panic!("fixture should contain a unified file");
        };
        let mut highlighter = SyntaxHighlighter::default();

        let highlights = highlighter.highlight_hunk(file, 0);

        assert_eq!(highlights[1][0].class, SyntaxClass::Keyword);
        assert_eq!(highlights[1][0].start, 0);
        assert_eq!(highlights[1][0].end, 2);
    }
}
