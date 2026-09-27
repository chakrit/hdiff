//! Writes one pane row into the frame buffer. Rules: `docs/spec/architecture.md` (Lossless
//! and safe rendering).

use std::ops::ControlFlow;

use ratatui::{
    buffer::{Buffer, CellWidth},
    layout::Rect,
    style::Style,
};
use unicode_segmentation::UnicodeSegmentation;

use super::rows::{PaneRow, gutter, row_style, segment_style};
use crate::{
    styling::{StyledRow, TextStyle},
    tab_stops::TabStops,
};

/// Fills `area` with the row's style, then writes its marker gutter and payload from
/// column `horizontal_offset` onward.
pub(super) fn write_row(
    buffer: &mut Buffer,
    area: Rect,
    row: PaneRow<'_>,
    horizontal_offset: u16,
    color_count: u16,
) {
    buffer.set_style(area, row_style(row.kind(), color_count));
    let PaneRow::Text(row) = row else {
        return;
    };

    let payload = ExpandedPayload::new(row);
    let payload_clusters = payload
        .clusters()
        .map(|(cluster, text_style)| (cluster, segment_style(row.kind, text_style, color_count)));
    let mut cursor = RowCursor {
        buffer,
        area,
        horizontal_offset: usize::from(horizontal_offset),
        column: 0,
    };

    for (cluster, style) in gutter(row.kind, color_count)
        .into_iter()
        .chain(payload_clusters)
    {
        if cursor.write(cluster, style).is_break() {
            break;
        }
    }
}

/// A row's payload with tabs expanded, segmented into grapheme clusters as a whole so
/// style boundaries cannot split a cluster.
struct ExpandedPayload {
    text: String,
    segment_ends: Vec<(usize, TextStyle)>,
}

impl ExpandedPayload {
    fn new(row: StyledRow<'_>) -> Self {
        let mut tab_stops = TabStops::default();
        let mut text = String::new();
        let mut segment_ends = Vec::new();

        for (segment, text_style) in row.segments() {
            tab_stops.expand_into(segment, &mut text);
            segment_ends.push((text.len(), text_style));
        }
        Self { text, segment_ends }
    }

    /// Each cluster with the style of the segment containing its first byte.
    fn clusters(&self) -> impl Iterator<Item = (&str, TextStyle)> {
        let mut segments = self.segment_ends.iter().peekable();

        self.text
            .grapheme_indices(true)
            .map(move |(start, cluster)| {
                while let Some((end, _)) = segments.peek()
                    && *end <= start
                {
                    segments.next();
                }
                let (_, text_style) = segments
                    .peek()
                    .expect("every cluster starts inside a segment");
                (cluster, *text_style)
            })
    }
}

/// Places clusters by logical column, showing the columns from `horizontal_offset` to the
/// pane's right edge.
struct RowCursor<'b> {
    buffer: &'b mut Buffer,
    area: Rect,
    horizontal_offset: usize,
    column: usize,
}

