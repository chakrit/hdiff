//! Where an event leads the display: the scroll extent the display shows, its bounds, and
//! the position its placement puts it at. Rules: `docs/spec/architecture.md` (View model,
//! Viewport coordinates).

use crate::{
    extent::{DisplayExtent, RowAnchor, WrapWidth},
    geometry::PaneShape,
    interaction::{
        AnchorLine, Destination, Display, DisplayBounds, Effect, Event, Interaction, Placement,
        ScrollPosition, Transition,
    },
    page_rows::PageRows,
    prepared::{PageIndex, PreparedDocument},
    screen::{Cells, ScreenLines},
};

/// The scroll extent of the display an interaction shows, rebuilt only when its page,
/// pane shape, context count, or wrap width changes.
pub struct Navigation {
    shown: Shown,
}

/// A scroll extent and the key it was built for.
struct Shown {
    key: ExtentKey,
    extent: DisplayExtent,
}

impl Navigation {
    pub fn new(document: &PreparedDocument, interaction: &Interaction) -> Self {
        Self {
            shown: ExtentKey::of(interaction.page(), interaction.display()).build(document),
        }
    }

    /// The scroll extent of the interaction this navigation last stepped to.
    pub fn extent(&self) -> &DisplayExtent {
        &self.shown.extent
    }

    /// The transition `event` makes from `interaction`; on a redraw, the extent then holds
    /// the lines of the interaction it leads to.
    pub fn step(
        &mut self,
        document: &PreparedDocument,
        interaction: &Interaction,
        event: Event,
    ) -> Transition {
        let (destination, rebuilt) = self.resolve(document, interaction, event);

        let transition = destination.transition(interaction);

        if let (Transition::Redraw(_), Some(shown)) = (&transition, rebuilt) {
            self.shown = shown;
        }
        transition
    }

    /// The destination of `event`, and the extent it shows when that is not the current one.
    fn resolve(
        &self,
        document: &PreparedDocument,
        interaction: &Interaction,
        event: Event,
    ) -> (Destination, Option<Shown>) {
        match interaction.effect(event, document.page_count()) {
            Effect::Exit => (Destination::EXIT, None),
            Effect::Keymap(keymap) => (Destination::keymap(keymap), None),
            Effect::Scroll(movement) => {
                let bounds = bounds(&self.shown.extent, interaction.display());
                (Destination::scroll(movement, bounds), None)
            }
            Effect::TurnPage(page) => {
                let display = *interaction.display();
                self.shown_with(document, ExtentKey::of(page, &display), |extent| {
                    let bounds = bounds(extent, &display);
                    Destination::show(page, display, ScrollPosition::START, bounds)
                })
            }
            Effect::Redisplay { display, placement } => {
                let page = interaction.page();
                let rows = document.page(page).rows();
                // An anchored row is found among the lines shown now, on the same page.
                let focus = match placement {
                    Placement::Anchor(anchor) => {
                        Focus::of(interaction, anchor, rows, &self.shown.extent)
                    }
                    Placement::Keep => None,
                };
                let current = interaction.scroll();

                self.shown_with(document, ExtentKey::of(page, &display), |extent| {
                    let vertical = match (placement, &focus) {
                        (Placement::Keep, _) => current.vertical,
                        (Placement::Anchor(_), Some(focus)) => focus.offset_in(rows, extent),
                        (Placement::Anchor(_), None) => ScreenLines::ZERO,
                    };
                    let start = ScrollPosition {
                        vertical,
                        ..current
                    };
                    Destination::show(page, display, start, bounds(extent, &display))
                })
            }
        }
    }

    /// `destination` of the extent for `key`, reusing the current extent when it has that key.
    fn shown_with(
        &self,
        document: &PreparedDocument,
        key: ExtentKey,
        destination: impl FnOnce(&DisplayExtent) -> Destination,
    ) -> (Destination, Option<Shown>) {
        match self.shown.key == key {
            true => (destination(&self.shown.extent), None),
            false => {
                let shown = key.build(document);
                (destination(&shown.extent), Some(shown))
            }
        }
    }
}

/// The limits of `extent` shown as `display`.
fn bounds(extent: &DisplayExtent, display: &Display) -> DisplayBounds {
    let content = display.pane_content();

    DisplayBounds {
        maximum_vertical_offset: extent.line_count().saturating_sub(content.height),
        maximum_horizontal_offset: maximum_horizontal_offset(extent.width(), content.width),
        pane_lines: content.height,
    }
}

/// The largest horizontal offset that keeps content `content_width` wide filling a pane
/// `pane_width` wide.
fn maximum_horizontal_offset(content_width: Cells, pane_width: Cells) -> Cells {
    match pane_width {
        Cells::ZERO => Cells::ZERO,
        _ => content_width.saturating_sub(pane_width),
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
    fn of(page: PageIndex, display: &Display) -> Self {
        let preferences = display.preferences();

        Self {
            page,
            shape: display.shape(),
            context_lines: preferences.context_lines,
            wrap: WrapWidth::for_panes(preferences.wrapping, display.pane_content().width),
        }
    }

    fn build(self, document: &PreparedDocument) -> Shown {
        Shown {
            key: self,
            extent: DisplayExtent::of(
                document.page(self.page).rows(),
                self.shape,
                self.context_lines,
                self.wrap,
            ),
        }
    }
}

/// The row on the diff-pane line that an anchored event keeps in place.
struct Focus {
    row: RowAnchor,
    /// Pane line the row sits on.
    pane_line: ScreenLines,
}

impl Focus {
    /// No focus when the lines end above the anchor line; the display then fits the pane.
    fn of(
        interaction: &Interaction,
        anchor: AnchorLine,
        rows: &PageRows,
        extent: &DisplayExtent,
    ) -> Option<Self> {
        let pane_line = match anchor {
            AnchorLine::PaneTop => ScreenLines::ZERO,
            AnchorLine::PaneMiddle => interaction.display().pane_content().height.half(),
        };
        let offset = interaction.scroll().vertical.saturating_add(pane_line);

        extent
            .anchor_at(rows, offset)
            .map(|row| Self { row, pane_line })
    }

    /// The vertical offset in `extent` that puts the focused row back on its pane line.
    fn offset_in(&self, rows: &PageRows, extent: &DisplayExtent) -> ScreenLines {
        extent
            .offset_of(rows, self.row)
            .saturating_sub(self.pane_line)
    }
}

#[cfg(test)]
mod tests {
    use super::maximum_horizontal_offset;
    use crate::screen::Cells;

    #[test]
    fn bounds_horizontal_movement_to_visible_content() {
        let offset =
            |content, pane| maximum_horizontal_offset(Cells::new(content), Cells::new(pane));

        assert_eq!(offset(40, 20), Cells::new(20));
        assert_eq!(offset(10, 20), Cells::ZERO);
        assert_eq!(offset(40, 0), Cells::ZERO);
        assert_eq!(
            offset(usize::MAX, 1),
            Cells::new(usize::MAX - 1),
            "no terminal offset type caps how far a row reaches"
        );
    }
}
