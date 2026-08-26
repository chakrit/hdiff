use std::{io, time::Duration};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::available_color_count,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout as RatatuiLayout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::{
    detail::DetailSpan,
    interaction::{
        self, DiffLayout, Input, Interaction, NavigationBounds, Transition, ViewPreferences,
        Viewport,
    },
    layout::{FileListRow, Layout, PaneLayout, SideBySideLine, SideBySideRow, pane_layout},
    prepared::{PreparedDocument, PreparedFile},
    render::RenderedLineKind,
    syntax::{SyntaxClass, SyntaxSpan},
};

#[cfg(test)]
use crate::{detail::detail_rows, document::DiffDocument, syntax::SyntaxHighlighter};

const MINIMUM_FILE_LIST_HEIGHT: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStage {
    RawMode,
    AlternateScreen,
    HiddenCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Finite,
    Interactive,
}

pub fn mode_for_output(is_terminal: bool) -> OutputMode {
    match is_terminal {
        true => OutputMode::Interactive,
        false => OutputMode::Finite,
    }
}

pub fn run_interactive(document: &PreparedDocument) -> io::Result<()> {
    let mut session = TerminalSession::start()?;
    let result = run_loop(&mut session, document, available_color_count());
    let cleanup_result = session.cleanup();

    combine_results(result, cleanup_result)
}

fn combine_results(result: io::Result<()>, cleanup_result: io::Result<()>) -> io::Result<()> {
    match (result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup_error)) => Err(io::Error::other(format!(
            "interactive session failed: {error}; cleanup failed: {cleanup_error}"
        ))),
    }
}

fn cleanup_failure(error: io::Error, cleanup_result: io::Result<()>) -> io::Error {
    match cleanup_result {
        Ok(()) => error,
        Err(cleanup_error) => io::Error::other(format!(
            "interactive session failed: {error}; cleanup failed: {cleanup_error}"
        )),
    }
}

fn run_loop(
    session: &mut TerminalSession,
    document: &PreparedDocument,
    color_count: u16,
) -> io::Result<()> {
    let (_, height) = terminal::size()?;
    let mut interaction = Interaction {
        selected_file: 0,
        preferences: ViewPreferences::line(DiffLayout::Unified),
        viewport: Viewport {
            offset: 0,
            height: usize::from(height),
        },
    };
    draw(session.terminal_mut(), document, &interaction, color_count)?;

    let mut pending_event = None;
    loop {
        let event = pending_event.take().unwrap_or(event::read()?);
        let (event, next_pending_event) = coalesce_resize_event(event)?;
        pending_event = next_pending_event;
        let Some(input) = input_for_event(event) else {
            continue;
        };

        let transition = {
            let bounds = NavigationBounds {
                file_count: document.file_count(),
                unified_line_count: document
                    .file(interaction.selected_file)
                    .map(|file| {
                        file.layout
                            .visible_unified_line_count(interaction.preferences.context_lines)
                    })
                    .unwrap_or_default(),
                paired_line_count: document
                    .file(interaction.selected_file)
                    .map(|file| {
                        file.layout
                            .visible_side_by_side_line_count(interaction.preferences.context_lines)
                    })
                    .unwrap_or_default(),
            };
            interaction::transition_interaction(&interaction, input, &bounds)
        };
        match transition {
            Transition::Exit => return Ok(()),
            Transition::Redraw(_) => unreachable!("interactive transition retains selection"),
            Transition::RedrawInteraction(next_interaction) => {
                interaction = next_interaction;
                draw(session.terminal_mut(), document, &interaction, color_count)?;
            }
        }
    }
}

fn coalesce_resize_event(
    event: CrosstermEvent,
) -> io::Result<(CrosstermEvent, Option<CrosstermEvent>)> {
    let CrosstermEvent::Resize(..) = event else {
        return Ok((event, None));
    };
    let mut latest_resize = event;

    while event::poll(Duration::ZERO)? {
        let next_event = event::read()?;
        if matches!(next_event, CrosstermEvent::Resize(..)) {
            latest_resize = next_event;
        } else {
            return Ok((latest_resize, Some(next_event)));
        }
    }

    Ok((latest_resize, None))
}

fn input_for_event(event: CrosstermEvent) -> Option<Input> {
    match event {
        CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => input_for_key(key),
        CrosstermEvent::Resize(width, height) => Some(Input::Resize { width, height }),
        _ => None,
    }
}

