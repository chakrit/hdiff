use std::num::NonZeroUsize;

use crate::{
    document::{DiffDocument, DiffFiles, FileView},
    file_tree::{FileSelection, FileTree, ListedFile, ListedTree, TreeRowIndex},
    measurement::{Observer, Stage, Unobserved},
    non_empty::NonEmpty,
    page_rows::{LineIndex, PageRows},
    preferences::DiffGranularity,
    source_text::SourceLine,
    styling::{StyledRow, StyledRows},
    syntax::SyntaxHighlighter,
};

/// A document ready to draw: one page per listed file with the file list, or one page of
/// the text of a document that lists no file. Rules:
/// docs/spec/architecture.md#core-model.
pub enum PreparedDocument {
    Files {
        pages: NonEmpty<FilePage>,
        file_tree: FileTree,
    },
    Text(PreparedPage),
}

/// A file occurrence's page, with the file-list row that selects it.
pub struct FilePage {
    page: PreparedPage,
    row: TreeRowIndex,
}

/// What one selection shows: a file occurrence with its surrounding text, or all the
/// text of a document that holds no file.
#[derive(Debug, PartialEq, Eq)]
pub struct PreparedPage {
    rows: StyledRows,
}

impl PreparedDocument {
    /// `document` ready to draw.
    pub fn prepare(document: &DiffDocument) -> Self {
        Self::prepare_observed(document, &mut Unobserved)
    }

    /// `document` ready to draw, with each stage and unit of work reported to
    /// `observer`.
    pub fn prepare_observed(document: &DiffDocument, observer: &mut impl Observer) -> Self {
        match document {
            DiffDocument::Files(files) => Self::of_files(files, observer),
            DiffDocument::Text(text) => Self::of_text(text, observer),
        }
    }

    /// The page at `index`.
    ///
    /// Panics when `index` is not below [`page_count`](Self::page_count).
    pub fn page(&self, index: PageIndex) -> &PreparedPage {
        match self {
            Self::Files { pages, .. } => &pages[index.0].page,
            Self::Text(page) => {
                assert_eq!(index, PageIndex::FIRST, "a text document has one page");
                page
            }
        }
    }

    /// How many pages the document holds: one per listed file, or the one text page.
    pub fn page_count(&self) -> PageCount {
        match self {
            Self::Files { pages, .. } => PageCount(pages.len()),
            Self::Text(_) => PageCount(NonZeroUsize::MIN),
        }
    }

    /// How many file occurrences the document lists.
    pub fn file_count(&self) -> usize {
        match self {
            Self::Files { pages, .. } => pages.len().get(),
            Self::Text(_) => 0,
        }
    }

    /// How many hunk records every page holds together.
    pub fn record_count(&self) -> usize {
        match self {
            Self::Files { pages, .. } => pages.iter().map(|file| file.page.record_count()).sum(),
            Self::Text(page) => page.record_count(),
        }
    }

