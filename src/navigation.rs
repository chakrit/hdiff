//! Where an event leads the display: the scroll extent the display shows, its bounds, and
//! the position its viewport policy places it at. Rules: `docs/spec/architecture.md` (View
//! model, Viewport coordinates).

use crate::{
    extent::{DisplayExtent, RowAnchor, Wrap},
    geometry::{PaneGeometry, maximum_horizontal_offset},
    interaction::{
        AnchorLine, Destination, DisplayBounds, Event, Interaction, ScrollPosition, Target,
        ViewportPolicy,
    },
    page_rows::PageRows,
    preferences::DiffLayout,
    prepared::{PageIndex, PreparedDocument},
};

/// The scroll extent of the display an interaction shows, rebuilt only when its page,
/// layout, context count, or wrap changes.
pub struct Navigation {
    key: ExtentKey,
    extent: DisplayExtent,
}

impl Navigation {
    pub fn new(document: &PreparedDocument, interaction: &Interaction) -> Self {
        let key = ExtentKey::of(interaction.page, interaction.shown());

        Self {
            key,
            extent: key.build(document),
        }
    }

    /// The scroll extent of the interaction this navigation last settled on.
    pub fn extent(&self) -> &DisplayExtent {
        &self.extent
    }

    /// The display `event` leads `interaction` to. The extent then holds that display's
    /// lines until [`settle`](Self::settle).
    pub fn destination(
        &mut self,
        document: &PreparedDocument,
        interaction: &Interaction,
        event: &Event,
    ) -> Destination {
        let target = interaction.target(event);
        let policy = event.viewport_policy();
        let rows = document.page(interaction.page).rows();
        // The focus is found among the current display's lines, before they are replaced.
        let focus = match policy {
            ViewportPolicy::Anchor(anchor) => Focus::of(interaction, anchor, rows, &self.extent),
            ViewportPolicy::Keep | ViewportPolicy::Reset => None,
        };

        self.select(document, ExtentKey::of(interaction.page, target));
        let geometry = PaneGeometry::of_size(target.screen, target.preferences);
        let bounds = DisplayBounds {
            maximum_vertical_offset: self
                .extent
                .line_count()
                .saturating_sub(usize::from(geometry.content_height())),
            maximum_horizontal_offset: maximum_horizontal_offset(
                self.extent.width(),
                geometry.content_width(),
            ),
        };
        let current = interaction.viewport.scroll;
        let start = match policy {
            ViewportPolicy::Keep => current,
            ViewportPolicy::Reset => ScrollPosition::START,
            ViewportPolicy::Anchor(_) => ScrollPosition {
                vertical: focus.map_or(0, |focus| focus.offset_in(rows, &self.extent)),
                ..current
            },
        };

        Destination::new(target, document.page_count(), bounds, start)
    }

    /// Holds the lines of the display `interaction` shows.
    pub fn settle(&mut self, document: &PreparedDocument, interaction: &Interaction) {
        self.select(
            document,
            ExtentKey::of(interaction.page, interaction.shown()),
        );
    }

    fn select(&mut self, document: &PreparedDocument, key: ExtentKey) {
        if self.key != key {
            self.key = key;
            self.extent = key.build(document);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ExtentKey {
    page: PageIndex,
    layout: DiffLayout,
    context_lines: usize,
    wrap: Wrap,
}

impl ExtentKey {
    fn of(page: PageIndex, target: Target) -> Self {
        let preferences = target.preferences;
        let geometry = PaneGeometry::of_size(target.screen, preferences);

        Self {
            page,
            layout: preferences.layout,
            context_lines: preferences.context_lines,
            wrap: Wrap::for_panes(preferences.wrapping, geometry.content_width()),
        }
    }

    fn build(self, document: &PreparedDocument) -> DisplayExtent {
        DisplayExtent::of(
            document.page(self.page).rows(),
            self.layout,
            self.context_lines,
            self.wrap,
        )
    }
}

/// The row on the diff-pane line that an anchored event keeps in place.
struct Focus {
    row: RowAnchor,
    /// Pane line the row sits on.
    pane_line: usize,
}

impl Focus {
    /// No focus when the lines end above the anchor line; the display then fits the pane.
    fn of(
        interaction: &Interaction,
        anchor: AnchorLine,
        rows: &PageRows,
        extent: &DisplayExtent,
    ) -> Option<Self> {
        let viewport = &interaction.viewport;
        let pane_line = match anchor {
            AnchorLine::PaneTop => 0,
            AnchorLine::PaneMiddle => {
                let geometry = PaneGeometry::of_size(viewport.screen, interaction.preferences);
                usize::from(geometry.content_height() / 2)
            }
        };

        extent
            .anchor_at(rows, viewport.scroll.vertical + pane_line)
            .map(|row| Self { row, pane_line })
    }

    /// The vertical offset in `extent` that puts the focused row back on its pane line.
    fn offset_in(&self, rows: &PageRows, extent: &DisplayExtent) -> usize {
        extent
            .offset_of(rows, self.row)
            .saturating_sub(self.pane_line)
    }
}