fn input_for_key(key: KeyEvent) -> Option<Input> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => Some(Input::Quit),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Input::Interrupt),
        (KeyCode::Char('j') | KeyCode::Down, _) => Some(Input::Down),
        (KeyCode::Char('k') | KeyCode::Up, _) => Some(Input::Up),
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => Some(Input::HalfPageDown),
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => Some(Input::HalfPageUp),
        (KeyCode::Char('g'), _) => Some(Input::Top),
        (KeyCode::Char('G'), _) => Some(Input::Bottom),
        (KeyCode::Char('v'), _) => Some(Input::NextLayout),
        (KeyCode::Char('c'), _) => Some(Input::ToggleGranularity),
        (KeyCode::Char('+'), _) => Some(Input::IncreaseContext),
        (KeyCode::Char('-'), _) => Some(Input::DecreaseContext),
        (KeyCode::Tab, KeyModifiers::SHIFT) => Some(Input::PreviousFile),
        (KeyCode::Tab, _) => Some(Input::NextFile),
        (KeyCode::BackTab, _) => Some(Input::PreviousFile),
        _ => None,
    }
}

fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    document: &PreparedDocument,
    interaction: &Interaction,
    color_count: u16,
) -> io::Result<()> {
    let Some(file) = document.file(interaction.selected_file) else {
        return terminal
            .draw(|frame| frame.render_widget(Paragraph::new(""), frame.area()))
            .map(|_| ());
    };
    let files = document.file_rows(interaction.selected_file);

    terminal
        .draw(|frame| render_prepared_frame(frame, &files, file, interaction, color_count))
        .map(|_| ())
}

#[cfg(test)]
fn render_frame(
    frame: &mut Frame,
    document: &DiffDocument,
    layout: &Layout,
    viewport: &Viewport,
    syntax: &mut SyntaxHighlighter,
    color_count: u16,
) {
    let preferences = ViewPreferences::line(DiffLayout::Unified);

    render_frame_with_layout(
        frame,
        document,
        layout,
        viewport,
        &preferences,
        syntax,
        color_count,
    );
}

#[cfg(test)]
fn render_frame_with_layout(
    frame: &mut Frame,
    document: &DiffDocument,
    layout: &Layout,
    viewport: &Viewport,
    preferences: &ViewPreferences,
    syntax: &mut SyntaxHighlighter,
    color_count: u16,
) {
    let area = frame.area();
    let selected = layout.files.iter().position(|file| file.selected);
    let file = selected.and_then(|index| document.files.get(index));
    let details = detail_rows(layout, preferences.granularity);
    let spans = file
        .map(|file| syntax.highlight_file(file))
        .unwrap_or_default();
    let rows = visible_diff_rows(
        layout,
        viewport,
        &spans,
        &details,
        preferences.context_lines,
        color_count,
    );
    let side_by_side_rows = visible_side_by_side_rows(
        layout,
        viewport,
        &spans,
        &details,
        preferences.context_lines,
        color_count,
    );
    let display = DisplayRows {
        unified: rows,
        side_by_side: side_by_side_rows,
        layout: preferences.layout,
        color_count,
    };

    match pane_layout(area.width, area.height) {
        PaneLayout::TooNarrow => frame.render_widget(Paragraph::new("screen too narrow"), area),
        PaneLayout::TooShort => frame.render_widget(Paragraph::new("screen too short"), area),
        PaneLayout::DiffOnly => render_display_layout(frame, display, area),
        PaneLayout::Split {
            file_list_width,
            separator_padding,
        } => render_split_panes(
            frame,
            &layout.files,
            area,
            file_list_width,
            separator_padding,
            display,
        ),
    }
}

fn render_prepared_frame(
    frame: &mut Frame,
    files: &[FileListRow],
    file: &PreparedFile,
    interaction: &Interaction,
    color_count: u16,
) {
    let area = frame.area();
    let layout = &file.layout;
    let details = file.details(interaction.preferences.granularity);
    let rows = visible_diff_rows(
        layout,
        &interaction.viewport,
        file.syntax(),
        details,
        interaction.preferences.context_lines,
        color_count,
    );
    let side_by_side_rows = visible_side_by_side_rows(
        layout,
        &interaction.viewport,
        file.syntax(),
        details,
        interaction.preferences.context_lines,
        color_count,
    );
    let display = DisplayRows {
        unified: rows,
        side_by_side: side_by_side_rows,
        layout: interaction.preferences.layout,
        color_count,
    };

    match pane_layout(area.width, area.height) {
        PaneLayout::TooNarrow => frame.render_widget(Paragraph::new("screen too narrow"), area),
        PaneLayout::TooShort => frame.render_widget(Paragraph::new("screen too short"), area),
        PaneLayout::DiffOnly => render_display_layout(frame, display, area),
        PaneLayout::Split {
            file_list_width,
            separator_padding,
        } => render_split_panes(
            frame,
            files,
            area,
            file_list_width,
            separator_padding,
            display,
        ),
    }
}

