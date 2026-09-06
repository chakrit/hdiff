use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::{
    detail::DetailSpan,
    interaction::Viewport,
    layout::{Layout, SideBySideRow},
    render::RenderedLineKind,
    syntax::{SyntaxClass, SyntaxSpan},
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
    diff_row(&line.bytes, Some(&line.kind), &[], &[], 0)
        .line
        .width()
}

pub(super) fn visible_diff_rows(
    layout: &Layout,
    viewport: &Viewport,
    spans: &[Vec<SyntaxSpan>],
    details: &[Vec<DetailSpan>],
    context_lines: usize,
    color_count: u16,
) -> Vec<DiffRow> {
    layout
        .diff_lines
        .iter()
        .enumerate()
        .skip(viewport.vertical_offset)
        .filter(|(index, _)| layout.shows_unified_row(*index, context_lines))
        .map(|(index, line)| {
            diff_row(
                &line.bytes,
                Some(&line.kind),
                spans.get(index).map(Vec::as_slice).unwrap_or_default(),
                details.get(index).map(Vec::as_slice).unwrap_or_default(),
                color_count,
            )
        })
        .collect()
}

pub(super) fn visible_side_by_side_rows(
    layout: &Layout,
    viewport: &Viewport,
    spans: &[Vec<SyntaxSpan>],
    details: &[Vec<DetailSpan>],
    context_lines: usize,
    color_count: u16,
) -> Vec<VisibleSideBySideRow> {
    layout
        .side_by_side_rows
        .iter()
        .skip(viewport.vertical_offset)
        .filter(|row| layout.shows_side_by_side_row(row, context_lines))
        .map(|row| match row {
            SideBySideRow::Shared(index) => VisibleSideBySideRow::Shared(diff_row_for_side(
                layout,
                *index,
                spans,
                details,
                color_count,
            )),
            SideBySideRow::Paired { before, after } => VisibleSideBySideRow::Paired {
                before: Some(diff_row_for_side(
                    layout,
                    *before,
                    spans,
                    details,
                    color_count,
                )),
                after: Some(diff_row_for_side(
                    layout,
                    *after,
                    spans,
                    details,
                    color_count,
                )),
            },
            SideBySideRow::BeforeOnly(index) => VisibleSideBySideRow::Paired {
                before: Some(diff_row_for_side(
                    layout,
                    *index,
                    spans,
                    details,
                    color_count,
                )),
                after: None,
            },
            SideBySideRow::AfterOnly(index) => VisibleSideBySideRow::Paired {
                before: None,
                after: Some(diff_row_for_side(
                    layout,
                    *index,
                    spans,
                    details,
                    color_count,
                )),
            },
        })
        .collect()
}

fn diff_row_for_side(
    layout: &Layout,
    index: usize,
    spans: &[Vec<SyntaxSpan>],
    details: &[Vec<DetailSpan>],
    color_count: u16,
) -> DiffRow {
    let line = layout
        .rendered_line(index)
        .expect("side-by-side rows reference rendered lines");

    diff_row(
        &line.bytes,
        Some(&line.kind),
        spans.get(index).map(Vec::as_slice).unwrap_or_default(),
        details.get(index).map(Vec::as_slice).unwrap_or_default(),
        color_count,
    )
}

fn diff_row(
    line: &[u8],
    kind: Option<&RenderedLineKind>,
    spans: &[SyntaxSpan],
    details: &[DetailSpan],
    color_count: u16,
) -> DiffRow {
    let without_ending = line
        .strip_suffix(b"\r\n")
        .or_else(|| line.strip_suffix(b"\n"))
        .unwrap_or(line);
    let row_kind = row_kind(kind, line.first());
    let palette = low_contrast_palette(color_count);
    let style = row_style(row_kind, palette, color_count);
    let line = styled_line(without_ending, row_kind, spans, details, color_count);

    DiffRow { line, style }
}

#[derive(Clone, Copy)]
enum RowKind {
    Metadata,
    HunkHeader,
    Context,
    Addition,
    Deletion,
}

fn row_kind(kind: Option<&RenderedLineKind>, marker: Option<&u8>) -> RowKind {
    match kind {
        Some(RenderedLineKind::HunkHeader) => RowKind::HunkHeader,
        Some(RenderedLineKind::Record(_)) => match marker {
            Some(b'+') => RowKind::Addition,
            Some(b'-') => RowKind::Deletion,
            _ => RowKind::Context,
        },
        Some(
            RenderedLineKind::Text | RenderedLineKind::Metadata | RenderedLineKind::FileHeader,
        )
        | None => RowKind::Metadata,
    }
}

fn styled_line(
    bytes: &[u8],
    row_kind: RowKind,
    syntax: &[SyntaxSpan],
    details: &[DetailSpan],
    color_count: u16,
) -> Line<'static> {
    let text = String::from_utf8_lossy(bytes).into_owned();
    let palette = low_contrast_palette(color_count);
    let mut rendered = Vec::new();
    let mut cursor = match row_kind {
        RowKind::Addition => {
            rendered.push(Span::styled(
                text[..1].to_owned(),
                Style::default().fg(palette.addition_marker),
            ));
            rendered.push(Span::raw(" "));
            1
        }
        RowKind::Deletion => {
            rendered.push(Span::styled(
                text[..1].to_owned(),
                Style::default().fg(palette.deletion_marker),
            ));
            rendered.push(Span::raw(" "));
            1
        }
        RowKind::Context | RowKind::Metadata | RowKind::HunkHeader => {
            rendered.push(Span::raw("  "));
            usize::from(matches!(row_kind, RowKind::Context))
        }
    };
    let mut spans = match details.is_empty() {
        true => syntax
            .iter()
            .map(|span| (span.start, span.end, false, span.class))
            .collect::<Vec<_>>(),
        false => details
            .iter()
            .map(|span| (span.start, span.end, true, SyntaxClass::Variable))
            .collect::<Vec<_>>(),
    };
    spans.sort_by_key(|(start, _, _, _)| *start);
    for (span_start, span_end, detail, class) in spans {
        let source_offset = usize::from(matches!(
            row_kind,
            RowKind::Context | RowKind::Addition | RowKind::Deletion
        ));
        let start = span_start + source_offset;
        let end = span_end + source_offset;
        if start > cursor {
            rendered.push(Span::raw(text[cursor..start].to_owned()));
        }
        let style = match detail {
            true => detail_style(row_kind, palette),
            false => Style::default().fg(DEFAULT_SYNTAX_THEME.color(class, color_count)),
        };
        rendered.push(Span::styled(text[start..end].to_owned(), style));
        cursor = end;
    }
    if cursor < text.len() {
        rendered.push(Span::raw(text[cursor..].to_owned()));
    }

    Line::from(rendered).style(row_style(row_kind, palette, color_count))
}

fn detail_style(row_kind: RowKind, palette: LowContrastPalette) -> Style {
    let color = match row_kind {
        RowKind::Addition => palette.addition_marker,
        RowKind::Deletion => palette.deletion_marker,
        RowKind::Context | RowKind::HunkHeader | RowKind::Metadata => palette.context_body,
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
        RowKind::Context => Style::default().fg(palette.context_body),
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

    use super::{hunk_header_color, low_contrast_palette, maximum_visible_row_width};
    use crate::{interaction::DiffLayout, layout::file_layout, parser::parse_unified_diff};

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
