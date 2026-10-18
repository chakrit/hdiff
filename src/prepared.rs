use std::num::NonZeroUsize;

use crate::{
    detail::detail_rows,
    document::{DiffDocument, FileView},
    file_tree::{FileTree, ListedFile},
    interaction::DiffGranularity,
    layout::{Layout, file_layout, text_layout},
    measurement::{Observer, Stage, Unobserved},
    render::RenderedLineKind,
    styling::{StyledLayout, StyledRow},
    syntax::{SyntaxHighlighter, SyntaxSpan},
};

/// A document ready to draw: at least one page, and the file list.
pub struct PreparedDocument {
    first: PreparedPage,
    rest: Box<[PreparedPage]>,
    file_tree: FileTree,
}

/// What one selection shows: a file occurrence with its surrounding text, or all the
/// text of a document that holds no file.
pub struct PreparedPage {
    rows: StyledLayout,
    list_row: Option<usize>,
}

impl PreparedDocument {
    pub fn prepare(document: &DiffDocument) -> Self {
        Self::prepare_observed(document, &mut Unobserved)
    }

    pub fn prepare_observed(document: &DiffDocument, observer: &mut impl Observer) -> Self {
        let views = document.file_views().collect::<Vec<_>>();
        let file_tree = observer.measure(Stage::Labels, |_| FileTree::build(&views));

        let (first, rest) = match file_tree.files() {
            [] => (PreparedPage::text(document, observer), Vec::new()),
            [first, rest @ ..] => {
                let mut syntax = SyntaxHighlighter::default();
                let mut prepare = |listed: &ListedFile| {
                    PreparedPage::file(views[listed.view], listed.row, &mut syntax, observer)
                };
                (prepare(first), rest.iter().map(prepare).collect())
            }
        };

        Self {
            first,
            rest: rest.into_boxed_slice(),
            file_tree,
        }
    }

    /// The page at `index`.
    ///
    /// Panics when `index` is not below [`page_count`](Self::page_count).
    pub fn page(&self, index: usize) -> &PreparedPage {
        match index.checked_sub(1) {
            None => &self.first,
            Some(rest_index) => &self.rest[rest_index],
        }
    }

    pub fn page_count(&self) -> NonZeroUsize {
        NonZeroUsize::MIN.saturating_add(self.rest.len())
    }

    pub fn file_count(&self) -> usize {
        self.file_tree.files().len()
    }

    pub fn record_count(&self) -> usize {
        self.pages().map(PreparedPage::record_count).sum()
    }

    pub fn file_tree(&self) -> &FileTree {
        &self.file_tree
    }

    fn pages(&self) -> impl Iterator<Item = &PreparedPage> {
        std::iter::once(&self.first).chain(&self.rest)
    }
}

impl PreparedPage {
    fn file(
        file: FileView<'_>,
        list_row: usize,
        syntax: &mut SyntaxHighlighter,
        observer: &mut impl Observer,
    ) -> Self {
        let layout = observer.measure(Stage::Layout, |_| file_layout(&file));
        let syntax: Vec<Vec<SyntaxSpan>> = observer.measure(Stage::Syntax, |observer| {
            let leading = std::iter::repeat_with(Vec::new).take(file.leading.len());
            let highlighted = syntax.highlight_file_observed(file.file, observer);
            let trailing = std::iter::repeat_with(Vec::new).take(file.trailing.len());

            leading.chain(highlighted).chain(trailing).collect()
        });
        let character_details = observer.measure(Stage::Detail, |_| {
            detail_rows(&layout, DiffGranularity::Character)
        });

        Self {
            rows: StyledLayout::new(layout, &syntax, &character_details),
            list_row: Some(list_row),
        }
    }