fn render_split_panes(
    frame: &mut Frame,
    files: &[FileListRow],
    area: ratatui::layout::Rect,
    file_list_width: u16,
    separator_padding: u16,
    display: DisplayRows,
) {
    let panes = RatatuiLayout::horizontal([
        Constraint::Length(file_list_width),
        Constraint::Length(separator_padding),
        Constraint::Length(1),
        Constraint::Length(separator_padding),
        Constraint::Min(1),
    ])
    .split(area);
    let hints = footer_hints();
    let footer_height = area
        .height
        .saturating_sub(MINIMUM_FILE_LIST_HEIGHT)
        .min(hints.len() as u16);
    let file_list_areas = RatatuiLayout::vertical([
        Constraint::Min(MINIMUM_FILE_LIST_HEIGHT),
        Constraint::Length(footer_height),
    ])
    .split(panes[0]);
    let mut selected_file = ListState::default();
    selected_file.select(files.iter().position(|file| file.selected));
    let chrome = chrome_palette(display.color_count);
    let files = files
        .iter()
        .map(|file| {
            ListItem::new(Line::styled(
                file.label.as_str(),
                Style::default().fg(chrome.file_list),
            ))
        })
        .collect::<Vec<_>>();
    let file_list = List::new(files).highlight_symbol("· ").highlight_style(
        Style::default()
            .fg(chrome.selected_file)
            .bg(chrome.selected_file_background),
    );
    let separator = Text::from(
        (0..area.height)
            .map(|_| Line::styled("│", Style::default().fg(chrome.separator)))
            .collect::<Vec<_>>(),
    );
    let hints = Paragraph::new(hints).style(Style::default().fg(chrome.footer));

    frame.render_stateful_widget(file_list, file_list_areas[0], &mut selected_file);
    frame.render_widget(hints, file_list_areas[1]);
    frame.render_widget(Paragraph::new(separator), panes[2]);
    render_display_layout(frame, display, panes[4]);
}

fn render_diff_rows(frame: &mut Frame, rows: Vec<DiffRow>, area: ratatui::layout::Rect) {
    for (index, row) in rows.into_iter().take(area.height.into()).enumerate() {
        let row_area = ratatui::layout::Rect::new(area.x, area.y + index as u16, area.width, 1);

        frame.render_widget(Paragraph::new(row.line).style(row.style), row_area);
    }
}

fn render_display_layout(frame: &mut Frame, display: DisplayRows, area: ratatui::layout::Rect) {
    match display.layout {
        DiffLayout::Unified => render_diff_rows(frame, display.unified, area),
        DiffLayout::Vertical => {
            render_vertical_rows(frame, display.side_by_side, area, display.color_count)
        }
        DiffLayout::Stacked => {
            render_stacked_rows(frame, display.side_by_side, area, display.color_count)
        }
    }
}

fn render_vertical_rows(
    frame: &mut Frame,
    rows: Vec<VisibleSideBySideRow>,
    area: ratatui::layout::Rect,
    color_count: u16,
) {
    let panes = RatatuiLayout::horizontal([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Percentage(50),
    ])
    .split(area);
    let separator_style = Style::default().fg(inner_separator_color(color_count));

    for (index, row) in rows.into_iter().take(area.height.into()).enumerate() {
        let row_area = ratatui::layout::Rect::new(area.x, area.y + index as u16, area.width, 1);
        let before_area = ratatui::layout::Rect::new(panes[0].x, row_area.y, panes[0].width, 1);
        let separator_area = ratatui::layout::Rect::new(panes[1].x, row_area.y, 1, 1);
        let after_area = ratatui::layout::Rect::new(panes[2].x, row_area.y, panes[2].width, 1);
        match row {
            VisibleSideBySideRow::Shared(row) => {
                render_diff_rows(frame, vec![row.clone()], before_area);
                frame.render_widget(
                    Paragraph::new(Line::styled("│", separator_style)),
                    separator_area,
                );
                render_diff_rows(frame, vec![row], after_area);
            }
            VisibleSideBySideRow::Paired { before, after } => {
                let Some((before, after)) = aligned_pair(before, after, color_count) else {
                    continue;
                };

                render_diff_rows(frame, vec![before], before_area);
                frame.render_widget(
                    Paragraph::new(Line::styled("│", separator_style)),
                    separator_area,
                );
                render_diff_rows(frame, vec![after], after_area);
            }
        }
    }
}

