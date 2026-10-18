use std::{io, time::Duration};

use crossterm::event::{self, Event as TerminalEvent};
use ratatui::{Terminal, backend::Backend, layout::Size};

use crate::{
    interaction::{self, Interaction, Transition},
    navigation::Navigation,
    preferences::{DiffLayout, ViewPreferences},
    prepared::PreparedDocument,
    theme::{ColorDepth, Theme},
};

mod chrome;
mod keymap;
mod row_style;
mod row_writer;
mod rows;
mod session;
mod view;

pub use session::{open_controlling_terminal, run_interactive};

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
    navigation: Navigation,
    theme: &'static Theme,
}

impl Tui {
    pub fn new(document: PreparedDocument, area: Size, color_depth: ColorDepth) -> Self {
        let interaction = Interaction::start(ViewPreferences::initial(DiffLayout::Unified), area);
        let navigation = Navigation::new(&document, &interaction);

        Self {
            document,
            interaction,
            navigation,
            theme: Theme::for_depth(color_depth),
        }
    }

    pub fn load(&mut self, document: PreparedDocument) {
        self.interaction = Interaction::start(
            self.interaction.preferences,
            self.interaction.viewport.screen,
        );
        self.document = document;
        self.navigation = Navigation::new(&self.document, &self.interaction);
    }

    pub fn preferences(&self) -> ViewPreferences {
        self.interaction.preferences
    }

    pub fn run<B: Backend<Error = io::Error>>(
        &mut self,
        terminal: &mut Terminal<B>,
        events: impl Iterator<Item = io::Result<TerminalEvent>>,
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
        event: TerminalEvent,
    ) -> Result<Step, B::Error> {
        let keymap = self.interaction.keymap;
        let Some(event) = keymap::event_for(event).and_then(|event| event.admitted(keymap)) else {
            return Ok(Step::Continue);
        };

        let destination = self
            .navigation
            .destination(&self.document, &self.interaction, &event);

        match interaction::transition(&self.interaction, event, &destination) {
            Transition::Exit => Ok(Step::Exit),
            Transition::Redraw(next) => {
                self.interaction = next;
                self.navigation.settle(&self.document, &self.interaction);
                self.draw(terminal)?;
                Ok(Step::Continue)
            }
        }
    }

    pub fn draw<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), B::Error> {
        let page = self.document.page(self.interaction.page);
        let tree = self.document.file_tree();

        terminal
            .draw(|frame| {
                view::render_prepared_frame(
                    frame,
                    tree,
                    page,
                    self.navigation.extent(),
                    &self.interaction,
                    self.theme,
                )
            })
            .map(|_| ())
    }
}

/// Blocking terminal events with consecutive resizes coalesced into the latest one.
#[derive(Default)]
struct TtyEvents {
    pending: Option<TerminalEvent>,
}

impl Iterator for TtyEvents {
    type Item = io::Result<TerminalEvent>;

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
    fn coalesce_resize(&mut self, event: TerminalEvent) -> io::Result<TerminalEvent> {
        let TerminalEvent::Resize(..) = event else {
            return Ok(event);
        };
        let mut latest_resize = event;

        while event::poll(Duration::ZERO)? {
            let next_event = event::read()?;
            if !matches!(next_event, TerminalEvent::Resize(..)) {
                self.pending = Some(next_event);
                break;
            }
            latest_resize = next_event;
        }
        Ok(latest_resize)
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend, layout::Size};

    use super::{OutputMode, Tui, mode_for_output};
    use crate::{
        extent::{DisplayExtent, Wrap},
        interaction::Interaction,
        parser::{parse_comparison, parse_patch},
        preferences::{DiffLayout, ViewPreferences},
        prepared::{PageIndex, PreparedDocument},
        theme::{ColorDepth, Theme},
    };

    fn screen_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn renders_surrounding_text_once_per_pane_for_each_file_occurrence() {
        let patch = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        let input = format!("first message\n{patch}second message\n{patch}trailer\n");
        let document = parse_patch(input.as_bytes()).expect("diffs with messages");
        let prepared = PreparedDocument::prepare(&document);
        let second = PageIndex::FIRST.next(prepared.page_count());

        for (layout, copies) in [
            (DiffLayout::Unified, 1),
            (DiffLayout::Vertical, 2),
            (DiffLayout::Stacked, 2),
        ] {
            for (page, message, absent, trailers) in [
                (PageIndex::FIRST, "first message", "second message", 0),
                (second, "second message", "first message", copies),
            ] {
                let interaction = Interaction {
                    page,
                    ..Interaction::start(ViewPreferences::initial(layout), Size::new(120, 30))
                };
                let extent = DisplayExtent::of(prepared.page(page).rows(), layout, 3, Wrap::Off);
                let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("test terminal");
                terminal
                    .draw(|frame| {
                        super::view::render_prepared_frame(
                            frame,
                            prepared.file_tree(),
                            prepared.page(page),
                            &extent,
                            &interaction,
                            Theme::for_depth(ColorDepth::TrueColor),
                        )
                    })
                    .expect("render prepared frame");
                let text = screen_text(&terminal);

                assert_eq!(text.matches(message).count(), copies, "{layout:?}");
                assert!(!text.contains(absent), "{layout:?}");
                assert_eq!(text.matches("trailer").count(), trailers, "{layout:?}");
            }
        }
    }

    /// Contract: docs/spec/architecture.md#input-contract (comparison output holding only
    /// surrounding text is shown).
    #[test]
    fn shows_comparison_output_that_holds_only_text() {
        let document = parse_comparison(b"Binary files a.bin and b.bin differ\n")
            .expect("text-only comparison output");
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("test terminal");
        let mut tui = Tui::new(
            PreparedDocument::prepare(&document),
            Size::new(120, 30),
            ColorDepth::TrueColor,
        );

        tui.draw(&mut terminal).expect("initial frame");
        for key in [KeyCode::Char('j'), KeyCode::Tab, KeyCode::BackTab] {
            let event = TerminalEvent::Key(KeyEvent::new(key, KeyModifiers::NONE));
            tui.step(&mut terminal, event).expect("step");
        }

        assert!(screen_text(&terminal).contains("Binary files a.bin and b.bin differ"));
    }

    #[test]
    fn requests_interactive_mode_only_for_terminal_output() {
        assert_eq!(mode_for_output(false), OutputMode::Finite);
        assert_eq!(mode_for_output(true), OutputMode::Interactive);
    }
}
