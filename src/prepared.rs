use crate::{
    display_lines::DisplayLine,
    document::{DiffDocument, DiffFiles, FileView},
    file_tree::{Arrival, FileSelection, ListedTree, Listing, TreeRowIndex},
    measurement::{Observer, Stage, Unobserved},
    non_empty::Growing,
    page_rows::{LineIndex, PageRows},
    preferences::DiffGranularity,
    source_text::SourceLine,
    styling::{StyledRow, StyledRows},
    syntax::DisplayScan,
};

/// A document ready to draw: one page per listed file with the file list, or one page of
/// the text of a document that lists no file. Rules:
/// docs/spec/architecture.md#core-model.
pub enum PreparedDocument {
    Files(PreparedFiles),
    Text(PreparedPage),
}

/// The pages of the files listed so far, in input order, with the file list they make
/// and whether more are coming.
pub struct PreparedFiles {
    pages: Growing<FilePage>,
    listed: ListedTree,
    arrival: Arrival,
}

/// A file occurrence's page, with where the file list lists it.
pub struct FilePage {
    page: PreparedPage,
    listing: Listing,
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
            DiffDocument::Files(files) => Self::Files(PreparedFiles::of(files, observer)),
            DiffDocument::Text(text) => Self::Text(PreparedPage::of_text(text, observer)),
        }
    }

    /// The page at `index`.
    ///
    /// Panics when `index` names no page of the document.
    pub fn page(&self, index: PageIndex) -> &PreparedPage {
        match self {
            Self::Files(files) => &files.pages[index.0].page,
            Self::Text(page) => {
                assert_eq!(index, PageIndex::FIRST, "a text document has one page");
                page
            }
        }
    }

    /// The order file rotation visits the pages in.
    pub fn page_order(&self) -> PageOrder<'_> {
        match self {
            Self::Files(files) => PageOrder {
                order: files.listed.order(),
                arrival: files.arrival,
            },
            Self::Text(_) => PageOrder {
                order: &[0],
                arrival: Arrival::Ended,
            },
        }
    }

    /// How many file occurrences the document lists.
    pub fn file_count(&self) -> usize {
        match self {
            Self::Files(files) => files.pages.len().get(),
            Self::Text(_) => 0,
        }
    }

    /// How many hunk records every page holds together.
    pub fn record_count(&self) -> usize {
        match self {
            Self::Files(files) => files
                .pages
                .iter()
                .map(|file_page| file_page.page.record_count())
                .sum(),
            Self::Text(page) => page.record_count(),
        }
    }

    /// The file list with the row of the file `page` shows; a document without files
    /// lists none.
    pub fn file_selection(&self, page: PageIndex) -> Option<FileSelection<'_>> {
        match self {
            Self::Files(files) => Some(FileSelection {
                tree: &files.listed.tree,
                selected: files.listed.row_of(page.0),
                pinned_line: None,
            }),
            Self::Text(_) => None,
        }
    }

    /// Every page, in input order.
    #[cfg(test)]
    pub(crate) fn pages(&self) -> Vec<&PreparedPage> {
        match self {
            Self::Files(files) => files
                .pages
                .iter()
                .map(|file_page| &file_page.page)
                .collect(),
            Self::Text(page) => vec![page],
        }
    }
}

impl PreparedFiles {
    /// The files of a document that more input follows, starting from its `first` page.
    pub fn loading(first: FilePage) -> Self {
        let pages = Growing::new(first);
        let listed = list(&pages, Arrival::More);

        Self {
            pages,
            listed,
            arrival: Arrival::More,
        }
    }

    /// One page per file of a complete document.
    fn of(files: &DiffFiles, observer: &mut impl Observer) -> Self {
        let pages = Growing::from(
            files
                .views()
                .map(|view| FilePage::prepared(*view, observer)),
        );
        let listed = observer.time(Stage::Labels, || list(&pages, Arrival::Ended));

        Self {
            pages,
            listed,
            arrival: Arrival::Ended,
        }
    }

    /// The file-list row of the file `page` shows.
    pub fn row_of(&self, page: PageIndex) -> TreeRowIndex {
        self.listed.row_of(page.0)
    }

    /// Applies `updates` in order, listing each arrived file as it comes; returns the page
    /// of the file whose page grew, if one did. A file whose whole section lists it
    /// elsewhere than its headers did, such as one that turned out broken, relists every
    /// file.
    pub fn receive(&mut self, updates: impl IntoIterator<Item = Arrived>) -> Option<PageIndex> {
        let mut grown = None;
        for update in updates {
            match update {
                Arrived::Whole(page) | Arrived::Opened(page) => {
                    self.listed.push(&page.listing);
                    self.pages.push(page);
                }
                Arrived::Grew(part) => grown = Some(self.grow_last(part)),
                Arrived::Closed { part, listing } => {
                    grown = Some(self.grow_last(part));
                    if self.last_mut().listing != listing {
                        self.last_mut().listing = listing;
                        self.listed = list(&self.pages, self.arrival);
                    }
                }
                Arrived::Ended(trailing) => {
                    grown = Some(self.grow_last(trailing));
                    self.arrival = Arrival::Ended;
                    self.listed.end();
                }
            }
        }

        grown
    }

