use crate::{
    detail::{DetailSpan, detail_rows},
    document::{DiffDocument, DiffFile, sanitize},
    interaction::DiffGranularity,
    layout::{FileListRow, Layout, file_layout},
    syntax::{SyntaxHighlighter, SyntaxSpan},
};

pub struct PreparedDocument {
    files: Vec<PreparedFile>,
    file_labels: Vec<String>,
}

pub struct PreparedFile {
    pub layout: Layout,
    syntax: Vec<Vec<SyntaxSpan>>,
    character_details: Vec<Vec<DetailSpan>>,
}

impl PreparedDocument {
    pub fn prepare(document: &DiffDocument) -> Self {
        let mut syntax = SyntaxHighlighter::default();
        let file_labels = document.files.iter().map(file_list_label).collect();
        let files = document
            .files
            .iter()
            .map(|file| PreparedFile::prepare(file, &mut syntax))
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
    fn prepare(file: &DiffFile, syntax: &mut SyntaxHighlighter) -> Self {
        let layout = file_layout(file);
        let syntax = syntax.highlight_file(file);
        let character_details = detail_rows(&layout, DiffGranularity::Character);

        Self {
            layout,
            syntax,
            character_details,
        }
    }

    pub fn syntax(&self) -> &[Vec<SyntaxSpan>] {
        &self.syntax
    }

    pub fn details(&self, granularity: DiffGranularity) -> &[Vec<DetailSpan>] {
        match granularity {
            DiffGranularity::Line => &[],
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
        .files
        .iter()
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
        assert!(first.details(DiffGranularity::Line).is_empty());
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

    #[test]
    fn labels_an_added_file_with_its_new_path() {
        let document =
            parse_unified_diff(b"--- /dev/null\n+++ b/added-file.txt\n@@ -0,0 +1 @@\n+new\n")
                .expect("valid added-file diff");
        let prepared = PreparedDocument::prepare(&document);

        assert_eq!(prepared.file_rows(0)[0].label, "added-file.txt");
    }
}
