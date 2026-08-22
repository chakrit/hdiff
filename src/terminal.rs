use std::{io, time::Duration};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout as RatatuiLayout},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    document::DiffDocument,
    interaction::{self, Input, Interaction, NavigationBounds, Transition, Viewport},
    layout::{Layout, layout},
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
    let result = run_loop(&mut session, document);
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

fn run_loop(session: &mut TerminalSession, document: &DiffDocument) -> io::Result<()> {
    let (_, height) = terminal::size()?;
    let mut interaction = Interaction {
        selected_file: 0,
        viewport: Viewport {
            offset: 0,
            height: usize::from(height),
        },
    };
    let mut view = layout(document, &interaction);

    draw(session.terminal_mut(), &view, &interaction.viewport)?;

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
                draw(session.terminal_mut(), &view, &interaction.viewport)?;
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
        (KeyCode::Tab, _) => Some(Input::NextFile),
        (KeyCode::Char('{'), _) => Some(Input::PreviousHunk),
        (KeyCode::Char('}'), _) => Some(Input::NextHunk),
        _ => None,
    }
}

fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    layout: &Layout,
    viewport: &Viewport,
) -> io::Result<()> {
    terminal
        .draw(|frame| render_frame(frame, layout, viewport))
        .map(|_| ())
}

fn render_frame(frame: &mut Frame, layout: &Layout, viewport: &Viewport) {
    let area = frame.area();
    if area.width < 20 {
        frame.render_widget(Paragraph::new("screen too narrow"), area);
        return;
    }
    if area.height < 3 {
        frame.render_widget(Paragraph::new("screen too short"), area);
        return;
    }

    let lines = visible_diff_lines(layout, viewport);
    let diff = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("Diff"));
    if area.width < 25 {
        frame.render_widget(diff, area);
        return;
    }

    let panes = RatatuiLayout::horizontal([Constraint::Length(24), Constraint::Min(1)]).split(area);
    let mut selected_file = ListState::default();
    selected_file.select(layout.files.iter().position(|file| file.selected));
    let files = layout
        .files
        .iter()
        .map(|file| ListItem::new(file.label.as_str()))
        .collect::<Vec<_>>();
    let file_list = List::new(files)
        .block(Block::default().borders(Borders::ALL).title("Files"))
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(file_list, panes[0], &mut selected_file);
    frame.render_widget(diff, panes[1]);
}

fn visible_diff_lines(layout: &Layout, viewport: &Viewport) -> Vec<Line<'static>> {
    layout
        .diff_lines
        .iter()
        .skip(viewport.offset)
        .map(|line| {
            let without_ending = line
                .strip_suffix(b"\r\n")
                .or_else(|| line.strip_suffix(b"\n"))
                .unwrap_or(line);
            Line::from(String::from_utf8_lossy(without_ending).into_owned())
        })
        .collect()
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
    use ratatui::{Terminal, backend::TestBackend};

    use super::{
        Input, Interaction, OutputMode, SetupStage, Viewport, cleanup_order, input_for_event,
        mode_for_output, render_frame,
    };
    use crate::{layout::layout, parser::parse_unified_diff};

    #[test]
    fn renders_distinct_file_and_diff_panes() {
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

        terminal
            .draw(|frame| render_frame(frame, &view, &interaction.viewport))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        assert_eq!(rendered[(0, 0)].symbol(), "┌");
        assert_eq!(rendered[(24, 0)].symbol(), "┌");
        assert_eq!(rendered[(3, 1)].symbol(), "a");
        assert_eq!(rendered[(1, 2)].symbol(), ">");
        assert_eq!(rendered[(25, 5)].symbol(), "+");
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

        terminal
            .draw(|frame| render_frame(frame, &view, &interaction.viewport))
            .expect("render frame");

        let rendered = terminal.backend().buffer();
        assert_eq!(rendered[(1, 0)].symbol(), "D");
        assert_eq!(rendered[(1, 1)].symbol(), "-");
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
