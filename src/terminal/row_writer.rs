//! Writes one pane row into the frame buffer. Rules:
//! docs/spec/architecture.md#lossless-and-safe-rendering,
//! docs/spec/architecture.md#view-model, and
//! docs/spec/architecture.md#palette-and-character-detail.

use std::ops::ControlFlow;

use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    row_style::{gutter, segment_style, whole_row},
    rows::PaneRow,
};
use crate::{
    line_breaks::LineBreaks,
    screen::{Cells, MARKER_GUTTER_WIDTH, NonZeroCells, ScreenLines},
    styling::{Boundary, StyledRow, StyledText, TextStyle},
    tab_stops::TabStops,
    theme::Theme,
};

/// Which part of a row a pane line shows.
pub(super) enum RowPlacement {
    /// The whole row, from column `horizontal_offset` onward.
    Scrolled { horizontal_offset: Cells },
    /// Wrapped line `line` of the row's payload, wrapped at `width` cells.
    Wrapped {
        line: ScreenLines,
        width: NonZeroCells,
    },
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
    buffer.set_style(area, whole_row(row.tone(), &theme.rows));
    let PaneRow::Text(text_row) = row else {
        return Clip::default();
    };

    let payload = ExpandedPayload::new(text_row);
    let payload_clusters = payload.clusters().map(|cluster| StyledText {
        text: cluster.text,
        style: segment_style(cluster.style, theme),
    });
    let marker_gutter = gutter(text_row.tone(), &theme.rows);

    let cursor = RowCursor {
        buffer,
        area,
        horizontal_offset: Cells::ZERO,
        column: Cells::ZERO,
        clip: Clip::default(),
    };

    match placement {
        RowPlacement::Scrolled { horizontal_offset } => {
            let cursor = RowCursor {
                horizontal_offset,
                ..cursor
            };
            write_clusters(cursor, marker_gutter.into_iter().chain(payload_clusters))
        }
        RowPlacement::Wrapped {
            line: ScreenLines::ZERO,
            width,
        } => write_wrapped_line(
            cursor,
            marker_gutter,
            payload_clusters,
            ScreenLines::ZERO,
            width,
        ),
        RowPlacement::Wrapped { line, width } => {
            let cursor = RowCursor {
                column: MARKER_GUTTER_WIDTH,
                ..cursor
            };
            write_wrapped_line(cursor, [], payload_clusters, line, width)
        }
    }
}

/// Writes `clusters` from `cursor` until one passes the pane's right edge, returning the
/// sides they left the pane.
fn write_clusters<'c>(
    mut cursor: RowCursor<'_>,
    clusters: impl Iterator<Item = StyledText<'c, Style>>,
) -> Clip {
    for cluster in clusters {
        if cursor.write(cluster).is_break() {
            break;
        }
    }

    cursor.clip
}

/// Writes `gutter`, then the payload clusters that wrapping at `width` places on `line`.
fn write_wrapped_line<'c>(
    cursor: RowCursor<'_>,
    gutter: impl IntoIterator<Item = StyledText<'c, Style>>,
    payload_clusters: impl Iterator<Item = StyledText<'c, Style>>,
    line: ScreenLines,
    width: NonZeroCells,
) -> Clip {
    let line_clusters = payload_clusters
        .scan(LineBreaks::new(width), |breaks, cluster| {
            let placed = breaks.place(cluster.text).map(|placed_line| PlacedCluster {
                line: placed_line,
                cluster,
            });
            Some(placed)
        })
        .flatten()
        .skip_while(|placed| placed.line < line)
        .take_while(|placed| placed.line == line)
        .map(|placed| placed.cluster);

    write_clusters(cursor, gutter.into_iter().chain(line_clusters))
}

/// A payload cluster with the wrapped line it falls on.
struct PlacedCluster<'c> {
    line: ScreenLines,
    cluster: StyledText<'c, Style>,
}

/// A row's payload with tabs expanded, segmented into grapheme clusters as a whole so
/// style boundaries cannot split a cluster.
struct ExpandedPayload {
    text: String,
    boundaries: Vec<Boundary>,
}

impl ExpandedPayload {
    /// `row`'s styled text with its tabs expanded and its segment ends moved to match.
    fn new(row: StyledRow<'_>) -> Self {
        let mut tab_stops = TabStops::default();
        let mut text = String::new();
        let mut boundaries = Vec::new();

        for segment in row.segments() {
            tab_stops.expand_into(segment.text, &mut text);
            boundaries.push(Boundary {
                end: text.len(),
                style: segment.style,
            });
        }

        Self { text, boundaries }
    }

