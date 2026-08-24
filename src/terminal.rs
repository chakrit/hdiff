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
    document::{DiffDocument, DiffFile},
    interaction::{self, Input, Interaction, NavigationBounds, Transition, Viewport},
    layout::{Layout, PaneLayout, layout, pane_layout},
    syntax::{SyntaxClass, SyntaxHighlighter, SyntaxSpan},
};

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

pub fn run_interactive(document: &DiffDocument) -> io::Result<()> {
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
    document: &DiffDocument,
    color_count: u16,
) -> io::Result<()> {
    let (_, height) = terminal::size()?;
    let mut interaction = Interaction {
        selected_file: 0,
        viewport: Viewport {
            offset: 0,
            height: usize::from(height),
        },
    };
    let mut view = layout(document, &interaction);
    let mut syntax = SyntaxHighlighter::default();

    draw(
        session.terminal_mut(),
        document,
        &view,
        &interaction.viewport,
        &mut syntax,
        color_count,
    )?;

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
                file_count: document.files.len(),
                line_count: view.diff_lines.len(),
                hunk_offsets: &view.hunk_offsets,
            };
            interaction::transition_interaction(&interaction, input, &bounds)
        };
        match transition {
            Transition::Exit => return Ok(()),
            Transition::Redraw(_) => unreachable!("interactive transition retains selection"),
            Transition::RedrawInteraction(next_interaction) => {
                interaction = next_interaction;
                view = layout(document, &interaction);
                draw(
                    session.terminal_mut(),
                    document,
                    &view,
                    &interaction.viewport,
                    &mut syntax,
                    color_count,
                )?;
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
        (KeyCode::Tab, KeyModifiers::SHIFT) => Some(Input::PreviousFile),
        (KeyCode::Tab, _) => Some(Input::NextFile),
        (KeyCode::BackTab, _) => Some(Input::PreviousFile),
        (KeyCode::Char('{'), _) => Some(Input::PreviousHunk),
        (KeyCode::Char('}'), _) => Some(Input::NextHunk),
        _ => None,
    }
}

fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    document: &DiffDocument,
    layout: &Layout,
    viewport: &Viewport,
    syntax: &mut SyntaxHighlighter,
    color_count: u16,
) -> io::Result<()> {
    terminal
        .draw(|frame| render_frame(frame, document, layout, viewport, syntax, color_count))
        .map(|_| ())
}

fn render_frame(
    frame: &mut Frame,
    document: &DiffDocument,
    layout: &Layout,
    viewport: &Viewport,
    syntax: &mut SyntaxHighlighter,
    color_count: u16,
) {
    let area = frame.area();
    let selected = layout.files.iter().position(|file| file.selected);
    let file = selected.and_then(|index| document.files.get(index));
    let lines = visible_diff_lines(layout, viewport, file, syntax, color_count);
    let diff = Paragraph::new(Text::from(lines));

    match pane_layout(area.width, area.height) {
        PaneLayout::TooNarrow => frame.render_widget(Paragraph::new("screen too narrow"), area),
        PaneLayout::TooShort => frame.render_widget(Paragraph::new("screen too short"), area),
        PaneLayout::DiffOnly => frame.render_widget(diff, area),
        PaneLayout::Split {
            file_list_width,
            separator_padding,
        } => render_split_panes(
            frame,
            layout,
            area,
            diff,
            file_list_width,
            separator_padding,
        ),
    }
}

fn render_split_panes(
    frame: &mut Frame,
    layout: &Layout,
    area: ratatui::layout::Rect,
    diff: Paragraph<'_>,
    file_list_width: u16,
    separator_padding: u16,
) {
    let panes = RatatuiLayout::horizontal([
        Constraint::Length(file_list_width),
        Constraint::Length(separator_padding),
        Constraint::Length(1),
        Constraint::Length(separator_padding),
        Constraint::Min(1),
    ])
    .split(area);
    let file_list_areas =
        RatatuiLayout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(panes[0]);
    let mut selected_file = ListState::default();
    selected_file.select(layout.files.iter().position(|file| file.selected));
    let files = layout
        .files
        .iter()
        .map(|file| ListItem::new(file.label.as_str()))
        .collect::<Vec<_>>();
    let file_list = List::new(files)
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let separator = Text::from(
        (0..area.height)
            .map(|_| Line::styled("│", Style::default().fg(Color::DarkGray)))
            .collect::<Vec<_>>(),
    );
    let hints = Paragraph::new("Tab next · ⇧Tab prev").style(Style::default().fg(Color::DarkGray));

    frame.render_stateful_widget(file_list, file_list_areas[0], &mut selected_file);
    frame.render_widget(hints, file_list_areas[1]);
    frame.render_widget(Paragraph::new(separator), panes[2]);
    frame.render_widget(diff, panes[4]);
}

fn visible_diff_lines(
    layout: &Layout,
    viewport: &Viewport,
    file: Option<&DiffFile>,
    syntax: &mut SyntaxHighlighter,
    color_count: u16,
) -> Vec<Line<'static>> {
    let spans = file
        .map(|file| syntax_rows(file, syntax))
        .unwrap_or_default();
    layout
        .diff_lines
        .iter()
        .enumerate()
        .skip(viewport.offset)
        .map(|(index, line)| {
            let without_ending = line
                .strip_suffix(b"\r\n")
                .or_else(|| line.strip_suffix(b"\n"))
                .unwrap_or(line);
            styled_line(
                without_ending,
                spans.get(index).map(Vec::as_slice).unwrap_or_default(),
                color_count,
            )
        })
        .collect()
}

