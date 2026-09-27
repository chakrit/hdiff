use std::{io, time::Duration};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::Backend, layout::Size, widgets::Paragraph};

use crate::{
    interaction::{
        self, DiffLayout, DisplayBounds, Input, Interaction, NavigationBounds, Transition,
        ViewPreferences, Viewport,
    },
    layout::{ScrollExtent, content_pane_height, content_pane_width, maximum_horizontal_offset},
    prepared::PreparedDocument,
};

mod row_writer;
mod rows;
mod view;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Exit,
}

/// The interactive loop. Contract: docs/spec/architecture.md#interactive-loop.
pub struct Tui {
    document: PreparedDocument,
    interaction: Interaction,
    extent: KeyedExtent,
    color_count: u16,
}

impl Tui {
    pub fn new(document: PreparedDocument, area: Size, color_count: u16) -> Self {
        let interaction = Interaction {
            selected_file: 0,
            preferences: ViewPreferences::line(DiffLayout::Unified),
            viewport: Viewport::new(area.width, usize::from(area.height)),
        };
        let extent = KeyedExtent::build(&document, ExtentKey::of(&interaction));

        Self {
            document,
            interaction,
            extent,
            color_count,
        }
    }

    pub fn load(&mut self, document: PreparedDocument) {
        let viewport = &self.interaction.viewport;

        self.interaction = Interaction {
            selected_file: 0,
            preferences: self.interaction.preferences,
            viewport: Viewport::new(viewport.width, viewport.height),
        };
        self.document = document;
        self.extent = KeyedExtent::build(&self.document, ExtentKey::of(&self.interaction));
    }

    pub fn preferences(&self) -> ViewPreferences {
        self.interaction.preferences
    }

    pub fn run<B: Backend<Error = io::Error>>(
        &mut self,
        terminal: &mut Terminal<B>,
        events: impl Iterator<Item = io::Result<Event>>,
    ) -> io::Result<()> {
        self.draw(terminal)?;

        for event in events {
            if self.step(terminal, event?)? == Step::Exit {
                break;
            }
        }
        Ok(())
    }

    pub fn step<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
        event: Event,
    ) -> Result<Step, B::Error> {
        let Some(input) = input_for_event(event) else {
            return Ok(Step::Continue);
        };

        let top_row = self
            .extent
            .extent
            .rows(self.interaction.viewport.vertical_offset, 1)
            .next();
        let target = ExtentKey::new(
            self.interaction.selected_file,
            self.interaction.preferences.after(&input),
        );
        let target_extent = self.extent.select(&self.document, target);
        let bounds = navigation_bounds(
            self.document.file_count(),
            &self.interaction,
            &input,
            target_extent,
            top_row,
        );

        match interaction::transition_interaction(&self.interaction, input, &bounds) {
            Transition::Exit => Ok(Step::Exit),
            Transition::Redraw(_) => unreachable!("interactive transition retains selection"),
            Transition::RedrawInteraction(next) => {
                self.interaction = next;
                self.extent
                    .select(&self.document, ExtentKey::of(&self.interaction));
                self.draw(terminal)?;
                Ok(Step::Continue)
            }
        }
    }

    pub fn draw<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), B::Error> {
        let Some(file) = self.document.file(self.interaction.selected_file) else {
            return terminal
                .draw(|frame| frame.render_widget(Paragraph::new(""), frame.area()))
                .map(|_| ());
        };
        let files = self.document.file_rows(self.interaction.selected_file);

        terminal
            .draw(|frame| {
                view::render_prepared_frame(
                    frame,
                    &files,
                    file,
                    &self.extent.extent,
                    &self.interaction,
                    self.color_count,
                )
            })
            .map(|_| ())
    }
}

/// The scroll extent of one file, layout, and context count, rebuilt only when that key
/// changes.
struct KeyedExtent {
    key: ExtentKey,
    extent: ScrollExtent,
}

impl KeyedExtent {
    fn build(document: &PreparedDocument, key: ExtentKey) -> Self {
        // A document without files shows no rows.
        let extent = document
            .file(key.file)
            .map(|file| file.layout().scroll_extent(key.layout, key.context_lines))
            .unwrap_or_default();

        Self { key, extent }
    }

    fn select(&mut self, document: &PreparedDocument, key: ExtentKey) -> &ScrollExtent {
        if self.key != key {
            *self = Self::build(document, key);
        }
        &self.extent
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ExtentKey {
    file: usize,
    layout: DiffLayout,
    context_lines: usize,
}

impl ExtentKey {
    fn new(file: usize, preferences: ViewPreferences) -> Self {
        Self {
            file,
            layout: preferences.layout,
            context_lines: preferences.context_lines,
        }
    }

    fn of(interaction: &Interaction) -> Self {
        Self::new(interaction.selected_file, interaction.preferences)
    }
}

/// Blocking terminal events with consecutive resizes coalesced into the latest one.
#[derive(Default)]
pub struct TtyEvents {
    pending: Option<Event>,
}

impl Iterator for TtyEvents {
    type Item = io::Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = match self.pending.take() {
            Some(event) => event,
            None => match event::read() {
                Ok(event) => event,
                Err(error) => return Some(Err(error)),
            },
        };
        Some(self.coalesce_resize(event))
    }
}

impl TtyEvents {
    fn coalesce_resize(&mut self, event: Event) -> io::Result<Event> {
        let Event::Resize(..) = event else {
            return Ok(event);
        };
        let mut latest_resize = event;

        while event::poll(Duration::ZERO)? {
            let next_event = event::read()?;
            if !matches!(next_event, Event::Resize(..)) {
                self.pending = Some(next_event);
                break;
            }
            latest_resize = next_event;
        }
        Ok(latest_resize)
    }
}

/// Bounds of the display `input` leads to, whose rows are `target_extent`. `top_row` is
/// the row-list index of the current top row, if any.
fn navigation_bounds(
    file_count: usize,
    interaction: &Interaction,
    input: &Input,
    target_extent: &ScrollExtent,
    top_row: Option<usize>,
) -> NavigationBounds {
    let display_layout = interaction.preferences.after(input).layout;
    let (screen_width, screen_height) = match input {
        Input::Resize { width, height } => (*width, usize::from(*height)),
        _ => (interaction.viewport.width, interaction.viewport.height),
    };
    let pane_width = content_pane_width(screen_width, display_layout);
    let pane_height = content_pane_height(screen_height, display_layout);

    NavigationBounds {
        file_count,
        target_display: DisplayBounds {
            maximum_vertical_offset: target_extent.row_count().saturating_sub(pane_height),
            maximum_horizontal_offset: maximum_horizontal_offset(target_extent.width(), pane_width),
        },
        anchored_vertical_offset: top_row.map_or(0, |row| target_extent.offset_of(row)),
    }
}

fn input_for_event(event: Event) -> Option<Input> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => input_for_key(key),
        Event::Resize(width, height) => Some(Input::Resize { width, height }),
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

#[cfg(test)]
mod tests {
    use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};

    use super::{Input, OutputMode, input_for_event, mode_for_output};

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
                            &prepared
                                .file(selected_file)
                                .expect("file occurrence")
                                .layout()
                                .scroll_extent(layout, 3),
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
}
