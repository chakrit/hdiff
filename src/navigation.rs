//! Where an effect leads the interaction: the effective view and scroll extent it shows,
//! their bounds, and the scroll position within them. Rules: `docs/spec/architecture.md`
//! (View model, Viewport coordinates).

use ratatui::layout::Size;

use crate::{
    effective_view::EffectiveView,
    extent::{DisplayExtent, RowAnchor, WrapWidth},
    geometry::PaneShape,
    interaction::{AnchorLine, Effect, Interaction, Movement, Placement, ScrollPosition},
    page_rows::PageRows,
    preferences::ViewPreferences,
    prepared::{PageIndex, PreparedDocument},
    screen::{Cells, ScreenLines},
};

/// The effective view and scroll extent of the interaction navigation last led to. The
/// extent is rebuilt only when its page, pane shape, context count, or wrap width changes.
pub struct Navigation {
    view: EffectiveView,
    key: ExtentKey,
    extent: DisplayExtent,
}

impl Navigation {
    pub fn new(document: &PreparedDocument, interaction: &Interaction) -> Self {
        let view = EffectiveView::of(interaction.preferences(), interaction.screen());
        let key = ExtentKey::of(interaction.page(), interaction.preferences(), &view);

        Self {
            view,
            key,
            extent: key.build(document),
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
        let current = interaction.scroll();

        match effect {
            Effect::Scroll(movement) => {
                interaction.scrolled(moved(movement, current, self.bounds()))
            }
            Effect::Keymap(keymap) => interaction.with_keymap(keymap),
            Effect::TurnPage(page) => {
                let preferences = interaction.preferences();
                let screen = interaction.screen();

                self.show(document, page, preferences, screen);
                // The start of a page is within every bound.
                interaction.shown(page, preferences, screen, ScrollPosition::START)
            }
            Effect::Redisplay {
                preferences,
                screen,
                placement,
            } => {
                let page = interaction.page();
                let rows = document.page(page).rows();
                // An anchored row is found among the lines shown now.
                let focus = match placement {
                    Placement::Anchor(anchor) => self.focus(anchor, current, rows),
                    Placement::Keep => None,
                };

                self.show(document, page, preferences, screen);

                let vertical = match (placement, focus) {
                    (Placement::Keep, _) => current.vertical,
                    (Placement::Anchor(_), Some(focus)) => focus.offset_in(rows, &self.extent),
                    (Placement::Anchor(_), None) => ScreenLines::ZERO,
                };
                let start = ScrollPosition {
                    vertical,
                    ..current
                };
                interaction.shown(page, preferences, screen, clamped(start, self.bounds()))
            }
        }
    }

    /// Shows `page` with `preferences` on a `screen`-sized terminal, reusing the extent when
    /// its key is unchanged.
    fn show(
        &mut self,
        document: &PreparedDocument,
        page: PageIndex,
        preferences: ViewPreferences,
        screen: Size,
    ) {
        let view = EffectiveView::of(preferences, screen);
        let key = ExtentKey::of(page, preferences, &view);

        if key != self.key {
            self.extent = key.build(document);
            self.key = key;
        }
        self.view = view;
    }

    fn bounds(&self) -> DisplayBounds {
        DisplayBounds::of(&self.extent, &self.view)
    }

    /// The row of `rows` on the diff-pane line `anchor` names, scrolled to `scroll`; none
    /// when the lines end above that line, since the view then fits the pane.
    fn focus(&self, anchor: AnchorLine, scroll: ScrollPosition, rows: &PageRows) -> Option<Focus> {
        let pane_line = match anchor {
            AnchorLine::PaneTop => ScreenLines::ZERO,
            AnchorLine::PaneMiddle => self.view.pane_content().height.half(),
        };
        let offset = scroll.vertical.saturating_add(pane_line);

        self.extent
            .anchor_at(rows, offset)
            .map(|row| Focus { row, pane_line })
    }
}

/// The limits of a scroll position, and the lines one pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayBounds {
    maximum_vertical_offset: ScreenLines,
    maximum_horizontal_offset: Cells,
    pane_lines: ScreenLines,
}

