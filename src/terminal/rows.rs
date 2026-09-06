use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::{
    interaction::{DiffGranularity, Viewport},
    layout::{Layout, SideBySideRow},
    prepared::PreparedFile,
    styling::{RowKind, StyledRow, TextStyle, payload},
    theme::DEFAULT_SYNTAX_THEME,
};

#[derive(Clone)]
pub(super) struct DiffRow {
    pub(super) line: Line<'static>,
    pub(super) style: Style,
}

pub(super) fn aligned_pair(
    before: Option<DiffRow>,
    after: Option<DiffRow>,
    color_count: u16,
) -> Option<(DiffRow, DiffRow)> {
    match (before, after) {
        (Some(before), Some(after)) => Some((before, after)),
        (Some(before), None) => {
            let after = alignment_peer(RowKind::Addition, color_count);
            Some((before, after))
        }
        (None, Some(after)) => {
            let before = alignment_peer(RowKind::Deletion, color_count);
            Some((before, after))
        }
        (None, None) => None,
    }
}

fn alignment_peer(row_kind: RowKind, color_count: u16) -> DiffRow {
    DiffRow {
        line: Line::raw(""),
        style: row_style(row_kind, low_contrast_palette(color_count), color_count),
    }
}

pub(super) enum VisibleSideBySideRow {
    Shared(DiffRow),
    Paired {
        before: Option<DiffRow>,
        after: Option<DiffRow>,
    },
}

pub(super) fn maximum_visible_row_width(
    layout: &Layout,
    display_layout: crate::interaction::DiffLayout,
    context_lines: usize,
) -> usize {
    match display_layout {
        crate::interaction::DiffLayout::Unified => layout
            .diff_lines
            .iter()
            .enumerate()
            .filter(|(index, _)| layout.shows_unified_row(*index, context_lines))
            .map(|(_, line)| rendered_line_width(line))
            .max()
            .unwrap_or_default(),
        crate::interaction::DiffLayout::Vertical | crate::interaction::DiffLayout::Stacked => {
            layout
                .side_by_side_rows
                .iter()
                .filter(|row| layout.shows_side_by_side_row(row, context_lines))
                .map(|row| side_by_side_row_width(layout, row))
                .max()
                .unwrap_or_default()
        }
    }
}

fn side_by_side_row_width(layout: &Layout, row: &SideBySideRow) -> usize {
    match row {
        SideBySideRow::Shared(index)
        | SideBySideRow::BeforeOnly(index)
        | SideBySideRow::AfterOnly(index) => rendered_line_width(
            layout
                .rendered_line(*index)
                .expect("side-by-side rows reference rendered lines"),
        ),
        SideBySideRow::Paired { before, after } => [before, after]
            .into_iter()
            .map(|index| {
                rendered_line_width(
                    layout
                        .rendered_line(*index)
                        .expect("side-by-side rows reference rendered lines"),
                )
            })
            .max()
            .expect("paired rows contain both rendered lines"),
    }
}

fn rendered_line_width(line: &crate::render::RenderedLine) -> usize {
    Line::raw(payload(line)).width() + 2
}

pub(super) fn visible_diff_rows(
    file: &PreparedFile,
    viewport: &Viewport,
    granularity: DiffGranularity,
    context_lines: usize,
    color_count: u16,
) -> Vec<DiffRow> {
    let layout = file.layout();

    layout
        .diff_lines
        .iter()
        .enumerate()
        .skip(viewport.vertical_offset)
        .filter(|(index, _)| layout.shows_unified_row(*index, context_lines))
        .map(|(index, _)| diff_row(file.row(index, granularity), color_count))
        .collect()
}

