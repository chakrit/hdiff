use crate::{
    detail::{DetailSpan, detail_rows},
    document::{DiffDocument, DiffFile},
    interaction::{DiffGranularity, DiffLayout, Interaction, ViewPreferences, Viewport},
    layout::{FileListRow, Layout, layout},
    syntax::{SyntaxHighlighter, SyntaxSpan},
};

pub struct PreparedDocument {
    files: Vec<PreparedFile>,
    file_labels: Vec<String>,
}

pub struct PreparedFile {
    pub layout: Layout,
    syntax: Vec<Vec<SyntaxSpan>>,
    line_details: Vec<Vec<DetailSpan>>,
    character_details: Vec<Vec<DetailSpan>>,
}

impl PreparedDocument {
    pub fn prepare(document: &DiffDocument) -> Self {
        let mut syntax = SyntaxHighlighter::default();
        let file_labels = file_labels(document);
        let files = document
            .files
            .iter()
            .enumerate()
            .map(|(selected_file, file)| {
                PreparedFile::prepare(document, file, selected_file, &mut syntax)
            })
            .collect();

        Self { files, file_labels }
    }

    pub fn file(&self, index: usize) -> Option<&PreparedFile> {
        self.files.get(index)
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn record_count(&self) -> usize {
        self.files.iter().map(PreparedFile::record_count).sum()
    }

    pub fn file_rows(&self, selected_file: usize) -> Vec<FileListRow> {
        self.file_labels
            .iter()
            .enumerate()
            .map(|(index, label)| FileListRow {
                label: label.clone(),
                selected: index == selected_file,
            })
            .collect()
    }
}

impl PreparedFile {
    fn prepare(
        document: &DiffDocument,
        file: &DiffFile,
        selected_file: usize,
        syntax: &mut SyntaxHighlighter,
    ) -> Self {
        let interaction = Interaction {
            selected_file,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 0,
                height: 0,
            },
        };
        let layout = layout(document, &interaction);
        let syntax = syntax.highlight_file(file);
        let line_details = detail_rows(&layout, DiffGranularity::Line);
        let character_details = detail_rows(&layout, DiffGranularity::Character);

        Self {
            layout,
            syntax,
            line_details,
            character_details,
        }
    }

    pub fn syntax(&self) -> &[Vec<SyntaxSpan>] {
        &self.syntax
    }

    pub fn details(&self, granularity: DiffGranularity) -> &[Vec<DetailSpan>] {
        match granularity {
            DiffGranularity::Line => &self.line_details,
            DiffGranularity::Character => &self.character_details,
        }
    }

    fn record_count(&self) -> usize {
        self.layout
            .line_kinds
            .iter()
            .filter(|kind| matches!(kind, crate::render::RenderedLineKind::Record(_)))
            .count()
    }
}

fn file_labels(document: &DiffDocument) -> Vec<String> {
    let interaction = Interaction {
        selected_file: 0,
        preferences: ViewPreferences::line(DiffLayout::Unified),
        viewport: Viewport {
            offset: 0,
            height: 0,
        },
    };

    layout(document, &interaction)
        .files
        .into_iter()
        .map(|row| row.label)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::PreparedDocument;
    use crate::{interaction::DiffGranularity, parser::parse_unified_diff};

    #[test]
    fn prepares_layout_syntax_and_character_detail_for_every_file() {
        let document = parse_unified_diff(include_bytes!("../tests/fixtures/git-multiline.patch"))
            .expect("valid fixture");

        let prepared = PreparedDocument::prepare(&document);

        assert_eq!(prepared.file_count(), 2);
        assert_eq!(prepared.record_count(), 7);
        let first = prepared.file(0).expect("first file");

        assert_eq!(first.syntax().len(), first.layout.diff_lines.len());
        assert_eq!(
            first.details(DiffGranularity::Character).len(),
            first.layout.diff_lines.len()
        );
    }

    #[test]
    fn prepares_character_detail_before_terminal_rendering() {
        let document =
            parse_unified_diff(b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-hello\n+hallo\n")
                .expect("valid diff");

        let prepared = PreparedDocument::prepare(&document);

        assert!(
            prepared
                .file(0)
                .expect("prepared file")
                .details(DiffGranularity::Character)
                .iter()
                .any(|spans| !spans.is_empty())
        );
    }
}