fn syntax_rows(file: &DiffFile, syntax: &mut SyntaxHighlighter) -> Vec<Vec<SyntaxSpan>> {
    match file {
        DiffFile::Metadata { lines } => (0..lines.len()).map(|_| Vec::new()).collect(),
        DiffFile::Unified(file) => {
            let mut rows = (0..file.metadata.len() + 2)
                .map(|_| Vec::new())
                .collect::<Vec<_>>();
            for hunk_index in 0..file.hunks.len() {
                rows.push(Vec::new());
                rows.extend(syntax.highlight_hunk(file, hunk_index));
            }
            rows
        }
    }
}

fn styled_line(bytes: &[u8], syntax: &[SyntaxSpan], color_count: u16) -> Line<'static> {
    let text = String::from_utf8_lossy(bytes).into_owned();
    let marker = bytes.first().copied();
    let palette = low_contrast_palette(color_count);
    let mut rendered = Vec::new();
    let mut cursor = match marker {
        Some(b'+') => {
            rendered.push(Span::styled(
                text[..1].to_owned(),
                Style::default().fg(palette.addition_marker),
            ));
            rendered.push(Span::raw(" "));
            1
        }
        Some(b'-') => {
            rendered.push(Span::styled(
                text[..1].to_owned(),
                Style::default().fg(palette.deletion_marker),
            ));
            rendered.push(Span::raw(" "));
            1
        }
        _ => 0,
    };
    for span in syntax {
        let start = span.start + 1;
        let end = span.end + 1;
        if start > cursor {
            rendered.push(Span::raw(text[cursor..start].to_owned()));
        }
        rendered.push(Span::styled(
            text[start..end].to_owned(),
            Style::default().fg(syntax_color(span.class, color_count)),
        ));
        cursor = end;
    }
    if cursor < text.len() {
        rendered.push(Span::raw(text[cursor..].to_owned()));
    }
    let line = Line::from(rendered);

    match marker {
        Some(b'+') => line.style(Style::default().bg(palette.addition_background)),
        Some(b'-') => line.style(Style::default().fg(palette.deletion_body)),
        _ => line,
    }
}

#[derive(Clone, Copy)]
struct LowContrastPalette {
    addition_background: Color,
    deletion_body: Color,
    deletion_marker: Color,
    addition_marker: Color,
}

fn low_contrast_palette(color_count: u16) -> LowContrastPalette {
    match color_count {
        u16::MAX => LowContrastPalette {
            addition_background: Color::Rgb(35, 73, 43),
            deletion_body: Color::Rgb(150, 150, 150),
            deletion_marker: Color::Rgb(206, 74, 74),
            addition_marker: Color::Rgb(98, 173, 99),
        },
        256.. => LowContrastPalette {
            addition_background: Color::Indexed(22),
            deletion_body: Color::Indexed(245),
            deletion_marker: Color::Indexed(167),
            addition_marker: Color::Indexed(71),
        },
        _ => LowContrastPalette {
            addition_background: Color::Green,
            deletion_body: Color::DarkGray,
            deletion_marker: Color::Red,
            addition_marker: Color::Green,
        },
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
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    use super::{
        Input, Interaction, OutputMode, SetupStage, Viewport, cleanup_order, input_for_event,
        low_contrast_palette, mode_for_output, render_frame, syntax_color,
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
            ((2, 0), "a", "first file"),
            ((24, 0), " ", "left separator padding"),
            ((25, 0), "│", "separator"),
            ((26, 0), " ", "right separator padding"),
            ((0, 1), ">", "selection marker"),
            ((27, 3), "-", "deletion marker"),
            ((27, 4), "+", "addition marker"),
            ((0, 9), "T", "footer hint"),
        ];

        for ((x, y), expected, name) in expected_symbols {
            assert_eq!(rendered[(x, y)].symbol(), expected, "{name}");
        }
        assert_eq!(rendered[(0, 9)].fg, Color::DarkGray, "footer hint");
    }

    #[test]
    fn collapses_the_file_list_before_the_diff_pane() {
        let document = parse_unified_diff(b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n")
            .expect("valid diff");
        let interaction = Interaction {
            selected_file: 0,
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
    fn renders_syntax_and_low_contrast_record_styles() {
        let document = parse_unified_diff(
            b"--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-old\n+fn added() { let label = \"value\"; }\n",
        )
        .expect("valid Rust diff");
        let interaction = Interaction {
            selected_file: 0,
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
            ((28, 3), " ", "deletion marker spacing"),
            ((28, 4), " ", "addition marker spacing"),
        ];
        let expected_styles = [
            (
                (27, 3),
                Color::Rgb(206, 74, 74),
                Color::Reset,
                "deletion marker",
            ),
            (
                (27, 4),
                Color::Rgb(98, 173, 99),
                Color::Rgb(35, 73, 43),
                "addition marker",
            ),
            (
                (29, 3),
                Color::Rgb(150, 150, 150),
                Color::Reset,
                "deletion payload",
            ),
            (
                (29, 4),
                Color::Rgb(86, 156, 214),
                Color::Rgb(35, 73, 43),
                "addition keyword",
            ),
            (
                (54, 4),
                Color::Rgb(206, 145, 120),
                Color::Rgb(35, 73, 43),
                "addition string",
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
        assert_eq!(
            input_for_event(CrosstermEvent::Key(KeyEvent::new(
                KeyCode::Char('}'),
                KeyModifiers::NONE
            ))),
            Some(Input::NextHunk)
        );
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
