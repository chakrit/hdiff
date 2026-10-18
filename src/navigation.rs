//! Where an effect leads the interaction: the effective view and scroll extent it shows,
//! and the scroll position within their bounds. Rules: `docs/spec/architecture.md` (View
//! model, Viewport coordinates).

use ratatui::layout::Size;

use crate::{
    effective_view::EffectiveView,
    extent::{DisplayExtent, ExtentSettings},
    interaction::{AnchorLine, Effect, Interaction, Placement},
    preferences::ViewPreferences,
    prepared::PreparedDocument,
    screen::ScreenLines,
    scroll::{BoundedScroll, DisplayBounds, ScrollPosition},
};

/// The effective view and scroll extent of the interaction navigation last led to. The
/// extent is rebuilt only when its page or settings change.
pub struct Navigation {
    view: EffectiveView,
    extent: DisplayExtent,
}

impl Navigation {
    pub fn new(document: &PreparedDocument, interaction: &Interaction) -> Self {
        let view = EffectiveView::of(interaction.preferences(), interaction.screen());
        let settings = settings_of(interaction.preferences(), &view);

        Self {
            view,
            extent: DisplayExtent::of(document, interaction.page(), settings),
        }
    }

    pub fn view(&self) -> &EffectiveView {
        &self.view
    }

    pub fn extent(&self) -> &DisplayExtent {
        &self.extent
    }

    /// The interaction `effect` leads `interaction` to; the view and extent then show it.
    pub fn step(
        &mut self,
        document: &PreparedDocument,
        interaction: &Interaction,
        effect: Effect,
    ) -> Interaction {
        match effect {
            Effect::Scroll(movement) => {
                let bounds = DisplayBounds::of(&self.extent, &self.view);
                interaction.scrolled(interaction.scroll().moved(movement, bounds))
            }
            Effect::Keymap(keymap) => interaction.with_keymap(keymap),
            Effect::TurnPage(page) => {
                let preferences = interaction.preferences();
                let screen = interaction.screen();
                self.view = EffectiveView::of(preferences, screen);

                let settings = settings_of(preferences, &self.view);
                self.extent.turn_to(document, page, settings);
                interaction.shown(page, preferences, screen, BoundedScroll::START)
            }
            Effect::Redisplay {
                preferences,
                placement,
            } => self.redisplay(
                document,
                interaction,
                preferences,
                interaction.screen(),
                placement,
            ),
        }
    }

    /// `interaction` on a `screen`-sized terminal, keeping the row on the diff pane's top
    /// line there.
    pub fn resize(
        &mut self,
        document: &PreparedDocument,
        interaction: &Interaction,
        screen: Size,
    ) -> Interaction {
        let placement = Placement::Anchor(AnchorLine::PaneTop);

        self.redisplay(
            document,
            interaction,
            interaction.preferences(),
            screen,
            placement,
        )
    }

    /// The same page with `preferences` on a `screen`-sized terminal, its first line placed
    /// as `placement` asks.
    fn redisplay(
        &mut self,
        document: &PreparedDocument,
        interaction: &Interaction,
        preferences: ViewPreferences,
        screen: Size,
        placement: Placement,
    ) -> Interaction {
        let current = interaction.scroll().position();
        let view = EffectiveView::of(preferences, screen);
        let settings = settings_of(preferences, &view);

        let vertical = match placement {
            Placement::Keep => {
                self.extent.reshow(document, settings);
                current.vertical
            }
            Placement::Anchor(anchor) => {
                // An anchored row is found among the lines shown now.
                let pane_line = self.pane_line(anchor);
                let offset = current.vertical.saturating_add(pane_line);
                // None when the lines end above the anchor line, since the view then fits.
                self.extent
                    .reanchor(document, settings, offset)
                    .map_or(ScreenLines::ZERO, |line| line.saturating_sub(pane_line))
            }
        };
        self.view = view;

        let start = ScrollPosition {
            vertical,
            ..current
        };
        let scroll = DisplayBounds::of(&self.extent, &self.view).clamp(start);
        interaction.shown(interaction.page(), preferences, screen, scroll)
    }

    /// The diff-pane line `anchor` names in the view shown now.
    fn pane_line(&self, anchor: AnchorLine) -> ScreenLines {
        match anchor {
            AnchorLine::PaneTop => ScreenLines::ZERO,
            AnchorLine::PaneMiddle => self.view.pane_content().height.half(),
        }
    }
}

/// What `view` shows a page's rows with at `preferences`' context count.
fn settings_of(preferences: ViewPreferences, view: &EffectiveView) -> ExtentSettings {
    ExtentSettings {
        shape: view.shape(),
        context_lines: preferences.context_lines,
        wrap: view.wrap(),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Size;

    use super::Navigation;
    use crate::{
        interaction::{Effect, Interaction, Placement},
        parser::parse_patch,
        preferences::{PreferenceChange, ViewPreferences},
        prepared::PreparedDocument,
        screen::{Cells, ScreenLines},
        scroll::{Movement, ScrollPosition},
    };

    /// architecture.md (View model): cycling layouts and toggling detail keep the page and
    /// position, and three layout changes return to the start.
    #[test]
    fn keeps_the_page_and_position_through_kept_preference_changes() {
        let document = parse_patch(b"--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n a\n-b\n+c\n d\n")
            .expect("valid diff");
        let prepared = PreparedDocument::prepare(&document);
        let start = Interaction::start(ViewPreferences::INITIAL, Size::new(80, 4));
        let mut navigation = Navigation::new(&prepared, &start);
        let start = navigation.step(&prepared, &start, Effect::Scroll(Movement::Down));
        let mut redisplayed = |interaction: &Interaction, change| {
            let effect = Effect::Redisplay {
                preferences: interaction.preferences().after(change),
                placement: Placement::Keep,
            };
            navigation.step(&prepared, interaction, effect)
        };

        let vertical = redisplayed(&start, PreferenceChange::NextLayout);
        let stacked = redisplayed(&vertical, PreferenceChange::NextLayout);
        let unified = redisplayed(&stacked, PreferenceChange::NextLayout);
        let character = redisplayed(&start, PreferenceChange::ToggleGranularity);

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
}