    /// Appends `part` to the last file's page, and returns that page.
    fn grow_last(&mut self, part: PreparedPage) -> PageIndex {
        self.last_mut().page.append(part);
        PageIndex(self.pages.len().get() - 1)
    }

    fn last_mut(&mut self) -> &mut FilePage {
        self.pages.last_mut()
    }
}

/// What arrives for a document whose files are still loading, in input order.
pub enum Arrived {
    /// A file that arrived whole.
    Whole(FilePage),
    /// A file whose page grows as more of it arrives.
    Opened(FilePage),
    /// The next part of the last file's page, from a hunk header on.
    Grew(PreparedPage),
    /// The last part of the last file's page, with where the list lists the whole file.
    Closed {
        part: PreparedPage,
        listing: Listing,
    },
    /// The text after the last file, which ends the input.
    Ended(PreparedPage),
}

impl Arrived {
    /// Whether more input follows this update.
    pub fn arrival(&self) -> Arrival {
        match self {
            Self::Ended(_) => Arrival::Ended,
            Self::Whole(_) | Self::Opened(_) | Self::Grew(_) | Self::Closed { .. } => Arrival::More,
        }
    }
}

/// The file list of `pages`, in input order.
fn list(pages: &Growing<FilePage>, arrival: Arrival) -> ListedTree {
    let listings = pages
        .iter()
        .map(|file_page| file_page.listing.clone())
        .collect::<Vec<_>>();

    ListedTree::build(&listings, arrival)
}

/// The position of one page among a prepared document's pages, in input order. A file
/// keeps its page as files arrive around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageIndex(usize);

impl PageIndex {
    pub const FIRST: Self = Self(0);
}

/// The order file rotation visits a document's pages in, which is the file list's, and
/// whether more pages are coming. Only a document issues one, so steps stay among its
/// pages.
#[derive(Clone, Copy)]
pub struct PageOrder<'a> {
    /// Each page's input position, in list order.
    order: &'a [usize],
    arrival: Arrival,
}

impl<'a> PageOrder<'a> {
    /// Pages visited in `order`, all arrived, for tests that step through pages without a
    /// document.
    #[cfg(test)]
    pub const fn of_test(order: &'a [usize]) -> Self {
        Self {
            order,
            arrival: Arrival::Ended,
        }
    }
}

impl PageOrder<'_> {
    /// The page listed first.
    pub fn first(self) -> PageIndex {
        PageIndex(self.order[0])
    }

    /// The page after `page` in list order, wrapping to the first once the input has
    /// ended. While more files are coming, the last page stays where it is.
    /// Rules: docs/spec/architecture.md#file-tree.
    pub fn next(self, page: PageIndex) -> PageIndex {
        let position = self.position(page);
        let last = position + 1 == self.order.len();

        match (last, self.arrival) {
            (true, Arrival::More) => page,
            (true, Arrival::Ended) => PageIndex(self.order[0]),
            (false, Arrival::More | Arrival::Ended) => PageIndex(self.order[position + 1]),
        }
    }

    /// The page before `page` in list order, wrapping to the last.
    pub fn previous(self, page: PageIndex) -> PageIndex {
        let position = self.position(page);
        let before = position.checked_sub(1).unwrap_or(self.order.len() - 1);

        PageIndex(self.order[before])
    }

    /// Where `page` sits in list order.
    fn position(self, page: PageIndex) -> usize {
        self.order
            .iter()
            .position(|&input_position| input_position == page.0)
            .expect("a page step starts at a listed page")
    }
}

impl FilePage {
    /// A page that starts with `part`, listed as `listing`.
    pub fn of_part(part: PreparedPage, listing: Listing) -> Self {
        Self {
            page: part,
            listing,
        }
    }

    /// The prepared page for one file occurrence, with where the file list lists it.
    pub fn prepare(view: FileView<'_>) -> Self {
        Self::prepared(view, &mut Unobserved)
    }

    fn prepared(view: FileView<'_>, observer: &mut impl Observer) -> Self {
        Self {
            page: PreparedPage::of_file_view(view, observer),
            listing: Listing::of(view),
        }
    }
}

impl PreparedPage {
    /// The part of a page `lines` make, whose syntax `scan` reads in display order; see
    /// [`StyledRows::of_lines`].
    pub fn of_lines<'a>(
        lines: impl Iterator<Item = DisplayLine<'a>>,
        scan: &mut DisplayScan,
        observer: &mut impl Observer,
    ) -> Self {
        Self {
            rows: StyledRows::of_lines(lines, scan, observer),
        }
    }

    /// Appends `later`, the next part of the page.
    fn append(&mut self, later: Self) {
        self.rows.append(later.rows);
    }

    /// The page of one file occurrence with its surrounding text.
    fn of_file_view(view: FileView<'_>, observer: &mut impl Observer) -> Self {
        Self {
            rows: StyledRows::of_file_view(view, observer),
        }
    }

    /// A part with no lines, which leaves a page as it is.
    pub fn empty() -> Self {
        Self::of_text(&[], &mut Unobserved)
    }

    /// The page of a document's text; surrounding text has no syntax or character
    /// detail.
    pub fn of_text(text: &[SourceLine], observer: &mut impl Observer) -> Self {
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
        for (ordinary_page, observed_page) in ordinary.pages().into_iter().zip(observed.pages()) {
            assert_eq!(ordinary_page, observed_page);
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