impl DisplayBounds {
    /// The limits of `extent` shown in `view`: content scrolls until its end fills the pane.
    fn of(extent: &DisplayExtent, view: &EffectiveView) -> Self {
        let content = view.pane_content();

        Self {
            maximum_vertical_offset: extent.line_count().saturating_sub(content.height),
            maximum_horizontal_offset: extent.width().saturating_sub(content.width),
            pane_lines: content.height,
        }
    }
}

/// `scroll` after `movement`, within `bounds`.
fn moved(movement: Movement, scroll: ScrollPosition, bounds: DisplayBounds) -> ScrollPosition {
    let horizontal = |horizontal| ScrollPosition {
        horizontal,
        ..scroll
    };
    let vertical = |vertical| ScrollPosition { vertical, ..scroll };
    let one_cell = Cells::new(1);
    let one_line = ScreenLines::new(1);
    let half_page = bounds.pane_lines.half().max(one_line);

    let moved = match movement {
        Movement::Left => horizontal(scroll.horizontal.saturating_sub(one_cell)),
        Movement::Right => horizontal(scroll.horizontal.saturating_add(one_cell)),
        Movement::FarLeft => horizontal(Cells::ZERO),
        Movement::FarRight => horizontal(bounds.maximum_horizontal_offset),
        Movement::Down => vertical(scroll.vertical.saturating_add(one_line)),
        Movement::Up => vertical(scroll.vertical.saturating_sub(one_line)),
        Movement::HalfPageDown => vertical(scroll.vertical.saturating_add(half_page)),
        Movement::HalfPageUp => vertical(scroll.vertical.saturating_sub(half_page)),
        Movement::Top => vertical(ScreenLines::ZERO),
        Movement::Bottom => vertical(bounds.maximum_vertical_offset),
    };

    clamped(moved, bounds)
}