fn render_stacked_rows(
    frame: &mut Frame,
    rows: Vec<VisibleSideBySideRow>,
    area: ratatui::layout::Rect,
    color_count: u16,
) {
    let panes = RatatuiLayout::vertical([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Percentage(50),
    ])
    .split(area);
    let mut before_rows = Vec::new();
    let mut after_rows = Vec::new();

    for row in rows {
        match row {
            VisibleSideBySideRow::Shared(row) => {
                before_rows.push(row.clone());
                after_rows.push(row);
            }
            VisibleSideBySideRow::Paired { before, after } => {
                let Some((before, after)) = aligned_pair(before, after, color_count) else {
                    continue;
                };

                before_rows.push(before);
                after_rows.push(after);
            }
        }
    }

    let separator = "─".repeat(usize::from(area.width));
    frame.render_widget(
        Paragraph::new(Line::styled(
            separator,
            Style::default().fg(inner_separator_color(color_count)),
        )),
        panes[1],
    );
    render_diff_rows(frame, before_rows, panes[0]);
    render_diff_rows(frame, after_rows, panes[2]);
}

fn footer_hints() -> Vec<Line<'static>> {
    vec![
        Line::raw("   k"),
        Line::raw(" h · l  movement"),
        Line::raw("   j"),
        Line::raw(""),
        Line::raw("  ^U    page up"),
        Line::raw("  ^D    page down"),
        Line::raw("  g·G   top/bottom"),
        Line::raw("  v    cycle layout"),
        Line::raw("  c    character detail"),
        Line::raw(" +/-   context"),
        Line::raw("(⇧)Tab  next/prev file"),
        Line::raw("   q    exit"),
    ]
}

#[derive(Clone)]
struct DiffRow {
    line: Line<'static>,
    style: Style,
}

