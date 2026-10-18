//! Writes one pane row into the frame buffer. Rules: `docs/spec/architecture.md` (Lossless
//! and safe rendering, View model).

use std::ops::ControlFlow;

use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use unicode_segmentation::UnicodeSegmentation;

use super::rows::{PaneRow, gutter, row_style, segment_style};
use crate::{
    layout::{MARKER_GUTTER_WIDTH, Wrap},
    line_breaks::LineBreaks,
    styling::{StyledRow, TextStyle},
    tab_stops::{TabStops, cluster_cells},
    theme::Theme,
};

/// Which part of a row a pane line shows.
#[derive(Clone, Copy)]
pub(super) enum RowPlacement {
    /// The whole row, from column `horizontal_offset` onward.
    Scrolled { horizontal_offset: u16 },
    /// Wrapped line `line` of the row's payload, wrapped at `width` cells.
    Wrapped { line: usize, width: usize },
}

impl RowPlacement {
    pub(super) fn new(wrap: Wrap, horizontal_offset: u16, line: usize) -> Self {
        match wrap {
            Wrap::Off => Self::Scrolled { horizontal_offset },
            Wrap::At(width) => Self::Wrapped { line, width },
        }
    }
}

/// Whether a written line left cells of its row outside the pane on either side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Clip {
    pub(super) left: bool,
    pub(super) right: bool,
}

impl Clip {
    /// The sides clipped in either `self` or `other`.
    pub(super) fn union(self, other: Clip) -> Clip {
        Clip {
            left: self.left || other.left,
            right: self.right || other.right,
        }
    }
}

/// Fills `area` with the row's style, then writes the part of its marker gutter and
/// payload that `placement` selects. Only a row's first line carries its marker.
pub(super) fn write_row(
    buffer: &mut Buffer,
    area: Rect,
    row: PaneRow<'_>,
    placement: RowPlacement,
    theme: &Theme,
) -> Clip {
    buffer.set_style(area, row_style(row.kind(), &theme.rows));
    let PaneRow::Text(row) = row else {
        return Clip::default();
    };

    let payload = ExpandedPayload::new(row);
    let payload_clusters = payload
        .clusters()
        .map(|(cluster, text_style)| (cluster, segment_style(row.kind, text_style, theme)));
    let gutter = gutter(row.kind, &theme.rows);

    let cursor = RowCursor {
        buffer,
        area,
        horizontal_offset: 0,
        column: 0,
        clip: Clip::default(),
    };

    match placement {
        RowPlacement::Scrolled { horizontal_offset } => {
            let cursor = RowCursor {
                horizontal_offset: usize::from(horizontal_offset),
                ..cursor
            };
            write_clusters(cursor, gutter.into_iter().chain(payload_clusters))
        }
        RowPlacement::Wrapped { line: 0, width } => {
            write_wrapped_line(cursor, gutter, payload_clusters, 0, width)
        }
        RowPlacement::Wrapped { line, width } => {
            let cursor = RowCursor {
                column: MARKER_GUTTER_WIDTH,
                ..cursor
            };
            write_wrapped_line(cursor, [], payload_clusters, line, width)
        }
    }
}

fn write_clusters<'c>(
    mut cursor: RowCursor<'_>,
    clusters: impl Iterator<Item = (&'c str, Style)>,
) -> Clip {
    for (cluster, style) in clusters {
        if cursor.write(cluster, style).is_break() {
            break;
        }
    }
    cursor.clip
}

