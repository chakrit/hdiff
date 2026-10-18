//! Where an effect leads the interaction: the interaction navigation holds, the effective
//! view and scroll extent it shows, and the scroll position within their bounds. Rules:
//! `docs/spec/architecture.md` (Core model, View model, Viewport coordinates).

pub mod interaction;
pub mod scroll;

use ratatui::layout::Size;

use self::{
    interaction::{AnchorLine, Effect, Interaction, Placement},
    scroll::{BoundedScroll, DisplayBounds, ScrollPosition},
};
use crate::{
    effective_view::EffectiveView,
    extent::{DisplayExtent, ExtentSettings},
    preferences::ViewPreferences,
    prepared::PreparedDocument,
    screen::ScreenLines,
};

/// The interaction a session is at, with the effective view and scroll extent that show
/// it. The extent is rebuilt only when its page or settings change.
pub struct Navigation {
    interaction: Interaction,
    view: EffectiveView,
    extent: DisplayExtent,
}

impl Navigation {
    /// The first page of `document` with `preferences` on a `screen`-sized terminal, at its
    /// start.
    pub fn start(document: &PreparedDocument, preferences: ViewPreferences, screen: Size) -> Self {
        let interaction = Interaction::start(preferences, screen);
        let view = EffectiveView::of(preferences, screen);
        let settings = settings_of(preferences, &view);

        Self {
            interaction,
            view,
            extent: DisplayExtent::of(document, interaction.page(), settings),
        }
    }

    pub fn interaction(&self) -> &Interaction {
        &self.interaction
    }

    pub fn view(&self) -> &EffectiveView {
        &self.view
    }

    pub fn extent(&self) -> &DisplayExtent {
        &self.extent
    }

    /// Leads the interaction where `effect` asks; the view and extent then show it.
    pub fn step(&mut self, document: &PreparedDocument, effect: Effect) {
        let interaction = self.interaction;

        match effect {
            Effect::Scroll(movement) => {
                let bounds = bounds_of(&self.extent, &self.view);
                self.interaction =
                    interaction.scrolled(interaction.scroll().moved(movement, bounds));
            }
            Effect::Keymap(keymap) => self.interaction = interaction.with_keymap(keymap),
            Effect::TurnPage(page) => {
                let preferences = interaction.preferences();
                // A page turn keeps the preferences and screen the view derives from.
                let settings = settings_of(preferences, &self.view);
                self.extent.turn_to(document, page, settings);
                self.interaction = interaction.shown(
                    page,
                    preferences,
                    interaction.screen(),
                    BoundedScroll::START,
                );
            }
            Effect::Redisplay {
                preferences,
                placement,
            } => self.redisplay(document, preferences, interaction.screen(), placement),
        }
    }

    /// Shows the interaction on a `screen`-sized terminal when it shows another size,
    /// keeping the row on the diff pane's top line there.
    pub fn adopt_screen(&mut self, document: &PreparedDocument, screen: Size) {
        if screen == self.interaction.screen() {
            return;
        }

        let preferences = self.interaction.preferences();
        let placement = Placement::Anchor(AnchorLine::PaneTop);
        self.redisplay(document, preferences, screen, placement);
    }

    /// Shows the same page with `preferences` on a `screen`-sized terminal, its first line
    /// placed as `placement` asks.
    fn redisplay(
        &mut self,
        document: &PreparedDocument,
        preferences: ViewPreferences,
        screen: Size,
        placement: Placement,
    ) {
        let current = self.interaction.scroll().position();
        let view = EffectiveView::of(preferences, screen);
        let settings = settings_of(preferences, &view);

        let vertical = match placement {
            Placement::Keep => {
                self.extent.reshow(document, settings);
                current.vertical
            }
            Placement::Anchor(anchor) => {
                // The row on the anchor line of the view shown now goes to the anchor line
                // of the new view.
                let source = pane_line(&self.view, anchor);
                let target = pane_line(&view, anchor);
                let offset = current.vertical.saturating_add(source);
                // None when the lines end above the anchor line, since the view then fits.
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
        let page = self.interaction.page();
        self.interaction = self.interaction.shown(page, preferences, screen, scroll);
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
        context_lines: preferences.context_lines,
        wrap: view.wrap(),
    }
}

/// The limits of `extent` shown in `view`: content scrolls until its end fills the pane.
fn bounds_of(extent: &DisplayExtent, view: &EffectiveView) -> DisplayBounds {
    let content = view.pane_content();

    DisplayBounds::new(
        extent.line_count().saturating_sub(content.height),
        extent.width().saturating_sub(content.width),
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
        preferences::{PreferenceChange, ViewPreferences},
        prepared::PreparedDocument,
        screen::{Cells, ScreenLines},
    };

    /// architecture.md (View model): cycling layouts and toggling detail keep the page and
    /// position, and three layout changes return to the start.
    #[test]
    fn keeps_the_page_and_position_through_kept_preference_changes() {
        let document = parse_patch(b"--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n a\n-b\n+c\n d\n")
            .expect("valid diff");
        let prepared = PreparedDocument::prepare(&document);
        let mut navigation =
            Navigation::start(&prepared, ViewPreferences::INITIAL, Size::new(80, 4));
        navigation.step(&prepared, Effect::Scroll(Movement::Down));
        let start = *navigation.interaction();
        let mut redisplayed = |change| {
            let effect = Effect::Redisplay {
                preferences: navigation.interaction().preferences().after(change),
                placement: Placement::Keep,
            };
            navigation.step(&prepared, effect);
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
}