    /// Each cluster with the style of the segment containing its first byte.
    fn clusters(&self) -> impl Iterator<Item = StyledText<'_, TextStyle>> {
        self.text
            .grapheme_indices(true)
            .map(|(start, cluster)| StyledText {
                text: cluster,
                style: self.style_at(start),
            })
    }

    /// The style of the segment containing byte `position`: the first segment ending past
    /// it, since segment ends ascend.
    fn style_at(&self, position: usize) -> TextStyle {
        let containing = self
            .boundaries
            .partition_point(|boundary| boundary.end <= position);
        let boundary = self
            .boundaries
            .get(containing)
            .expect("every cluster starts inside a segment");

        boundary.style
    }
}

/// Places clusters by logical column, showing the columns from `horizontal_offset` to the
/// pane's right edge, and records the sides where clusters fell outside.
struct RowCursor<'b> {
    buffer: &'b mut Buffer,
    area: Rect,
    horizontal_offset: Cells,
    column: Cells,
    clip: Clip,
}

impl RowCursor<'_> {
    /// Places one `styled` cluster at the cursor's column, drawing what the pane shows of
    /// it; breaks when the cluster does not fit before the pane's right edge.
    fn write(&mut self, styled: StyledText<'_, Style>) -> ControlFlow<()> {
        let StyledText {
            text: cluster,
            style,
        } = styled;
        let width = Cells::of_cluster(cluster);
        if width == Cells::ZERO {
            return ControlFlow::Continue(());
        }

        let start = self.column;
        let end = start + width;
        self.column = end;
        if start < self.horizontal_offset {
            self.clip.left = true;
        }
        let Some(visible_end) = end.checked_sub(self.horizontal_offset) else {
            return ControlFlow::Continue(());
        };
        if visible_end == Cells::ZERO {
            return ControlFlow::Continue(());
        }
        if visible_end > Cells::from(self.area.width) {
            self.clip.right = true;
            return ControlFlow::Break(());
        }

        match start.checked_sub(self.horizontal_offset) {
            None => self.fill_blank(visible_end, style),
            Some(visible_start) => self.put(visible_start, width, cluster, style),
        }
        ControlFlow::Continue(())
    }

    /// Blanks the pane cells before `end`: the visible part of a cluster cut by the left
    /// edge.
    fn fill_blank(&mut self, end: Cells, style: Style) {
        for cell in Cells::ZERO.columns_until(end) {
            let x = self.x(cell);
            self.buffer[(x, self.area.y)]
                .set_symbol(" ")
                .set_style(style);
        }
    }

    /// Writes the cluster in its first cell and resets the cells it covers, as
    /// `Buffer::set_stringn` does.
    fn put(&mut self, cell: Cells, width: Cells, cluster: &str, style: Style) {
        let x = self.x(cell);
        let y = self.area.y;

        self.buffer[(x, y)].set_symbol(cluster).set_style(style);
        for continuation in (cell + Cells::ONE).columns_until(cell + width) {
            let continuation_x = self.x(continuation);
            self.buffer[(continuation_x, y)].reset();
        }
    }

    /// The frame column of pane cell `cell`.
    fn x(&self, cell: Cells) -> u16 {
        self.area.x + cell.frame_columns()
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::{Buffer, Cell, CellWidth},
        layout::Rect,
        style::Modifier,
    };

    use super::{Clip, RowPlacement, write_row};
    use crate::{
        parser::parse_patch,
        preferences::DiffGranularity,
        prepared::{PageIndex, PreparedDocument},
        screen::{Cells, MARKER_GUTTER_WIDTH, NonZeroCells, ScreenLines},
        side::Side,
        terminal::rows::PaneRow,
        theme::{ColorDepth, Theme},
    };

    const THEME: &Theme = Theme::of_depth(ColorDepth::TrueColor);

    /// A written row: the buffer it drew into and the sides where it left the pane.
    struct Written {
        buffer: Buffer,
        clip: Clip,
    }

    /// `diff` parsed and prepared.
    fn prepare(diff: &[u8]) -> PreparedDocument {
        let document = parse_patch(diff).expect("valid diff");
        PreparedDocument::prepare(&document)
    }

    /// The text row of the first added line on `prepared`'s first page.
    fn first_added_row(prepared: &PreparedDocument, granularity: DiffGranularity) -> PaneRow<'_> {
        let page = prepared.page(PageIndex::FIRST);
        let (addition, _) = page
            .rows()
            .lines()
            .find(|(_, line)| line.kind().changed_side() == Some(Side::New))
            .expect("the patch adds a line");
        PaneRow::Text(page.row(addition, granularity))
    }

    /// The one-line buffer `row` draws into, `width` cells wide, scrolled right by
    /// `horizontal_offset`.
    fn scrolled_buffer(row: PaneRow<'_>, width: u16, horizontal_offset: Cells) -> Buffer {
        let placement = RowPlacement::Scrolled { horizontal_offset };
        write_placed(row, width, placement).buffer
    }

    /// `row` written at `placement` into a one-line pane `width` cells wide.
    fn write_placed(row: PaneRow<'_>, width: u16, placement: RowPlacement) -> Written {
        let area = Rect::new(0, 0, width, 1);
        let mut buffer = Buffer::empty(area);
        let clip = write_row(&mut buffer, area, row, placement, THEME);
        Written { buffer, clip }
    }

    /// The width of a pane that holds the marker gutter and `payload` cells after it.
    fn pane_width(payload: Cells) -> u16 {
        (MARKER_GUTTER_WIDTH + payload).frame_columns()
    }

    /// The cell of a one-line `buffer` that shows payload column `column` of a row
    /// scrolled right by `horizontal_offset`: past the marker gutter, less the offset.
    fn payload_cell(buffer: &Buffer, column: Cells, horizontal_offset: Cells) -> &Cell {
        let row_column = (MARKER_GUTTER_WIDTH + column)
            .checked_sub(horizontal_offset)
            .expect("the payload column is not scrolled out of the pane");
        &buffer[(row_column.frame_columns(), 0)]
    }

    /// The first cell of a one-line `buffer`.
    fn first_cell(buffer: &Buffer) -> &Cell {
        &buffer[(0, 0)]
    }

    /// The last cell of a one-line `buffer`.
    fn last_cell(buffer: &Buffer) -> &Cell {
        &buffer[(buffer.area.width - 1, 0)]
    }

    /// Every row of the first page, written in line detail.
    fn rendered_rows(prepared: &PreparedDocument, width: u16) -> Vec<String> {
        let page = prepared.page(PageIndex::FIRST);

        page.rows()
            .line_indices()
            .map(|line| page.row(line, DiffGranularity::Line))
            .map(|row| row_text(&scrolled_buffer(PaneRow::Text(row), width, Cells::ZERO)))
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

    /// docs/spec/architecture.md#lossless-and-safe-rendering: lossless rendering
    /// preserves source text through styling.
    #[test]
    fn syntax_highlighting_preserves_source_text() {
        let prepared = prepare(
            b"--- a/example.rs\n+++ b/example.rs\n@@ -1,3 +1,3 @@\n fn example() {\n-    let excluded = ace.excluded_mcp();\n+    let excluded = ace.excluded_mcp()?;\n }\n",
        );

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
    fn preserves_unicode_crlf_blank_context_and_no_newline_markers() {
        let prepared = prepare(
            "--- a/file\r\n+++ b/file\r\n@@ -1,2 +1,2 @@\r\n\r\n-旧\r\n\\ No newline at end of file\r\n+新\r\n\\ No newline at end of file".as_bytes(),
        );

        assert_eq!(
            rendered_rows(&prepared, 120),
            [
                "  --- a/file",
                "  +++ b/file",
                "  @@ -1,2 +1,2 @@",
                "",
                "- 旧",
                "  \\ No newline at end of file",
                "+ 新",
                "  \\ No newline at end of file",
            ]
        );
    }

    /// docs/spec/architecture.md#lossless-and-safe-rendering: style boundaries never
    /// split or drop a grapheme cluster.
    #[test]
    fn keeps_a_cluster_whole_when_its_mark_starts_a_style_segment() {
        let prepared = prepare("--- a/t\n+++ b/t\n@@ -1 +1 @@\n-ก\n+ก่\n".as_bytes());
        let addition = first_added_row(&prepared, DiffGranularity::Character);

        let buffer = scrolled_buffer(addition, 8, Cells::ZERO);

        assert_eq!(
            payload_cell(&buffer, Cells::ZERO, Cells::ZERO).symbol(),
            "ก่"
        );
    }

    /// docs/spec/architecture.md#lossless-and-safe-rendering: a wide grapheme cut by the
    /// left pane edge renders its visible cells blank, keeping later text in its column.
    #[test]
    fn blanks_the_visible_cell_of_a_wide_cluster_cut_by_the_left_edge() {
        let prepared = prepare("--- a/w\n+++ b/w\n@@ -0,0 +1 @@\n+界x\n".as_bytes());
        let addition = first_added_row(&prepared, DiffGranularity::Line);

        // Scrolled past the gutter and the wide cluster's first cell.
        let offset = MARKER_GUTTER_WIDTH + Cells::ONE;
        let buffer = scrolled_buffer(addition, 8, offset);
        let wide_second_cell = payload_cell(&buffer, Cells::ONE, offset);

        assert_eq!(wide_second_cell.symbol(), " ");
        assert_eq!(wide_second_cell.bg, THEME.rows.change.new.background);
        assert_eq!(payload_cell(&buffer, Cells::new(2), offset).symbol(), "x");
    }

    #[test]
    fn leaves_a_wide_cluster_that_does_not_fit_the_right_edge_blank() {
        let prepared = prepare("--- a/w\n+++ b/w\n@@ -0,0 +1 @@\n+ab界\n".as_bytes());
        let addition = first_added_row(&prepared, DiffGranularity::Line);

        let one_cell_short_of_the_wide_cluster = pane_width(Cells::of_text("ab") + Cells::ONE);
        let buffer = scrolled_buffer(addition, one_cell_short_of_the_wide_cluster, Cells::ZERO);

        let wide_start = payload_cell(&buffer, Cells::new(2), Cells::ZERO);

        assert_eq!(payload_cell(&buffer, Cells::ONE, Cells::ZERO).symbol(), "b");
        assert_eq!(wide_start.symbol(), " ");
        assert_eq!(wide_start.bg, THEME.rows.change.new.background);
    }

    #[test]
    fn writes_each_wrapped_line_with_the_marker_on_the_first_only() {
        let prepared = prepare("--- a/w\n+++ b/w\n@@ -0,0 +1 @@\n+abcde界f\n".as_bytes());
        let addition = first_added_row(&prepared, DiffGranularity::Line);
        let wrap_width = 4;
        let wrapped_line = |line| {
            let placement = RowPlacement::Wrapped {
                line: ScreenLines::new(line),
                width: NonZeroCells::of(wrap_width),
            };
            write_placed(addition, pane_width(Cells::new(wrap_width)), placement).buffer
        };

        let first = wrapped_line(0);
        let second = wrapped_line(1);
        let past_the_end = wrapped_line(2);

        assert_eq!(row_text(&first), "+ abcd");
        assert_eq!(row_text(&second), "  e界f");
        assert_eq!(row_text(&past_the_end), "");
        assert_eq!(first_cell(&second).bg, THEME.rows.change.new.background);
        assert_eq!(
            last_cell(&past_the_end).bg,
            THEME.rows.change.new.background
        );
    }

    /// docs/spec/architecture.md#continuity-markers: the row has cells left of the
    /// offset, or continues past the right edge.
    #[test]
    fn reports_the_sides_where_a_row_leaves_the_pane() {
        let prepared = prepare("--- a/c\n+++ b/c\n@@ -0,0 +1 @@\n+abc界\n".as_bytes());
        let row = first_added_row(&prepared, DiffGranularity::Line);
        let clip = |width, horizontal_offset| {
            let placement = RowPlacement::Scrolled { horizontal_offset };
            write_placed(row, width, placement).clip
        };
        let wrapped = RowPlacement::Wrapped {
            line: ScreenLines::new(1),
            width: NonZeroCells::of(1),
        };
        let inside = Clip {
            left: false,
            right: false,
        };
        let past_right = Clip {
            left: false,
            right: true,
        };
        let past_left = Clip {
            left: true,
            right: false,
        };
        let past_both = Clip {
            left: true,
            right: true,
        };

        let fits = pane_width(Cells::of_text("abc界"));

        assert_eq!(clip(fits, Cells::ZERO), inside, "the row fits");
        assert_eq!(
            clip(fits - 1, Cells::ZERO),
            past_right,
            "a wide cluster that does not fit"
        );
        assert_eq!(clip(fits - 3, Cells::ONE), past_both);
        assert_eq!(clip(fits - 1, Cells::ONE), past_left);
        assert_eq!(
            write_placed(row, pane_width(Cells::ONE), wrapped).clip,
            inside,
            "a wrapped line shows its whole part of the row"
        );
    }

    #[test]
    fn fills_an_alignment_peer_with_its_dimmed_change_background() {
        let buffer = scrolled_buffer(PaneRow::AlignmentPeer(Side::Old), 4, Cells::ZERO);

        assert_eq!(row_text(&buffer), "", "an alignment peer writes no text");
        let cell = last_cell(&buffer);
        assert_eq!(cell.bg, THEME.rows.change.old.background);
        assert!(cell.modifier.contains(Modifier::DIM), "{:?}", cell.modifier);
    }
}