pub(super) fn visible_side_by_side_rows(
    file: &PreparedFile,
    viewport: &Viewport,
    granularity: DiffGranularity,
    context_lines: usize,
    color_count: u16,
) -> Vec<VisibleSideBySideRow> {
    let layout = file.layout();

    layout
        .side_by_side_rows
        .iter()
        .skip(viewport.vertical_offset)
        .filter(|row| layout.shows_side_by_side_row(row, context_lines))
        .map(|row| match row {
            SideBySideRow::Shared(index) => {
                VisibleSideBySideRow::Shared(diff_row(file.row(*index, granularity), color_count))
            }
            SideBySideRow::Paired { before, after } => VisibleSideBySideRow::Paired {
                before: Some(diff_row(file.row(*before, granularity), color_count)),
                after: Some(diff_row(file.row(*after, granularity), color_count)),
            },
            SideBySideRow::BeforeOnly(index) => VisibleSideBySideRow::Paired {
                before: Some(diff_row(file.row(*index, granularity), color_count)),
                after: None,
            },
            SideBySideRow::AfterOnly(index) => VisibleSideBySideRow::Paired {
                before: None,
                after: Some(diff_row(file.row(*index, granularity), color_count)),
            },
        })
        .collect()
}

fn diff_row(row: StyledRow<'_>, color_count: u16) -> DiffRow {
    let palette = low_contrast_palette(color_count);
    let style = row_style(row.kind, palette, color_count);
    let line = styled_line(row, color_count);

    DiffRow { line, style }
}