fn aligned_pair(
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

enum VisibleSideBySideRow {
    Shared(DiffRow),
    Paired {
        before: Option<DiffRow>,
        after: Option<DiffRow>,
    },
}

struct DisplayRows {
    unified: Vec<DiffRow>,
    side_by_side: Vec<VisibleSideBySideRow>,
    layout: DiffLayout,
    color_count: u16,
}

fn visible_diff_rows(
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
        .skip(viewport.offset)
        .filter(|(index, _)| layout.shows_unified_row(*index, context_lines))
        .map(|(index, line)| {
            diff_row(
                line,
                layout.line_kinds.get(index),
                spans.get(index).map(Vec::as_slice).unwrap_or_default(),
                details.get(index).map(Vec::as_slice).unwrap_or_default(),
                color_count,
            )
        })
        .collect()
}

fn visible_side_by_side_rows(
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
        .skip(viewport.offset)
        .filter(|row| side_by_side_row_visible(layout, row, context_lines))
        .map(|row| match row {
            SideBySideRow::Shared(line) => {
                VisibleSideBySideRow::Shared(diff_row_for_side(line, spans, details, color_count))
            }
            SideBySideRow::Paired { before, after } => VisibleSideBySideRow::Paired {
                before: before
                    .as_ref()
                    .map(|line| diff_row_for_side(line, spans, details, color_count)),
                after: after
                    .as_ref()
                    .map(|line| diff_row_for_side(line, spans, details, color_count)),
            },
        })
        .collect()
}

fn side_by_side_row_visible(layout: &Layout, row: &SideBySideRow, context_lines: usize) -> bool {
    layout.shows_side_by_side_row(row, context_lines)
}

fn diff_row_for_side(
    line: &SideBySideLine,
    spans: &[Vec<SyntaxSpan>],
    details: &[Vec<DetailSpan>],
    color_count: u16,
) -> DiffRow {
    diff_row(
        &line.bytes,
        Some(&line.kind),
        spans
            .get(line.source_index)
            .map(Vec::as_slice)
            .unwrap_or_default(),
        details
            .get(line.source_index)
            .map(Vec::as_slice)
            .unwrap_or_default(),
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
        Some(RenderedLineKind::Metadata | RenderedLineKind::FileHeader) | None => RowKind::Metadata,
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
            false => Style::default().fg(syntax_color(class, color_count)),
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

#[derive(Clone, Copy)]
struct ChromePalette {
    file_list: Color,
    selected_file: Color,
    selected_file_background: Color,
    separator: Color,
    footer: Color,
}

fn chrome_palette(color_count: u16) -> ChromePalette {
    match color_count {
        u16::MAX => ChromePalette {
            file_list: Color::Rgb(160, 160, 160),
            selected_file: Color::Rgb(220, 220, 220),
            selected_file_background: Color::Rgb(65, 65, 65),
            separator: Color::Rgb(125, 125, 125),
            footer: Color::Rgb(110, 110, 110),
        },
        256.. => ChromePalette {
            file_list: Color::Indexed(246),
            selected_file: Color::Indexed(255),
            selected_file_background: Color::Indexed(238),
            separator: Color::Indexed(243),
            footer: Color::Indexed(240),
        },
        _ => ChromePalette {
            file_list: Color::Gray,
            selected_file: Color::White,
            selected_file_background: Color::DarkGray,
            separator: Color::DarkGray,
            footer: Color::DarkGray,
        },
    }
}

fn inner_separator_color(color_count: u16) -> Color {
    match color_count {
        u16::MAX => Color::Rgb(95, 95, 95),
        256.. => Color::Indexed(238),
        _ => Color::DarkGray,
    }
}

fn syntax_color(class: SyntaxClass, color_count: u16) -> Color {
    match color_count {
        u16::MAX => match class {
            SyntaxClass::Keyword | SyntaxClass::Constant => Color::Rgb(86, 156, 214),
            SyntaxClass::String => Color::Rgb(206, 145, 120),
            SyntaxClass::Comment => Color::Rgb(106, 153, 85),
            SyntaxClass::Number => Color::Rgb(181, 206, 168),
            SyntaxClass::Function => Color::Rgb(220, 220, 170),
            SyntaxClass::Type => Color::Rgb(78, 201, 176),
            SyntaxClass::Operator => Color::Rgb(212, 212, 212),
            SyntaxClass::Property => Color::Rgb(156, 220, 254),
            SyntaxClass::Variable => Color::Rgb(220, 220, 220),
        },
        256.. => match class {
            SyntaxClass::Keyword | SyntaxClass::Constant => Color::Indexed(75),
            SyntaxClass::String => Color::Indexed(180),
            SyntaxClass::Comment => Color::Indexed(107),
            SyntaxClass::Number => Color::Indexed(151),
            SyntaxClass::Function => Color::Indexed(187),
            SyntaxClass::Type => Color::Indexed(80),
            SyntaxClass::Operator => Color::Indexed(252),
            SyntaxClass::Property => Color::Indexed(153),
            SyntaxClass::Variable => Color::Indexed(253),
        },
        _ => match class {
            SyntaxClass::Keyword | SyntaxClass::Constant => Color::Blue,
            SyntaxClass::String => Color::Yellow,
            SyntaxClass::Comment | SyntaxClass::Number => Color::Green,
            SyntaxClass::Function | SyntaxClass::Type => Color::Cyan,
            SyntaxClass::Operator | SyntaxClass::Variable => Color::White,
            SyntaxClass::Property => Color::Magenta,
        },
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    stages: Vec<SetupStage>,
}

impl TerminalSession {
    fn start() -> io::Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        let mut session = TerminalSession {
            terminal,
            stages: Vec::new(),
        };

        terminal::enable_raw_mode()?;
        session.stages.push(SetupStage::RawMode);
        if let Err(error) = execute!(session.terminal.backend_mut(), EnterAlternateScreen) {
            return Err(cleanup_failure(error, session.cleanup()));
        }
        session.stages.push(SetupStage::AlternateScreen);
        if let Err(error) = execute!(session.terminal.backend_mut(), Hide) {
            return Err(cleanup_failure(error, session.cleanup()));
        }
        session.stages.push(SetupStage::HiddenCursor);

        Ok(session)
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }

    fn cleanup(&mut self) -> io::Result<()> {
        let mut errors = Vec::new();
        for stage in cleanup_order(&self.stages) {
            let result = match stage {
                SetupStage::HiddenCursor => execute!(self.terminal.backend_mut(), Show),
                SetupStage::AlternateScreen => {
                    execute!(self.terminal.backend_mut(), LeaveAlternateScreen)
                }
                SetupStage::RawMode => terminal::disable_raw_mode(),
            };
            if let Err(error) = result {
                errors.push(error.to_string());
            }
        }
        self.stages.clear();

        match errors.is_empty() {
            true => Ok(()),
            false => Err(io::Error::other(errors.join("; "))),
        }
    }
}

pub fn cleanup_order(stages: &[SetupStage]) -> Vec<SetupStage> {
    stages.iter().rev().copied().collect()
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };

    use super::{
        DiffLayout, Input, Interaction, OutputMode, SetupStage, ViewPreferences, Viewport,
        cleanup_order, hunk_header_color, input_for_event, low_contrast_palette, mode_for_output,
        render_frame, render_frame_with_layout, syntax_color,
    };
    use crate::{
        layout::layout,
        parser::parse_unified_diff,
        syntax::{SyntaxClass, SyntaxHighlighter},
    };

    #[test]
    fn renders_compact_file_and_diff_panes_with_one_separator() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-before\n+after\n",
        )
        .expect("valid two-file diff");
        let interaction = Interaction {
            selected_file: 1,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).expect("test terminal");
        let mut syntax = SyntaxHighlighter::default();

        terminal
            .draw(|frame| {
                render_frame(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &mut syntax,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let expected_symbols = [
            ((2, 0), "f", "first file"),
            ((24, 0), " ", "left separator padding"),
            ((25, 0), "│", "separator"),
            ((26, 0), " ", "right separator padding"),
            ((27, 0), " ", "metadata marker column"),
            ((28, 0), " ", "metadata marker spacing"),
            ((29, 0), "-", "metadata content"),
            ((0, 1), "·", "selection marker"),
            ((27, 3), "-", "deletion marker"),
            ((27, 4), "+", "addition marker"),
            ((3, 3), "k", "footer hint"),
        ];

        for ((x, y), expected, name) in expected_symbols {
            assert_eq!(rendered[(x, y)].symbol(), expected, "{name}");
        }
        assert_eq!(rendered[(2, 0)].fg, Color::Rgb(160, 160, 160), "file label");
        assert_eq!(
            rendered[(0, 9)].fg,
            Color::Rgb(110, 110, 110),
            "footer hint"
        );
    }

    #[test]
    fn collapses_the_file_list_before_the_diff_pane() {
        let document = parse_unified_diff(b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n")
            .expect("valid diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(23, 8)).expect("test terminal");
        let mut syntax = SyntaxHighlighter::default();

        terminal
            .draw(|frame| {
                render_frame(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &mut syntax,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        assert_eq!(rendered[(0, 3)].symbol(), "-");
        assert_eq!(rendered[(1, 3)].symbol(), " ");
    }

    #[test]
    fn renders_vertical_pairs_with_a_dimmer_inner_separator() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        )
        .expect("valid asymmetric diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Vertical),
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");
        let mut syntax = SyntaxHighlighter::default();

        terminal
            .draw(|frame| {
                render_frame_with_layout(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &interaction.preferences,
                    &mut syntax,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        assert_eq!(rendered[(27, 3)].symbol(), "-", "before marker");
        assert_eq!(rendered[(54, 3)].symbol(), "+", "after marker");
        assert_eq!(rendered[(53, 3)].symbol(), "│", "inner separator");
        assert_eq!(rendered[(29, 5)].symbol(), "c", "before context payload");
        assert_eq!(rendered[(56, 5)].symbol(), "c", "after context payload");
        assert_eq!(rendered[(53, 5)].symbol(), "│", "context separator");
        assert_eq!(
            rendered[(54, 4)].symbol(),
            " ",
            "alignment peer has no marker"
        );
        assert_eq!(
            rendered[(79, 4)].bg,
            Color::Rgb(35, 73, 43),
            "alignment peer retains addition background"
        );
        assert!(
            !rendered[(79, 4)].modifier.contains(Modifier::DIM),
            "alignment peer does not inherit deletion dimming"
        );
        assert_eq!(
            rendered[(53, 3)].fg,
            Color::Rgb(95, 95, 95),
            "inner separator is dimmer than the outer separator"
        );
    }

    #[test]
    fn renders_vertical_deletion_peers_with_deletion_background() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,3 @@\n-old first\n+new first\n+new second\n context\n",
        )
        .expect("valid asymmetric diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Vertical),
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");
        let mut syntax = SyntaxHighlighter::default();

        terminal
            .draw(|frame| {
                render_frame_with_layout(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &interaction.preferences,
                    &mut syntax,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();

        assert_eq!(
            rendered[(27, 4)].symbol(),
            " ",
            "deletion peer has no marker"
        );
        assert_eq!(
            rendered[(52, 4)].bg,
            Color::Rgb(50, 30, 30),
            "deletion peer reaches the before pane edge"
        );
        assert!(
            rendered[(52, 4)].modifier.contains(Modifier::DIM),
            "deletion peer retains deletion dimming"
        );
    }

    #[test]
    fn renders_stacked_alignment_peers_as_markerless_rows() {
        let document = parse_unified_diff(
            b"--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        )
        .expect("valid asymmetric diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Stacked),
            viewport: Viewport {
                offset: 0,
                height: 14,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 14)).expect("test terminal");
        let mut syntax = SyntaxHighlighter::default();

        terminal
            .draw(|frame| {
                render_frame_with_layout(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &interaction.preferences,
                    &mut syntax,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let context_row = (0..14)
            .rev()
            .find(|row| rendered[(29, *row)].symbol() == "c")
            .expect("after context row");
        let alignment_peer_row = context_row
            .checked_sub(1)
            .expect("alignment peer precedes context");

        assert_eq!(
            rendered[(29, alignment_peer_row)].symbol(),
            " ",
            "alignment peer is empty"
        );
        assert_eq!(
            rendered[(79, alignment_peer_row)].bg,
            Color::Rgb(35, 73, 43),
            "alignment peer retains addition background"
        );
        assert!(
            !rendered[(79, alignment_peer_row)]
                .modifier
                .contains(Modifier::DIM),
            "alignment peer does not inherit deletion dimming"
        );
    }

    #[test]
    fn renders_syntax_and_low_contrast_record_styles() {
        let document = parse_unified_diff(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1,2 +1,2 @@\n context\n-fn removed() { let label = \"value\"; }\n+fn added() { let label = \"value\"; }\n",
        )
        .expect("valid Rust diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");
        let mut syntax = SyntaxHighlighter::default();

        terminal
            .draw(|frame| {
                render_frame(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &mut syntax,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let expected_symbols = [
            ((27, 0), " ", "header marker column"),
            ((28, 0), " ", "header marker spacing"),
            ((29, 0), "-", "header content"),
            ((27, 3), " ", "context marker column"),
            ((28, 3), " ", "context marker spacing"),
            ((28, 4), " ", "deletion marker spacing"),
            ((28, 5), " ", "addition marker spacing"),
        ];
        let expected_styles = [
            (
                (29, 3),
                Color::Rgb(180, 180, 180),
                Color::Reset,
                "context payload",
            ),
            (
                (27, 4),
                Color::Rgb(206, 74, 74),
                Color::Rgb(50, 30, 30),
                "deletion marker",
            ),
            (
                (27, 5),
                Color::Rgb(112, 195, 115),
                Color::Rgb(35, 73, 43),
                "addition marker",
            ),
            (
                (29, 4),
                Color::Rgb(86, 156, 214),
                Color::Rgb(50, 30, 30),
                "deletion keyword",
            ),
            (
                (29, 5),
                Color::Rgb(86, 156, 214),
                Color::Rgb(35, 73, 43),
                "addition keyword",
            ),
            (
                (54, 5),
                Color::Rgb(206, 145, 120),
                Color::Rgb(35, 73, 43),
                "addition string",
            ),
            (
                (79, 4),
                Color::Rgb(150, 150, 150),
                Color::Rgb(50, 30, 30),
                "deletion background reaches the diff pane edge",
            ),
            (
                (79, 5),
                Color::Rgb(220, 235, 220),
                Color::Rgb(35, 73, 43),
                "addition background reaches the diff pane edge",
            ),
        ];

        for ((x, y), expected, name) in expected_symbols {
            assert_eq!(rendered[(x, y)].symbol(), expected, "{name}");
        }
        for ((x, y), expected_foreground, expected_background, name) in expected_styles {
            let cell = &rendered[(x, y)];

            assert_eq!(cell.fg, expected_foreground, "{name} foreground");
            assert_eq!(cell.bg, expected_background, "{name} background");
        }
        assert!(
            rendered[(29, 4)].modifier.contains(Modifier::DIM),
            "deletion syntax inherits row-wide dimming"
        );
    }

    #[test]
    fn renders_character_detail_in_dim_change_colors_and_selects_files_cohesively() {
        let document =
            parse_unified_diff(b"--- a/source.txt\n+++ b/source.txt\n@@ -1 +1 @@\n-one\n+ore\n")
                .expect("valid character diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences {
                layout: DiffLayout::Unified,
                granularity: crate::interaction::DiffGranularity::Character,
                context_lines: 3,
            },
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");
        let mut syntax = SyntaxHighlighter::default();

        terminal
            .draw(|frame| {
                render_frame_with_layout(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &interaction.preferences,
                    &mut syntax,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let selected_marker = &rendered[(0, 0)];
        let selected_label = &rendered[(2, 0)];

        assert_eq!(selected_marker.symbol(), "·");
        assert_eq!(selected_marker.fg, selected_label.fg);
        assert_eq!(selected_marker.bg, selected_label.bg);
        assert_eq!(rendered[(30, 3)].fg, Color::Rgb(206, 74, 74));
        assert_eq!(rendered[(30, 4)].fg, Color::Rgb(112, 195, 115));
        assert!(rendered[(30, 3)].modifier.contains(Modifier::DIM));
        assert!(rendered[(30, 4)].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn renders_hunk_headers_as_muted_cyan_blue() {
        let document = parse_unified_diff(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-old\n+new\n@@ -3 +3 @@\n-before\n+after\n",
        )
        .expect("valid Rust diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 0,
                height: 8,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("test terminal");
        let mut syntax = SyntaxHighlighter::default();

        terminal
            .draw(|frame| {
                render_frame(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &mut syntax,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        for y in [2, 5] {
            let hunk_header = &rendered[(29, y)];

            assert_eq!(hunk_header.symbol(), "@");
            assert_eq!(hunk_header.fg, Color::Rgb(100, 155, 175));
            assert_eq!(hunk_header.bg, Color::Reset);
        }
        assert_eq!(
            rendered[(29, 0)].fg,
            Color::Reset,
            "file header stays neutral"
        );
    }

    #[test]
    fn renders_current_shortcuts_in_a_multiline_footer() {
        let document = parse_unified_diff(
            b"--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-before\n+after\n",
        )
        .expect("valid two-file diff");
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport {
                offset: 0,
                height: 12,
            },
        };
        let view = layout(&document, &interaction);
        let mut terminal = Terminal::new(TestBackend::new(80, 15)).expect("test terminal");
        let mut syntax = SyntaxHighlighter::default();

        terminal
            .draw(|frame| {
                render_frame(
                    frame,
                    &document,
                    &view,
                    &interaction.viewport,
                    &mut syntax,
                    u16::MAX,
                )
            })
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        let expected = [
            ((3, 3), "k", "upward movement"),
            ((1, 4), "h", "leftward movement"),
            ((3, 4), "·", "movement separator"),
            ((5, 4), "l", "rightward movement"),
            ((8, 4), "m", "movement label"),
            ((3, 5), "j", "downward movement"),
            ((2, 7), "^", "page up"),
            ((2, 8), "^", "page down"),
            ((2, 9), "g", "top and bottom"),
            ((2, 10), "v", "layout cycle"),
            ((2, 11), "c", "character detail"),
            ((1, 12), "+", "context controls"),
            ((0, 13), "(", "file rotation"),
            ((3, 14), "q", "exit"),
        ];

        for ((x, y), symbol, name) in expected {
            assert_eq!(rendered[(x, y)].symbol(), symbol, "{name}");
            assert_eq!(
                rendered[(x, y)].fg,
                Color::Rgb(110, 110, 110),
                "{name} color"
            );
        }
    }

    #[test]
    fn falls_back_from_truecolor_to_256_and_basic_semantic_colors() {
        assert_eq!(
            syntax_color(SyntaxClass::Keyword, u16::MAX),
            Color::Rgb(86, 156, 214)
        );
        assert_eq!(syntax_color(SyntaxClass::String, 256), Color::Indexed(180));
        assert_eq!(syntax_color(SyntaxClass::Type, 8), Color::Cyan);
        assert_ne!(
            syntax_color(SyntaxClass::Keyword, 8),
            syntax_color(SyntaxClass::String, 8)
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

    #[test]
    fn requests_interactive_mode_only_for_terminal_output() {
        assert_eq!(mode_for_output(false), OutputMode::Finite);
        assert_eq!(mode_for_output(true), OutputMode::Interactive);
    }

    #[test]
    fn maps_pager_keys_and_resize_events() {
        assert_eq!(
            input_for_event(CrosstermEvent::Key(KeyEvent::new(
                KeyCode::Char('j'),
                KeyModifiers::NONE
            ))),
            Some(Input::Down)
        );
        assert_eq!(
            input_for_event(CrosstermEvent::Key(KeyEvent::new(
                KeyCode::Tab,
                KeyModifiers::NONE
            ))),
            Some(Input::NextFile)
        );
        assert_eq!(
            input_for_event(CrosstermEvent::Key(KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::NONE
            ))),
            Some(Input::NextLayout)
        );
        assert_eq!(
            input_for_event(CrosstermEvent::Key(KeyEvent::new(
                KeyCode::Char('+'),
                KeyModifiers::NONE
            ))),
            Some(Input::IncreaseContext)
        );
        assert_eq!(
            input_for_event(CrosstermEvent::Key(KeyEvent::new(
                KeyCode::Char('-'),
                KeyModifiers::NONE
            ))),
            Some(Input::DecreaseContext)
        );
        assert_eq!(
            input_for_event(CrosstermEvent::Key(KeyEvent::new(
                KeyCode::BackTab,
                KeyModifiers::SHIFT
            ))),
            Some(Input::PreviousFile)
        );
        assert_eq!(
            input_for_event(CrosstermEvent::Key(KeyEvent::new(
                KeyCode::Tab,
                KeyModifiers::SHIFT
            ))),
            Some(Input::PreviousFile)
        );
        for key in ['{', '}'] {
            assert_eq!(
                input_for_event(CrosstermEvent::Key(KeyEvent::new(
                    KeyCode::Char(key),
                    KeyModifiers::NONE
                ))),
                None,
                "{key} has no cursor-visible destination"
            );
        }
        assert_eq!(
            input_for_event(CrosstermEvent::Resize(100, 30)),
            Some(Input::Resize {
                width: 100,
                height: 30,
            })
        );
        assert_eq!(
            input_for_event(CrosstermEvent::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL
            ))),
            Some(Input::Interrupt)
        );
    }

    #[test]
    fn unwinds_completed_setup_in_reverse_order() {
        let completed = [
            SetupStage::RawMode,
            SetupStage::AlternateScreen,
            SetupStage::HiddenCursor,
        ];

        assert_eq!(
            cleanup_order(&completed),
            vec![
                SetupStage::HiddenCursor,
                SetupStage::AlternateScreen,
                SetupStage::RawMode,
            ]
        );
    }
}
