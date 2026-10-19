//! Where an effect leads the interaction: the document navigation shows, the interaction
//! it holds, the effective view and scroll extent that show it, and the scroll position
//! within their bounds. Rules: docs/spec/architecture.md#core-model and
//! docs/spec/architecture.md#view-model and
//! docs/spec/architecture.md#viewport-coordinates.

pub mod extent;
pub mod interaction;
pub mod scroll;

use ratatui::layout::Size;

use self::{
    extent::{ExtentSettings, ScrollExtent, ShownLines},
    interaction::{AnchorLine, Effect, Event, Interaction, Outcome, Placement},
    scroll::{ScrollBounds, ScrollPosition},
};
use crate::{
    effective_view::EffectiveView, preferences::ViewPreferences, prepared::PreparedDocument,
    screen::ScreenLines,
};

/// Whether the session goes on after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Exit,
}

/// The document a session shows and the interaction it is at, with the effective view and
/// scroll extent that show it. Navigation owns the document, so the extent resolves its
/// rows on the document it was built from. The extent is rebuilt only when its page or
/// settings change.
pub struct Navigation {
    document: PreparedDocument,
    interaction: Interaction,
    view: EffectiveView,
    extent: ScrollExtent,
}

impl Navigation {
    /// The first page of `document` with `preferences` on a `screen`-sized terminal, at
    /// its start.
    pub fn start(document: PreparedDocument, preferences: ViewPreferences, screen: Size) -> Self {
        let interaction = Interaction::start(preferences, screen);
        let view = EffectiveView::of(preferences, screen);
        let settings = settings_of(preferences, &view);
        let extent = ScrollExtent::of(&document, interaction.page(), settings);

        Self {
            document,
            interaction,
            view,
            extent,
        }
    }

    pub fn document(&self) -> &PreparedDocument {
        &self.document
    }

    pub fn interaction(&self) -> &Interaction {
        &self.interaction
    }

    pub fn view(&self) -> &EffectiveView {
        &self.view
    }

    pub fn extent(&self) -> &ScrollExtent {
        &self.extent
    }

    /// The screen lines the diff panes show from the scroll position, with their rows in
    /// the interaction's granularity.
    pub fn shown(&self) -> ShownLines<'_> {
        let granularity = self.interaction.preferences().granularity;
        let offset = self.interaction.scroll().position().vertical;
        let limit = self.view.pane_content().height;

        self.extent
            .shown(&self.document, granularity, offset, limit)
    }

    /// Leads the interaction where `event` asks among the document's pages: the session
    /// exits, or goes on to draw what it leads to.
    pub fn respond(&mut self, event: Event) -> Step {
        match self.interaction.respond(event, self.document.page_count()) {
            Outcome::Exit => Step::Exit,
            Outcome::Redraw => Step::Continue,
            Outcome::Effect(effect) => {
                self.step(effect);
                Step::Continue
            }
        }
    }

    /// Leads the interaction where `effect` asks; the view and extent then show it.
    fn step(&mut self, effect: Effect) {
        let interaction = self.interaction;

        match effect {
            Effect::Scroll(movement) => {
                let bounds = bounds_of(&self.extent, &self.view);
                self.interaction =
                    interaction.scrolled(interaction.scroll().moved(movement, bounds));
            }
            Effect::Keymap(keymap) => self.interaction = interaction.keymap_switched(keymap),
            Effect::TurnPage(page) => {
                self.extent.turn_to(&self.document, page);
                self.interaction = interaction.turned_to(page);
            }
            Effect::Redisplay {
                preferences,
                placement,
            } => self.redisplay(preferences, interaction.screen(), placement),
        }
    }

    /// Shows the interaction on a `screen`-sized terminal when it shows another size,
    /// keeping the row on the diff pane's top line there.
    pub fn adopt_screen(&mut self, screen: Size) {
        if screen == self.interaction.screen() {
            return;
        }

        let preferences = self.interaction.preferences();
        let placement = Placement::Anchor(AnchorLine::PaneTop);
        self.redisplay(preferences, screen, placement);
    }

    /// Shows the same page with `preferences` on a `screen`-sized terminal, its first
    /// line placed as `placement` asks.
    fn redisplay(&mut self, preferences: ViewPreferences, screen: Size, placement: Placement) {
        let current = self.interaction.scroll().position();
        let view = EffectiveView::of(preferences, screen);
        let settings = settings_of(preferences, &view);
        let document = &self.document;

        let vertical = match placement {
            Placement::Keep => {
                self.extent.reshow(document, settings);
                current.vertical
            }
            Placement::Anchor(anchor) => {
                // The row on the anchor line of the view shown now goes to the anchor
                // line of the new view.
                let source = pane_line(&self.view, anchor);
                let target = pane_line(&view, anchor);
                let offset = current.vertical.saturating_add(source);
                // None only for a page without lines, since every in-bounds offset names
                // a line.
                self.extent
                    .reanchor(document, settings, offset)
                    .map_or(ScreenLines::ZERO, |line| line.saturating_sub(target))
            }
        };

        self.view = view;
        let start = ScrollPosition {
            vertical,
            ..current
        };
        let scroll = bounds_of(&self.extent, &self.view).clamp(start);
        self.interaction = self.interaction.redisplayed(preferences, screen, scroll);
    }
}