    /// The file list with the row of the file `page` shows; a document without files
    /// lists none.
    pub fn file_selection(&self, page: PageIndex) -> Option<FileSelection<'_>> {
        match self {
            Self::Files { pages, file_tree } => Some(FileSelection {
                tree: file_tree,
                selected: pages[page.0].row,
            }),
            Self::Text(_) => None,
        }
    }

    /// Every page, in page order.
    #[cfg(test)]
    fn pages(&self) -> Vec<&PreparedPage> {
        match self {
            Self::Files { pages, .. } => pages.iter().map(|file| &file.page).collect(),
            Self::Text(page) => vec![page],
        }
    }

    // Preparation

    /// One page per listed file, in list order.
    fn of_files(files: &DiffFiles, observer: &mut impl Observer) -> Self {
        let ListedTree {
            tree: file_tree,
            files: listed,
        } = observer.measure(Stage::Labels, |_| ListedTree::build(files));

        let mut syntax = SyntaxHighlighter::default();
        Self::Files {
            pages: listed.map(|file| FilePage::of(*file, &mut syntax, observer)),
            file_tree,
        }
    }

    /// One page of all the text.
    fn of_text(text: &[SourceLine], observer: &mut impl Observer) -> Self {
        Self::Text(PreparedPage::of_text(text, observer))
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
pub struct PageCount(NonZeroUsize);

impl PageCount {
    /// `count` pages, for tests that step through pages without a document.
    #[cfg(test)]
    pub const fn of_test(count: usize) -> Self {
        Self(NonZeroUsize::new(count).expect("a test page count is nonzero"))
    }
}

impl FilePage {
    /// The prepared page for a listed file, beside the file-list row that names it.
    fn of(
        listed: ListedFile<'_>,
        syntax: &mut SyntaxHighlighter,
        observer: &mut impl Observer,
    ) -> Self {
        Self {
            page: PreparedPage::of_file_view(listed.view, syntax, observer),
            row: listed.row,
        }
    }
}

impl PreparedPage {
    /// The page of one file occurrence with its surrounding text.
    fn of_file_view(
        view: FileView<'_>,
        syntax: &mut SyntaxHighlighter,
        observer: &mut impl Observer,
    ) -> Self {
        Self {
            rows: StyledRows::of_file_view(view, syntax, observer),
        }
    }

    /// The page of a document's text; surrounding text has no syntax or character
    /// detail.
    fn of_text(text: &[SourceLine], observer: &mut impl Observer) -> Self {
        Self {
            rows: StyledRows::of_text(text, observer),
        }
    }

    pub fn rows(&self) -> &PageRows {
        self.rows.rows()
    }

    /// Row `index` with the styles `granularity` presents.
    pub fn row(&self, index: LineIndex, granularity: DiffGranularity) -> StyledRow<'_> {
        self.rows.row(index, granularity)
    }

    /// How many hunk records the page's rows show; a text page shows none.
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
        // Each page opens with its message, then the `---`, `+++` and `@@` header lines.
        const MESSAGE_AND_HEADERS: usize = 4;
        let patch = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
        let input = format!("first message\n{patch}second message\n{patch}trailer\n");
        let document = parse_patch(input.as_bytes()).expect("diffs with messages");
        let prepared = PreparedDocument::prepare(&document);

        assert_eq!(prepared.file_count(), 2);
        assert_eq!(prepared.record_count(), 4);
        for (page, expected) in prepared.pages().into_iter().zip([
            format!("first message\n{patch}"),
            format!("second message\n{patch}trailer\n"),
        ]) {
            let rendered: String = page
                .rows()
                .lines()
                .map(|(_, line)| format!("{}{}\n", line.kind().marker(), line.payload()))
                .collect();
            let line_styles = |row| {
                page.row(row, DiffGranularity::Line)
                    .segments()
                    .map(|segment| segment.style)
                    .collect::<Vec<_>>()
            };
            let message_styles = line_styles(page.rows().line_index(0));
            let record_styles: Vec<_> = page
                .rows()
                .line_indices()
                .skip(MESSAGE_AND_HEADERS)
                .flat_map(line_styles)
                .collect();

            assert_eq!(rendered, expected);
            assert!(
                message_styles
                    .iter()
                    .all(|style| *style == TextStyle::Plain),
                "message is not source code: {message_styles:?}"
            );
            assert!(
                record_styles
                    .iter()
                    .any(|style| matches!(style, TextStyle::Syntax(_))),
                "records are highlighted as source code: {record_styles:?}"
            );
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

        assert_eq!(
            ordinary.file_selection(PageIndex::FIRST),
            observed.file_selection(PageIndex::FIRST)
        );
        assert_eq!(ordinary.record_count(), observed.record_count());
        for (ordinary, observed) in ordinary.pages().into_iter().zip(observed.pages()) {
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

        let page = prepared.page(PageIndex::FIRST);
        let styles: Vec<_> = page
            .rows()
            .line_indices()
            .flat_map(|row| {
                page.row(row, DiffGranularity::Character)
                    .segments()
                    .map(|segment| segment.style)
                    .collect::<Vec<_>>()
            })
            .collect();

        assert!(
            styles
                .iter()
                .any(|style| matches!(style, TextStyle::Detail(_, DetailStyle::Changed))),
            "a changed character is marked in character detail: {styles:?}"
        );
    }
}
