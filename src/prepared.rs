use crate::{
    detail::detail_rows,
    document::{DiffDocument, DiffFile, FileView, sanitize},
    interaction::DiffGranularity,
    layout::{FileListRow, Layout, file_layout},
    measurement::{Observer, Stage, Unobserved},
    styling::{StyledLayout, StyledRow},
    syntax::{SyntaxHighlighter, SyntaxSpan},
};

pub struct PreparedDocument {
    files: Vec<PreparedFile>,
    file_labels: Vec<String>,
}

pub struct PreparedFile {
    rows: StyledLayout,
}

impl PreparedDocument {
    pub fn prepare(document: &DiffDocument) -> Self {
        Self::prepare_observed(document, &mut Unobserved)
    }

    pub fn prepare_observed(document: &DiffDocument, observer: &mut impl Observer) -> Self {
        let mut syntax = SyntaxHighlighter::default();
        let file_labels = observer.measure(Stage::Labels, |_| {
            document.files().map(file_list_label).collect()
        });
        let files = document
            .file_views()
            .map(|file| PreparedFile::prepare(file, &mut syntax, observer))
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
        file: FileView<'_>,
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
        }
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

fn file_list_label(file: &DiffFile) -> String {
    match file {
        DiffFile::Metadata { lines } => lines
            .first()
            .map(|line| git_metadata_label(&sanitize(&line.text)))
            .unwrap_or_default(),
        DiffFile::Unified(file) => match file.old_path.as_str() {
            "/dev/null" => display_path(&file.new_path),
            _ => display_path(&file.old_path),
        },
    }
}

fn git_metadata_label(header: &str) -> String {
    let Some(paths) = header.strip_prefix("diff --git ") else {
        return header.to_owned();
    };
    let Some((old_path, new_path)) = paths.split_once(" b/") else {
        return header.to_owned();
    };
    let old_path = display_path(old_path);
    let new_path = display_path(&format!("b/{new_path}"));

    match old_path == new_path {
        true => old_path,
        false => format!("{old_path} → {new_path}"),
    }
}

fn display_path(path: &str) -> String {
    let sanitized = sanitize(path);
    sanitized
        .strip_prefix("a/")
        .or_else(|| sanitized.strip_prefix("b/"))
        .unwrap_or(&sanitized)
        .to_owned()
}

#[cfg(test)]
pub(crate) fn test_file_rows(document: &DiffDocument, selected_file: usize) -> Vec<FileListRow> {
    document
        .files()
        .enumerate()
        .map(|(index, file)| FileListRow {
            label: file_list_label(file),
            selected: index == selected_file,
        })
        .collect()
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

        assert_eq!(ordinary.file_rows(0), observed.file_rows(0));
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

    #[test]
    fn labels_an_added_file_with_its_new_path() {
        let document =
            parse_unified_diff(b"--- /dev/null\n+++ b/added-file.txt\n@@ -0,0 +1 @@\n+new\n")
                .expect("valid added-file diff");
        let prepared = PreparedDocument::prepare(&document);

        assert_eq!(prepared.file_rows(0)[0].label, "added-file.txt");
    }
}