/// The diff-pane line `anchor` names in `view`.
fn pane_line(view: &EffectiveView, anchor: AnchorLine) -> ScreenLines {
    match anchor {
        AnchorLine::PaneTop => ScreenLines::ZERO,
        AnchorLine::PaneMiddle => view.pane_content().height.half(),
    }
}

/// What `view` shows a page's rows with at `preferences`' context count.
fn settings_of(preferences: ViewPreferences, view: &EffectiveView) -> ExtentSettings {
    ExtentSettings {
        shape: view.shape(),
        context: preferences.context,
        wrap: view.wrap(),
    }
}

/// The limits of `extent` shown in `view`: content scrolls until its end fills the pane,
/// and only unwrapped rows scroll horizontally. The vertical offset always names a line,
/// even when a too-short screen shows no pane.
fn bounds_of(extent: &ScrollExtent, view: &EffectiveView) -> ScrollBounds {
    let content = view.pane_content();
    let shown_lines = content.height.max(ScreenLines::ONE);

    ScrollBounds::new(
        extent.line_count().saturating_sub(shown_lines),
        view.wrap().horizontal_range(extent.width(), content.width),
        content.height,
    )
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Size;

    use super::{
        Navigation,
        interaction::{Effect, Placement},
        scroll::{Movement, ScrollPosition},
    };
    use crate::{
        parser::parse_patch,
        preferences::{PreferenceChange, ViewPreferences, Wrapping},
        prepared::PreparedDocument,
        screen::{Cells, ScreenLines},
    };

    /// docs/spec/architecture.md#view-model: cycling layouts and toggling detail keep the
    /// page and position, and three layout changes return to the start.
    #[test]
    fn keeps_the_page_and_position_through_kept_preference_changes() {
        let document = parse_patch(b"--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n a\n-b\n+c\n d\n")
            .expect("valid diff");
        let prepared = PreparedDocument::prepare(&document);
        let mut navigation =
            Navigation::start(prepared, ViewPreferences::INITIAL, Size::new(80, 4));
        navigation.step(Effect::Scroll(Movement::Down));
        let start = *navigation.interaction();
        let mut redisplayed = |change| {
            let effect = Effect::Redisplay {
                preferences: navigation.interaction().preferences().after(change),
                placement: Placement::Keep,
            };
            navigation.step(effect);
            *navigation.interaction()
        };

        let layouts = [PreferenceChange::NextLayout; 3].map(&mut redisplayed);
        let unified = layouts[2];
        let character = redisplayed(PreferenceChange::ToggleGranularity);

        let one_line_down = ScrollPosition {
            vertical: ScreenLines::ONE,
            horizontal: Cells::ZERO,
        };
        assert_eq!(
            start.scroll().position(),
            one_line_down,
            "the patch scrolls by a line"
        );
        assert_eq!(unified, start, "three layout changes return to the start");
        assert_eq!(character.page(), start.page());
        assert_eq!(character.scroll(), start.scroll());
    }

    /// docs/spec/architecture.md#view-model: while wrapping is enabled the maximum
    /// horizontal offset is zero, even in a pane too narrow for a payload column.
    #[test]
    fn never_scrolls_horizontally_while_wrapping_in_a_pane_without_payload_room() {
        let document = parse_patch(
            b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-a long deleted line\n+a long added line\n",
        )
        .expect("valid diff");
        let prepared = PreparedDocument::prepare(&document);
        let preferences = ViewPreferences {
            wrapping: Wrapping::Enabled,
            ..ViewPreferences::INITIAL
        };
        // The file column leaves the diff one column, all of it frame.
        let mut navigation = Navigation::start(prepared, preferences, Size::new(28, 3));

        navigation.step(Effect::Scroll(Movement::Right));
        navigation.step(Effect::Scroll(Movement::FarRight));

        assert_eq!(navigation.view().too_small(), None, "the panes still show");
        assert_eq!(
            navigation.interaction().scroll().position().horizontal,
            Cells::ZERO
        );
    }
}
