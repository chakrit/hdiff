use std::{io, time::Duration};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::available_color_count,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::Paragraph};

use crate::{
    interaction::{
        self, DiffLayout, DisplayBounds, Input, Interaction, NavigationBounds, Transition,
        ViewPreferences, Viewport,
    },
    layout::{content_pane_width, maximum_horizontal_offset},
    prepared::PreparedDocument,
};

mod rows;
mod view;

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
    let (width, height) = terminal::size()?;
    let mut interaction = Interaction {
        selected_file: 0,
        preferences: ViewPreferences::line(DiffLayout::Unified),
        viewport: Viewport::new(width, usize::from(height)),
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
            let display_layout = match &input {
                Input::NextLayout => interaction.preferences.layout.next(),
                _ => interaction.preferences.layout,
            };
            let file = document.file(interaction.selected_file);
            let content_width = file
                .map(|file| {
                    rows::maximum_visible_row_width(
                        &file.layout,
                        display_layout,
                        interaction.preferences.context_lines,
                    )
                })
                .unwrap_or_default();
            let screen_width = match &input {
                Input::Resize { width, .. } => *width,
                _ => interaction.viewport.width,
            };
            let pane_width = content_pane_width(screen_width, display_layout);
            let maximum_horizontal_offset = maximum_horizontal_offset(content_width, pane_width);
            let line_count = file
                .map(|file| match display_layout {
                    DiffLayout::Unified => file
                        .layout
                        .visible_unified_line_count(interaction.preferences.context_lines),
                    DiffLayout::Vertical | DiffLayout::Stacked => file
                        .layout
                        .visible_side_by_side_line_count(interaction.preferences.context_lines),
                })
                .unwrap_or_default();
            let bounds = NavigationBounds {
                file_count: document.file_count(),
                target_display: DisplayBounds {
                    line_count,
                    maximum_horizontal_offset,
                },
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
        (KeyCode::Char('h'), _) => Some(Input::Left),
        (KeyCode::Char('l'), _) => Some(Input::Right),
        (KeyCode::Char('0') | KeyCode::Char('^'), _) => Some(Input::HorizontalStart),
        (KeyCode::Char('$'), _) => Some(Input::HorizontalEnd),
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
        .draw(|frame| view::render_prepared_frame(frame, &files, file, interaction, color_count))
        .map(|_| ())
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

    use super::{Input, OutputMode, SetupStage, cleanup_order, input_for_event, mode_for_output};

    #[test]
    fn renders_surrounding_text_once_per_pane_for_each_file_occurrence() {
        use crate::{
            interaction::{DiffLayout, Interaction, ViewPreferences, Viewport},
            parser::parse_unified_diff,
            prepared::PreparedDocument,
        };
        use ratatui::{Terminal, backend::TestBackend};

        let patch = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        let input = format!("first message\n{patch}second message\n{patch}trailer\n");
        let document = parse_unified_diff(input.as_bytes()).expect("diffs with messages");
        let prepared = PreparedDocument::prepare(&document);

        for (layout, copies) in [
            (DiffLayout::Unified, 1),
            (DiffLayout::Vertical, 2),
            (DiffLayout::Stacked, 2),
        ] {
            for (selected_file, message, absent, trailers) in [
                (0, "first message", "second message", 0),
                (1, "second message", "first message", copies),
            ] {
                let interaction = Interaction {
                    selected_file,
                    preferences: ViewPreferences::line(layout),
                    viewport: Viewport::new(120, 30),
                };
                let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("test terminal");
                terminal
                    .draw(|frame| {
                        super::view::render_prepared_frame(
                            frame,
                            &prepared.file_rows(selected_file),
                            prepared.file(selected_file).expect("file occurrence"),
                            &interaction,
                            u16::MAX,
                        )
                    })
                    .expect("render prepared frame");
                let text: String = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect();

                assert_eq!(text.matches(message).count(), copies, "{layout:?}");
                assert!(!text.contains(absent), "{layout:?}");
                assert_eq!(text.matches("trailer").count(), trailers, "{layout:?}");
            }
        }
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
        for (key, expected) in [
            ('h', Input::Left),
            ('l', Input::Right),
            ('0', Input::HorizontalStart),
            ('^', Input::HorizontalStart),
            ('$', Input::HorizontalEnd),
        ] {
            assert_eq!(
                input_for_event(CrosstermEvent::Key(KeyEvent::new(
                    KeyCode::Char(key),
                    KeyModifiers::NONE
                ))),
                Some(expected),
                "{key} horizontal movement"
            );
        }
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
