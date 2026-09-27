use crate::{
    detail::detail_rows,
    document::{DiffDocument, FileView},
    file_tree::FileTree,
    interaction::DiffGranularity,
    layout::{Layout, file_layout},
    measurement::{Observer, Stage, Unobserved},
    styling::{StyledLayout, StyledRow},
    syntax::{SyntaxHighlighter, SyntaxSpan},
};

pub struct PreparedDocument {
    files: Vec<PreparedFile>,
    file_tree: FileTree,
}

pub struct PreparedFile {
    rows: StyledLayout,
    list_row: usize,
}

impl PreparedDocument {
    pub fn prepare(document: &DiffDocument) -> Self {
        Self::prepare_observed(document, &mut Unobserved)
    }

    pub fn prepare_observed(document: &DiffDocument, observer: &mut impl Observer) -> Self {
        let mut syntax = SyntaxHighlighter::default();
        let views = document.file_views().collect::<Vec<_>>();
        let file_tree = observer.measure(Stage::Labels, |_| FileTree::build(&views));

        let files = file_tree
            .files()
            .iter()
            .map(|listed| {
                PreparedFile::prepare(views[listed.view], listed.row, &mut syntax, observer)
            })
            .collect();

        Self { files, file_tree }
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

    pub fn file_tree(&self) -> &FileTree {
        &self.file_tree
    }
}

impl PreparedFile {
    fn prepare(
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
            list_row,
        }
    }

    /// The file's row in the [file tree](FileTree).
    pub fn list_row(&self) -> usize {
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
            .filter(|line| matches!(line.kind, crate::render::RenderedLineKind::Record(_)))
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
            let file = prepared.file(index).expect("separate file occurrence");
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
        for (ordinary, observed) in ordinary.files.iter().zip(&observed.files) {
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

        let file = prepared.file(0).expect("prepared file");
        assert!((0..file.layout().diff_lines.len()).any(|row| {
            file.row(row, DiffGranularity::Character)
                .segments()
                .any(|(_, style)| style == TextStyle::Detail)
        }));
    }
}