    /// Surrounding text has no syntax or character detail.
    fn text(document: &DiffDocument, observer: &mut impl Observer) -> Self {
        let layout = observer.measure(Stage::Layout, |_| text_layout(document.text()));
        let row_count = layout.diff_lines.len();
        let syntax = std::iter::repeat_with(Vec::new)
            .take(row_count)
            .collect::<Vec<_>>();
        let details = std::iter::repeat_with(Vec::new)
            .take(row_count)
            .collect::<Vec<_>>();

        Self {
            rows: StyledLayout::new(layout, &syntax, &details),
            list_row: None,
        }
    }

    /// The page's file row in the [file tree](FileTree); a text page lists no file.
    pub fn list_row(&self) -> Option<usize> {
        self.list_row
    }

    pub fn layout(&self) -> &Layout {
        self.rows.layout()
    }

    pub fn row(&self, index: usize, granularity: DiffGranularity) -> StyledRow<'_> {
        self.rows.row(index, granularity)
    }

    fn record_count(&self) -> usize {
        self.layout()
            .diff_lines
            .iter()
            .filter(|line| matches!(line.kind, RenderedLineKind::Record(_)))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::PreparedDocument;
    use crate::{interaction::DiffGranularity, parser::parse_unified_diff, styling::TextStyle};

    #[test]
    fn retains_surrounding_text_with_separate_repeated_file_views() {
        let patch = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
        let input = format!("first message\n{patch}second message\n{patch}trailer\n");
        let document = parse_unified_diff(input.as_bytes()).expect("diffs with messages");
        let prepared = PreparedDocument::prepare(&document);

        assert_eq!(prepared.file_count(), 2);
        assert_eq!(prepared.record_count(), 4);
        for (index, expected) in [
            format!("first message\n{patch}"),
            format!("second message\n{patch}trailer\n"),
        ]
        .iter()
        .enumerate()
        {
            let file = prepared.page(index);
            let rendered: Vec<u8> = file
                .layout()
                .diff_lines
                .iter()
                .flat_map(|line| line.bytes.iter().copied())
                .collect();
            assert_eq!(rendered, expected.as_bytes());
            assert!(
                file.row(0, DiffGranularity::Line)
                    .segments()
                    .all(|(_, style)| style == TextStyle::Plain),
                "message is not source code"
            );
            assert!((4..file.layout().diff_lines.len()).any(|row| {
                file.row(row, DiffGranularity::Line)
                    .segments()
                    .any(|(_, style)| matches!(style, TextStyle::Syntax(_)))
            }));
        }
    }

    #[test]
    fn diagnostic_observation_preserves_every_prepared_rendering_component() {
        let document = parse_unified_diff(include_bytes!(
            "../tests/fixtures/mixed-language-layout.patch"
        ))
        .expect("mixed language diff");
        let ordinary = PreparedDocument::prepare(&document);
        let mut profile = crate::measurement::Profile::default();
        let observed = PreparedDocument::prepare_observed(&document, &mut profile);

        assert_eq!(ordinary.file_tree(), observed.file_tree());
        assert_eq!(ordinary.record_count(), observed.record_count());
        for (ordinary, observed) in ordinary.pages().zip(observed.pages()) {
            assert_eq!(ordinary.rows, observed.rows);
        }
    }

    #[test]
    fn prepares_every_file_and_record() {
        let document = parse_unified_diff(include_bytes!("../tests/fixtures/git-multiline.patch"))
            .expect("valid fixture");

        let prepared = PreparedDocument::prepare(&document);

        assert_eq!(prepared.file_count(), 2);
        assert_eq!(prepared.record_count(), 7);
    }

    #[test]
    fn prepares_character_detail_before_terminal_rendering() {
        let document =
            parse_unified_diff(b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-hello\n+hallo\n")
                .expect("valid diff");

        let prepared = PreparedDocument::prepare(&document);

        let file = prepared.page(0);
        assert!((0..file.layout().diff_lines.len()).any(|row| {
            file.row(row, DiffGranularity::Character)
                .segments()
                .any(|(_, style)| style == TextStyle::Detail)
        }));
    }
}
