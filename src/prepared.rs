use std::num::NonZeroUsize;

use crate::{
    detail::character_details,
    document::{DiffDocument, FileView, RecordMap},
    file_tree::{FileTree, ListedFile, ListedTree, TreeRowIndex},
    measurement::{Observer, Stage, Unobserved},
    page_rows::{LineIndex, PageRows},
    preferences::DiffGranularity,
    styling::{StyledRow, StyledRows},
    syntax::SyntaxHighlighter,
};

/// A document ready to draw: at least one page, and the file list.
pub struct PreparedDocument {
    first: PreparedPage,
    rest: Box<[PreparedPage]>,
    file_tree: FileTree,
}

/// What one selection shows: a file occurrence with its surrounding text, or all the
/// text of a document that holds no file.
#[derive(Debug, PartialEq, Eq)]
pub struct PreparedPage {
    rows: StyledRows,
    kind: PageKind,
}

/// What a page shows: a listed file, or the text of a document that lists no file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    File { row: TreeRowIndex },
    Text,
}

impl PreparedDocument {
    pub fn prepare(document: &DiffDocument) -> Self {
        Self::prepare_observed(document, &mut Unobserved)
    }

    pub fn prepare_observed(document: &DiffDocument, observer: &mut impl Observer) -> Self {
        let ListedTree {
            tree: file_tree,
            files,
        } = observer.measure(Stage::Labels, |_| ListedTree::build(document.file_views()));

        let (first, rest) = match files.as_ref() {
            [] => (PreparedPage::text(document, observer), Vec::new()),
            [first, rest @ ..] => {
                let mut syntax = SyntaxHighlighter::default();
                let mut prepare = |listed: &ListedFile| {
                    PreparedPage::file(listed.view, listed.row, &mut syntax, observer)
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
    pub fn page(&self, index: PageIndex) -> &PreparedPage {
        match index.0.checked_sub(1) {
            None => &self.first,
            Some(rest_index) => &self.rest[rest_index],
        }
    }

    pub fn page_count(&self) -> PageCount {
        PageCount(NonZeroUsize::MIN.saturating_add(self.rest.len()))
    }

    pub fn file_count(&self) -> usize {
        self.pages()
            .filter(|page| matches!(page.kind, PageKind::File { .. }))
            .count()
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

/// The position of one page in a prepared document's page order, which is the file
/// list's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageIndex(usize);

impl PageIndex {
    pub const FIRST: Self = Self(0);

    /// The page after this one among `pages`, wrapping to the first.
    pub fn next(self, pages: PageCount) -> Self {
        Self((self.0 + 1) % pages.0)
    }

    /// The page before this one among `pages`, wrapping to the last.
    pub fn previous(self, pages: PageCount) -> Self {
        let last = pages.0.get() - 1;

        Self(self.0.checked_sub(1).unwrap_or(last))
    }
}

/// How many pages a prepared document holds; only the document issues one, so page steps
/// stay among its pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageCount(NonZeroUsize);

impl PageCount {
    /// `count` pages, for tests that step through pages without a document.
    #[cfg(test)]
    pub const fn of_test(count: usize) -> Self {
        Self(NonZeroUsize::new(count).expect("a test page count is nonzero"))
    }
}

impl PreparedPage {
    fn file(
        file: FileView<'_>,
        row: TreeRowIndex,
        syntax: &mut SyntaxHighlighter,
        observer: &mut impl Observer,
    ) -> Self {
        let rows = observer.measure(Stage::Layout, |_| PageRows::of_file(&file));
        let syntax = observer.measure(Stage::Syntax, |observer| {
            syntax.highlight_file(file.file, observer)
        });
        let details = observer.measure(Stage::Detail, |_| character_details(&rows));

        Self {
            rows: StyledRows::new(rows, &syntax, &details),
            kind: PageKind::File { row },
        }
    }

    /// Surrounding text has no syntax or character detail.
    fn text(document: &DiffDocument, observer: &mut impl Observer) -> Self {
        let rows = observer.measure(Stage::Layout, |_| PageRows::of_text(document.text()));
        let details = rows.map_lines(|_, _| None);

        Self {
            rows: StyledRows::new(rows, &RecordMap::empty(), &details),
            kind: PageKind::Text,
        }
    }

    pub fn kind(&self) -> PageKind {
        self.kind
    }

    pub fn rows(&self) -> &PageRows {
        self.rows.rows()
    }

    /// Row `index` with the styles `granularity` presents.
    pub fn row(&self, index: LineIndex, granularity: DiffGranularity) -> StyledRow<'_> {
        self.rows.row(index, granularity)
    }

    fn record_count(&self) -> usize {
        self.rows.rows().record_count()
    }
}

#[cfg(test)]
mod tests {
    use super::{PageIndex, PreparedDocument};
    use crate::{
        parser::parse_patch,
        preferences::DiffGranularity,
        styling::{DetailStyle, TextStyle},
    };

    #[test]
    fn retains_surrounding_text_with_separate_repeated_file_views() {
        let patch = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
        let input = format!("first message\n{patch}second message\n{patch}trailer\n");
        let document = parse_patch(input.as_bytes()).expect("diffs with messages");
        let prepared = PreparedDocument::prepare(&document);

        assert_eq!(prepared.file_count(), 2);
        assert_eq!(prepared.record_count(), 4);
        for (file, expected) in prepared.pages().zip([
            format!("first message\n{patch}"),
            format!("second message\n{patch}trailer\n"),
        ]) {
            let rendered: String = file
                .rows()
                .lines()
                .map(|(_, line)| format!("{}{}\n", line.kind().marker(), line.payload()))
                .collect();
            assert_eq!(rendered, expected);
            assert!(
                file.row(file.rows().line_index(0), DiffGranularity::Line)
                    .segments()
                    .all(|(_, style)| style == TextStyle::Plain),
                "message is not source code"
            );
            assert!(file.rows().line_indices().skip(4).any(|row| {
                file.row(row, DiffGranularity::Line)
                    .segments()
                    .any(|(_, style)| matches!(style, TextStyle::Syntax(_)))
            }));
        }
    }

    #[test]
    fn diagnostic_observation_preserves_every_prepared_rendering_component() {
        let document = parse_patch(include_bytes!(
            "../tests/fixtures/mixed-language-layout.patch"
        ))
        .expect("mixed language diff");
        let ordinary = PreparedDocument::prepare(&document);
        let mut profile = crate::measurement::Profile::default();
        let observed = PreparedDocument::prepare_observed(&document, &mut profile);

        assert_eq!(ordinary.file_tree(), observed.file_tree());
        assert_eq!(ordinary.record_count(), observed.record_count());
        for (ordinary, observed) in ordinary.pages().zip(observed.pages()) {
            assert_eq!(ordinary, observed);
        }
    }

    #[test]
    fn prepares_every_file_and_record() {
        let document = parse_patch(include_bytes!("../tests/fixtures/git-multiline.patch"))
            .expect("valid fixture");

        let prepared = PreparedDocument::prepare(&document);

        assert_eq!(prepared.file_count(), 2);
        assert_eq!(prepared.record_count(), 7);
    }

    #[test]
    fn prepares_character_detail_before_terminal_rendering() {
        let document =
            parse_patch(b"--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-hello\n+hallo\n")
                .expect("valid diff");

        let prepared = PreparedDocument::prepare(&document);

        let file = prepared.page(PageIndex::FIRST);
        assert!(file.rows().line_indices().any(|row| {
            file.row(row, DiffGranularity::Character)
                .segments()
                .any(|(_, style)| matches!(style, TextStyle::Detail(_, DetailStyle::Changed)))
        }));
    }
}