fn clamped(scroll: ScrollPosition, bounds: DisplayBounds) -> ScrollPosition {
    ScrollPosition {
        vertical: scroll.vertical.min(bounds.maximum_vertical_offset),
        horizontal: scroll.horizontal.min(bounds.maximum_horizontal_offset),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ExtentKey {
    page: PageIndex,
    shape: PaneShape,
    context_lines: usize,
    wrap: WrapWidth,
}

impl ExtentKey {
    fn of(page: PageIndex, preferences: ViewPreferences, view: &EffectiveView) -> Self {
        Self {
            page,
            shape: view.shape(),
            context_lines: preferences.context_lines,
            wrap: view.wrap(),
        }
    }

    fn build(self, document: &PreparedDocument) -> DisplayExtent {
        DisplayExtent::of(
            document.page(self.page).rows(),
            self.shape,
            self.context_lines,
            self.wrap,
        )
    }
}

/// The row on the diff-pane line that an anchored effect keeps in place.
struct Focus {
    row: RowAnchor,
    /// Pane line the row sits on.
    pane_line: ScreenLines,
}

impl Focus {
    /// The vertical offset in `extent` that puts the focused row back on its pane line.
    fn offset_in(&self, rows: &PageRows, extent: &DisplayExtent) -> ScreenLines {
        extent
            .offset_of(rows, self.row)
            .saturating_sub(self.pane_line)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Size;

    use super::{DisplayBounds, Navigation, moved};
    use crate::{
        interaction::{Effect, Interaction, Movement, Placement, ScrollPosition},
        parser::parse_patch,
        preferences::{PreferenceChange, ViewPreferences},
        prepared::PreparedDocument,
        screen::{Cells, ScreenLines},
    };

    fn scroll(vertical: usize, horizontal: usize) -> ScrollPosition {
        ScrollPosition {
            vertical: ScreenLines::new(vertical),
            horizontal: Cells::new(horizontal),
        }
    }

    fn bounds(maximum_vertical_offset: usize, maximum_horizontal_offset: usize) -> DisplayBounds {
        DisplayBounds {
            maximum_vertical_offset: ScreenLines::new(maximum_vertical_offset),
            maximum_horizontal_offset: Cells::new(maximum_horizontal_offset),
            pane_lines: ScreenLines::new(8),
        }
    }

    #[test]
    fn moves_and_clamps_the_vertical_viewport() {
        let down = moved(Movement::Down, scroll(4, 2), bounds(5, 4));
        let clamped = moved(Movement::Down, down, bounds(5, 4));

        assert_eq!(down, scroll(5, 2));
        assert_eq!(clamped, scroll(5, 2));
    }

    #[test]
    fn moves_and_clamps_the_horizontal_viewport() {
        let limits = bounds(7, 3);

        let right = moved(Movement::Right, scroll(4, 2), limits);
        let clamped = moved(Movement::Right, right, limits);
        let left = moved(Movement::Left, clamped, limits);

        assert_eq!(right, scroll(4, 3));
        assert_eq!(clamped, scroll(4, 3));
        assert_eq!(left, scroll(4, 2));
    }

    #[test]
    fn jumps_to_horizontal_boundaries() {
        let limits = bounds(7, 12);

        assert_eq!(moved(Movement::FarLeft, scroll(4, 2), limits), scroll(4, 0));
        assert_eq!(
            moved(Movement::FarRight, scroll(4, 2), limits),
            scroll(4, 12)
        );
    }

    /// architecture.md (Keyboard bindings): half-page movement moves half of a pane's lines.
    #[test]
    fn moves_half_a_pane_by_half_page() {
        let limits = DisplayBounds {
            pane_lines: ScreenLines::new(15),
            ..bounds(100, 0)
        };

        let down = moved(Movement::HalfPageDown, scroll(4, 0), limits);
        let up = moved(Movement::HalfPageUp, down, limits);

        assert_eq!(down, scroll(11, 0));
        assert_eq!(up, scroll(4, 0));
    }

    /// architecture.md (Lossless and safe rendering): no terminal offset type caps how far a
    /// row reaches.
    #[test]
    fn bounds_horizontal_movement_to_the_widest_row() {
        let wide = DisplayBounds {
            maximum_horizontal_offset: Cells::new(usize::MAX - 1),
            ..bounds(0, 0)
        };

        assert_eq!(
            moved(Movement::FarRight, ScrollPosition::START, wide),
            scroll(0, usize::MAX - 1)
        );
    }

    /// architecture.md (View model): cycling layouts and toggling detail keep the page and
    /// position, and three layout changes return to the start.
    #[test]
    fn keeps_the_page_and_position_through_kept_preference_changes() {
        let document = parse_patch(b"--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n a\n-b\n+c\n d\n")
            .expect("valid diff");
        let prepared = PreparedDocument::prepare(&document);
        let screen = Size::new(80, 4);
        let start = Interaction::start(ViewPreferences::INITIAL, screen);
        let mut navigation = Navigation::new(&prepared, &start);
        let start = navigation.step(&prepared, &start, Effect::Scroll(Movement::Down));
        let mut redisplayed = |interaction: &Interaction, change| {
            let effect = Effect::Redisplay {
                preferences: interaction.preferences().after(change),
                screen,
                placement: Placement::Keep,
            };
            navigation.step(&prepared, interaction, effect)
        };

        let vertical = redisplayed(&start, PreferenceChange::NextLayout);
        let stacked = redisplayed(&vertical, PreferenceChange::NextLayout);
        let unified = redisplayed(&stacked, PreferenceChange::NextLayout);
        let character = redisplayed(&start, PreferenceChange::ToggleGranularity);

        assert_eq!(start.scroll(), scroll(1, 0), "the patch scrolls by a line");
        assert_eq!(unified, start, "three layout changes return to the start");
        assert_eq!(character.page(), start.page());
        assert_eq!(character.scroll(), start.scroll());
    }
}