impl RowCursor<'_> {
    fn write(&mut self, cluster: &str, style: Style) -> ControlFlow<()> {
        // Ratatui skips control-containing and zero-width clusters the same way.
        if cluster.contains(char::is_control) {
            return ControlFlow::Continue(());
        }
        let width = usize::from(cluster.cell_width());
        if width == 0 {
            return ControlFlow::Continue(());
        }

        let start = self.column;
        let end = start + width;
        self.column = end;
        if end <= self.horizontal_offset {
            return ControlFlow::Continue(());
        }
        if end - self.horizontal_offset > usize::from(self.area.width) {
            return ControlFlow::Break(());
        }

        match start.checked_sub(self.horizontal_offset) {
            None => self.fill_blank(0..end - self.horizontal_offset, style),
            Some(visible_start) => self.put(visible_start, width, cluster, style),
        }
        ControlFlow::Continue(())
    }

    fn fill_blank(&mut self, cells: std::ops::Range<usize>, style: Style) {
        for cell in cells {
            let x = self.x(cell);
            self.buffer[(x, self.area.y)]
                .set_symbol(" ")
                .set_style(style);
        }
    }

    /// Writes the cluster in its first cell and resets the cells it covers, as
    /// `Buffer::set_stringn` does.
    fn put(&mut self, cell: usize, width: usize, cluster: &str, style: Style) {
        let x = self.x(cell);
        let y = self.area.y;

        self.buffer[(x, y)].set_symbol(cluster).set_style(style);
        for continuation in cell + 1..cell + width {
            let x = self.x(continuation);
            self.buffer[(x, y)].reset();
        }
    }

    fn x(&self, cell: usize) -> u16 {
        let cell = u16::try_from(cell).expect("visible cells fit the pane");
        self.area.x + cell
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::{Buffer, CellWidth},
        layout::Rect,
        style::{Color, Modifier},
    };

    use super::write_row;
    use crate::{
        interaction::{DiffGranularity, Viewport},
        parser::parse_unified_diff,
        prepared::PreparedDocument,
        terminal::rows::{PaneRow, visible_diff_rows},
    };

    const ADDITION_BACKGROUND: Color = Color::Rgb(35, 73, 43);

    #[test]
    fn syntax_highlighting_preserves_source_text() {
        let prepared = prepare(
            b"--- a/example.rs\n+++ b/example.rs\n@@ -1,3 +1,3 @@\n fn example() {\n-    let excluded = ace.excluded_mcp();\n+    let excluded = ace.excluded_mcp()?;\n }\n",
        );

        // architecture.md: lossless rendering preserves source text through styling.
        assert_eq!(
            rendered_rows(&prepared, 120),
            [
                "  --- a/example.rs",
                "  +++ b/example.rs",
                "  @@ -1,3 +1,3 @@",
                "  fn example() {",
                "-     let excluded = ace.excluded_mcp();",
                "+     let excluded = ace.excluded_mcp()?;",
                "  }",
            ]
        );
    }

    #[test]
    fn preserves_unicode_crlf_and_unmarked_records() {
        let prepared = prepare(
            "--- a/file\r\n+++ b/file\r\n@@ -1 +1 @@\r\n-旧\r\n\\ No newline at end of file\r\n\r\n界\r\n+新\r\n\\ No newline at end of file".as_bytes(),
        );

        assert_eq!(
            rendered_rows(&prepared, 120),
            [
                "  --- a/file",
                "  +++ b/file",
                "  @@ -1 +1 @@",
                "- 旧",
                "  \\ No newline at end of file",
                "",
                "  界",
                "+ 新",
                "  \\ No newline at end of file",
            ]
        );
    }

    #[test]
    fn keeps_a_cluster_whole_when_its_mark_starts_a_style_segment() {
        let prepared = prepare("--- a/t\n+++ b/t\n@@ -1 +1 @@\n-ก\n+ก่\n".as_bytes());
        let file = prepared.file(0).expect("prepared file");
        let addition = file.row(4, DiffGranularity::Character);

        let buffer = write(PaneRow::Text(addition), 8, 0);

        // architecture.md: style boundaries never split or drop a grapheme cluster.
        assert_eq!(buffer[(2, 0)].symbol(), "ก่");
    }

    #[test]
    fn blanks_the_visible_cell_of_a_wide_cluster_cut_by_the_left_edge() {
        let prepared = prepare("--- a/w\n+++ b/w\n@@ -0,0 +1 @@\n+界x\n".as_bytes());
        let file = prepared.file(0).expect("prepared file");
        let addition = file.row(3, DiffGranularity::Line);

        let buffer = write(PaneRow::Text(addition), 8, 3);

        // architecture.md: a wide grapheme cut by the left pane edge renders its visible
        // cells blank, keeping later text in its column.
        assert_eq!(buffer[(0, 0)].symbol(), " ");
        assert_eq!(buffer[(0, 0)].bg, ADDITION_BACKGROUND);
        assert_eq!(buffer[(1, 0)].symbol(), "x");
    }

    #[test]
    fn leaves_a_wide_cluster_that_does_not_fit_the_right_edge_blank() {
        let prepared = prepare("--- a/w\n+++ b/w\n@@ -0,0 +1 @@\n+ab界\n".as_bytes());
        let file = prepared.file(0).expect("prepared file");
        let addition = file.row(3, DiffGranularity::Line);

        let buffer = write(PaneRow::Text(addition), 5, 0);

        assert_eq!(buffer[(3, 0)].symbol(), "b");
        assert_eq!(buffer[(4, 0)].symbol(), " ");
        assert_eq!(buffer[(4, 0)].bg, ADDITION_BACKGROUND);
    }

    #[test]
    fn fills_an_alignment_peer_with_its_change_style_only() {
        let buffer = write(
            PaneRow::AlignmentPeer(crate::styling::RowKind::Deletion),
            4,
            0,
        );

        assert!((0..4).all(|x| buffer[(x, 0)].symbol() == " "));
        assert!(buffer[(3, 0)].modifier.contains(Modifier::DIM));
    }

    fn prepare(diff: &[u8]) -> PreparedDocument {
        let document = parse_unified_diff(diff).expect("valid diff");
        PreparedDocument::prepare(&document)
    }

    fn write(row: PaneRow<'_>, width: u16, horizontal_offset: u16) -> Buffer {
        let area = Rect::new(0, 0, width, 1);
        let mut buffer = Buffer::empty(area);
        write_row(&mut buffer, area, row, horizontal_offset, u16::MAX);
        buffer
    }

    fn rendered_rows(prepared: &PreparedDocument, width: u16) -> Vec<String> {
        let file = prepared.file(0).expect("prepared file");
        let rows = visible_diff_rows(
            file,
            &Viewport::new(width, 12),
            DiffGranularity::Line,
            3,
            usize::MAX,
        );

        rows.into_iter()
            .map(|row| row_text(&write(PaneRow::Text(row), width, 0)))
            .collect()
    }

    /// The row's symbols with wide-cluster continuation cells skipped and trailing blanks
    /// trimmed.
    fn row_text(buffer: &Buffer) -> String {
        let mut text = String::new();
        let mut x = 0;
        while x < buffer.area.width {
            let symbol = buffer[(x, 0)].symbol();
            text.push_str(symbol);
            x += symbol.cell_width().max(1);
        }
        text.trim_end().to_owned()
    }
}