fn styled_line(row: StyledRow<'_>, color_count: u16) -> Line<'static> {
    let palette = low_contrast_palette(color_count);
    let mut rendered = match row.kind {
        RowKind::Addition => vec![
            Span::styled("+", Style::default().fg(palette.addition_marker)),
            Span::raw(" "),
        ],
        RowKind::Deletion => vec![
            Span::styled("-", Style::default().fg(palette.deletion_marker)),
            Span::raw(" "),
        ],
        RowKind::Context | RowKind::Raw | RowKind::Metadata | RowKind::HunkHeader => {
            vec![Span::raw("  ")]
        }
    };

    rendered.extend(row.segments().map(|(text, style)| {
        let style = match style {
            TextStyle::Plain => Style::default(),
            TextStyle::Syntax(class) => {
                Style::default().fg(DEFAULT_SYNTAX_THEME.color(class, color_count))
            }
            TextStyle::Detail => detail_style(row.kind, palette),
        };
        Span::styled(text.to_owned(), style)
    }));

    Line::from(rendered).style(row_style(row.kind, palette, color_count))
}

fn detail_style(row_kind: RowKind, palette: LowContrastPalette) -> Style {
    let color = match row_kind {
        RowKind::Addition => palette.addition_marker,
        RowKind::Deletion => palette.deletion_marker,
        RowKind::Context | RowKind::Raw | RowKind::HunkHeader | RowKind::Metadata => {
            palette.context_body
        }
    };

    Style::default().fg(color).add_modifier(Modifier::DIM)
}

fn row_style(row_kind: RowKind, palette: LowContrastPalette, color_count: u16) -> Style {
    match row_kind {
        RowKind::Addition => Style::default()
            .bg(palette.addition_background)
            .fg(palette.addition_body),
        RowKind::Deletion => Style::default()
            .bg(palette.deletion_background)
            .fg(palette.deletion_body)
            .add_modifier(Modifier::DIM),
        RowKind::Context | RowKind::Raw => Style::default().fg(palette.context_body),
        RowKind::HunkHeader => Style::default().fg(hunk_header_color(color_count)),
        RowKind::Metadata => Style::default(),
    }
}

fn hunk_header_color(color_count: u16) -> Color {
    match color_count {
        u16::MAX => Color::Rgb(100, 155, 175),
        256.. => Color::Indexed(109),
        _ => Color::Cyan,
    }
}

#[derive(Clone, Copy)]
struct LowContrastPalette {
    addition_background: Color,
    addition_body: Color,
    deletion_background: Color,
    deletion_body: Color,
    context_body: Color,
    deletion_marker: Color,
    addition_marker: Color,
}

fn low_contrast_palette(color_count: u16) -> LowContrastPalette {
    match color_count {
        u16::MAX => LowContrastPalette {
            addition_background: Color::Rgb(35, 73, 43),
            addition_body: Color::Rgb(220, 235, 220),
            deletion_background: Color::Rgb(50, 30, 30),
            deletion_body: Color::Rgb(150, 150, 150),
            context_body: Color::Rgb(180, 180, 180),
            deletion_marker: Color::Rgb(206, 74, 74),
            addition_marker: Color::Rgb(112, 195, 115),
        },
        256.. => LowContrastPalette {
            addition_background: Color::Indexed(22),
            addition_body: Color::Indexed(255),
            deletion_background: Color::Indexed(52),
            deletion_body: Color::Indexed(245),
            context_body: Color::Indexed(250),
            deletion_marker: Color::Indexed(167),
            addition_marker: Color::Indexed(71),
        },
        _ => LowContrastPalette {
            addition_background: Color::Green,
            addition_body: Color::White,
            deletion_background: Color::Red,
            deletion_body: Color::DarkGray,
            context_body: Color::Gray,
            deletion_marker: Color::Red,
            addition_marker: Color::Green,
        },
    }
}

pub(super) fn inner_separator_color(color_count: u16) -> Color {
    match color_count {
        u16::MAX => Color::Rgb(95, 95, 95),
        256.. => Color::Indexed(238),
        _ => Color::DarkGray,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::{
        hunk_header_color, low_contrast_palette, maximum_visible_row_width, visible_diff_rows,
    };
    use crate::{
        interaction::{DiffGranularity, DiffLayout, Viewport},
        layout::file_layout,
        parser::parse_unified_diff,
        prepared::PreparedDocument,
    };

    #[test]
    fn syntax_highlighting_preserves_source_text() {
        let document = parse_unified_diff(
            b"--- a/example.rs\n+++ b/example.rs\n@@ -1,3 +1,3 @@\n fn example() {\n-    let excluded = ace.excluded_mcp();\n+    let excluded = ace.excluded_mcp()?;\n }\n",
        )
        .expect("valid Rust method-call diff");
        let prepared = PreparedDocument::prepare(&document);
        let file = prepared.file(0).expect("prepared example file");

        let rows = visible_diff_rows(
            file,
            &Viewport::new(120, 12),
            DiffGranularity::Line,
            3,
            u16::MAX,
        );
        let text = rows
            .iter()
            .map(|row| row.line.to_string())
            .collect::<Vec<_>>();

        // architecture.md: lossless rendering preserves source text through styling.
        assert_eq!(
            text,
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
        let document = parse_unified_diff(
            "--- a/file\r\n+++ b/file\r\n@@ -1 +1 @@\r\n-旧\r\n\\ No newline at end of file\r\n\r\n界\r\n+新\r\n\\ No newline at end of file".as_bytes(),
        )
        .expect("valid diff with Unicode and raw records");
        let prepared = PreparedDocument::prepare(&document);
        let file = prepared.file(0).expect("prepared file");

        let rows = visible_diff_rows(
            file,
            &Viewport::new(120, 12),
            DiffGranularity::Line,
            3,
            u16::MAX,
        );
        let text = rows
            .iter()
            .map(|row| row.line.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            text,
            [
                "  --- a/file",
                "  +++ b/file",
                "  @@ -1 +1 @@",
                "- 旧",
                "  \\ No newline at end of file",
                "  ",
                "  界",
                "+ 新",
                "  \\ No newline at end of file",
            ]
        );
    }

    #[test]
    fn measures_rendered_rows_in_terminal_cells() {
        let document = parse_unified_diff(
            "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+界界界界界界\n".as_bytes(),
        )
        .expect("valid Unicode diff");
        let layout = file_layout(&document.file_views().next().expect("first file"));

        assert_eq!(
            maximum_visible_row_width(&layout, DiffLayout::Unified, 3),
            14
        );
    }

    #[test]
    fn falls_back_from_truecolor_to_256_and_basic_low_contrast_colors() {
        assert_eq!(
            low_contrast_palette(u16::MAX).addition_background,
            Color::Rgb(35, 73, 43)
        );
        assert_eq!(
            low_contrast_palette(256).addition_marker,
            Color::Indexed(71)
        );
        assert_eq!(low_contrast_palette(8).deletion_marker, Color::Red);
    }

    #[test]
    fn falls_back_from_truecolor_to_256_and_basic_hunk_header_colors() {
        assert_eq!(hunk_header_color(u16::MAX), Color::Rgb(100, 155, 175));
        assert_eq!(hunk_header_color(256), Color::Indexed(109));
        assert_eq!(hunk_header_color(8), Color::Cyan);
    }
}