/// Writes `gutter`, then the payload clusters that wrapping at `width` places on `line`.
fn write_wrapped_line<'c>(
    cursor: RowCursor<'_>,
    gutter: impl IntoIterator<Item = (&'c str, Style)>,
    payload_clusters: impl Iterator<Item = (&'c str, Style)>,
    line: usize,
    width: usize,
) -> Clip {
    let mut breaks = LineBreaks::new(width);
    let line_clusters = payload_clusters
        .filter_map(|(cluster, style)| Some((breaks.place(cluster)?, cluster, style)))
        .skip_while(|(placed_line, _, _)| *placed_line < line)
        .take_while(|(placed_line, _, _)| *placed_line == line)
        .map(|(_, cluster, style)| (cluster, style));

    write_clusters(cursor, gutter.into_iter().chain(line_clusters))
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
/// pane's right edge, and records the sides where clusters fell outside.
struct RowCursor<'b> {
    buffer: &'b mut Buffer,
    area: Rect,
    horizontal_offset: usize,
    column: usize,
    clip: Clip,
}

impl RowCursor<'_> {
    fn write(&mut self, cluster: &str, style: Style) -> ControlFlow<()> {
        let width = cluster_cells(cluster);
        if width == 0 {
            return ControlFlow::Continue(());
        }

        let start = self.column;
        let end = start + width;
        self.column = end;
        if start < self.horizontal_offset {
            self.clip.left = true;
        }
        if end <= self.horizontal_offset {
            return ControlFlow::Continue(());
        }
        if end - self.horizontal_offset > usize::from(self.area.width) {
            self.clip.right = true;
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
        style::Modifier,
    };

    use super::{Clip, RowPlacement, write_row};
    use crate::{
        document::RecordKind,
        layout::{DisplayExtent, Wrap},
        parser::parse_unified_diff,
        preferences::{DiffGranularity, DiffLayout},
        prepared::{PageIndex, PreparedDocument},
        styling::RowKind,
        terminal::rows::{PaneRow, visible_diff_rows},
        theme::{ColorDepth, Theme},
    };

    const THEME: &Theme = Theme::for_depth(ColorDepth::TrueColor);

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
        let file = prepared.page(PageIndex::FIRST);
        let addition = file.row(file.layout().line_index(4), DiffGranularity::Character);

        let buffer = write(PaneRow::Text(addition), 8, 0);

        // architecture.md: style boundaries never split or drop a grapheme cluster.
        assert_eq!(buffer[(2, 0)].symbol(), "ก่");
    }

    #[test]
    fn blanks_the_visible_cell_of_a_wide_cluster_cut_by_the_left_edge() {
        let prepared = prepare("--- a/w\n+++ b/w\n@@ -0,0 +1 @@\n+界x\n".as_bytes());
        let file = prepared.page(PageIndex::FIRST);
        let addition = file.row(file.layout().line_index(3), DiffGranularity::Line);

        let buffer = write(PaneRow::Text(addition), 8, 3);

        // architecture.md: a wide grapheme cut by the left pane edge renders its visible
        // cells blank, keeping later text in its column.
        assert_eq!(buffer[(0, 0)].symbol(), " ");
        assert_eq!(buffer[(0, 0)].bg, THEME.rows.addition_background);
        assert_eq!(buffer[(1, 0)].symbol(), "x");
    }

    #[test]
    fn leaves_a_wide_cluster_that_does_not_fit_the_right_edge_blank() {
        let prepared = prepare("--- a/w\n+++ b/w\n@@ -0,0 +1 @@\n+ab界\n".as_bytes());
        let file = prepared.page(PageIndex::FIRST);
        let addition = file.row(file.layout().line_index(3), DiffGranularity::Line);

        let buffer = write(PaneRow::Text(addition), 5, 0);

        assert_eq!(buffer[(3, 0)].symbol(), "b");
        assert_eq!(buffer[(4, 0)].symbol(), " ");
        assert_eq!(buffer[(4, 0)].bg, THEME.rows.addition_background);
    }

    #[test]
    fn writes_each_wrapped_line_with_the_marker_on_the_first_only() {
        let prepared = prepare("--- a/w\n+++ b/w\n@@ -0,0 +1 @@\n+abcde界f\n".as_bytes());
        let file = prepared.page(PageIndex::FIRST);
        let addition = file.row(file.layout().line_index(3), DiffGranularity::Line);
        let line = |line| {
            write_placed(
                PaneRow::Text(addition),
                6,
                RowPlacement::Wrapped { line, width: 4 },
            )
            .0
        };

        let (first, second, past_the_end) = (line(0), line(1), line(2));

        assert_eq!(row_text(&first), "+ abcd");
        assert_eq!(row_text(&second), "  e界f");
        assert_eq!(row_text(&past_the_end), "");
        assert_eq!(second[(0, 0)].bg, THEME.rows.addition_background);
        assert_eq!(past_the_end[(5, 0)].bg, THEME.rows.addition_background);
    }

    #[test]
    fn reports_the_sides_where_a_row_leaves_the_pane() {
        let prepared = prepare("--- a/c\n+++ b/c\n@@ -0,0 +1 @@\n+abc界\n".as_bytes());
        let file = prepared.page(PageIndex::FIRST);
        let row = PaneRow::Text(file.row(file.layout().line_index(3), DiffGranularity::Line));
        let clip = |width, horizontal_offset| {
            let area = Rect::new(0, 0, width, 1);
            let mut buffer = Buffer::empty(area);
            let placement = RowPlacement::Scrolled { horizontal_offset };
            write_row(&mut buffer, area, row, placement, THEME)
        };
        let sides = |left, right| Clip { left, right };

        // architecture.md (Continuity markers): the row has cells left of the offset, or
        // continues past the right edge.
        assert_eq!(clip(7, 0), sides(false, false), "the row fits");
        assert_eq!(
            clip(6, 0),
            sides(false, true),
            "a wide cluster that does not fit"
        );
        assert_eq!(clip(4, 1), sides(true, true));
        assert_eq!(clip(6, 1), sides(true, false));
        assert_eq!(
            write_placed(row, 3, RowPlacement::Wrapped { line: 1, width: 1 }).1,
            sides(false, false),
            "a wrapped line shows its whole part of the row"
        );
    }

    #[test]
    fn fills_an_alignment_peer_with_its_change_style_only() {
        let buffer = write(
            PaneRow::AlignmentPeer(RowKind::Record(RecordKind::Deletion)),
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
        write_placed(row, width, RowPlacement::Scrolled { horizontal_offset }).0
    }

    fn write_placed(row: PaneRow<'_>, width: u16, placement: RowPlacement) -> (Buffer, Clip) {
        let area = Rect::new(0, 0, width, 1);
        let mut buffer = Buffer::empty(area);
        let clip = write_row(&mut buffer, area, row, placement, THEME);
        (buffer, clip)
    }

    fn rendered_rows(prepared: &PreparedDocument, width: u16) -> Vec<String> {
        let file = prepared.page(PageIndex::FIRST);
        let DisplayExtent::Unified(extent) =
            file.layout()
                .scroll_extent(DiffLayout::Unified, 3, Wrap::Off)
        else {
            panic!("the unified layout shows lines");
        };
        let rows = visible_diff_rows(file, &extent, 0, DiffGranularity::Line, usize::MAX);

        rows.into_iter()
            .map(|line| row_text(&write(PaneRow::Text(line.row), width, 0)))
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
