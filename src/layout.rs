use crate::{
    document::DiffDocument,
    interaction::Interaction,
    render::{render_file, sanitize},
};

#[derive(Debug, PartialEq, Eq)]
pub struct FileListRow {
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    pub files: Vec<FileListRow>,
    pub diff_lines: Vec<Vec<u8>>,
    pub hunk_offsets: Vec<usize>,
}

pub fn layout(document: &DiffDocument, interaction: &Interaction) -> Layout {
    let files = document
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| FileListRow {
            label: sanitize(&file.old_path),
            selected: index == interaction.selected_file,
        })
        .collect();
    let Some(file) = document.files.get(interaction.selected_file) else {
        return Layout {
            files,
            diff_lines: Vec::new(),
            hunk_offsets: Vec::new(),
        };
    };
    let diff_lines = render_file(file)
        .split_inclusive(|byte| *byte == b'\n')
        .map(ToOwned::to_owned)
        .collect();
    let mut line_offset = 2;
    let hunk_offsets = file
        .hunks
        .iter()
        .map(|hunk| {
            let offset = line_offset;
            line_offset += 1 + hunk.records.len();
            offset
        })
        .collect();

    Layout {
        files,
        diff_lines,
        hunk_offsets,
    }
}

#[cfg(test)]
mod tests {
    use super::{FileListRow, layout};
    use crate::{
        interaction::{Interaction, Viewport},
        parser::parse_unified_diff,
    };

    #[test]
    fn derives_selected_file_rows_and_hunk_offsets() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-old\n+new\n@@ -1 +1 @@\n-old-again\n+new-again\n",
        )
        .expect("valid multi-file diff");
        let interaction = Interaction {
            selected_file: 1,
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };

        let view = layout(&document, &interaction);

        assert_eq!(
            view.files,
            vec![
                FileListRow {
                    label: "a/first".to_owned(),
                    selected: false,
                },
                FileListRow {
                    label: "a/second".to_owned(),
                    selected: true,
                },
            ]
        );
        assert_eq!(view.hunk_offsets, vec![2, 5]);
        assert_eq!(
            view.diff_lines,
            vec![
                b"--- a/second\n".to_vec(),
                b"+++ b/second\n".to_vec(),
                b"@@ -1 +1 @@\n".to_vec(),
                b"-old\n".to_vec(),
                b"+new\n".to_vec(),
                b"@@ -1 +1 @@\n".to_vec(),
                b"-old-again\n".to_vec(),
                b"+new-again\n".to_vec(),
            ]
        );
    }
}
