//! Where an event leads the display: the scroll extent the display shows, its bounds, and
//! the position its viewport policy places it at. Rules: `docs/spec/architecture.md` (View
//! model, Viewport coordinates).

use crate::{
    interaction::{
        AnchorLine, Destination, DisplayBounds, Event, Interaction, ScrollPosition, Target,
        ViewportPolicy,
    },
    layout::{
        ScreenLine, ScrollExtent, Wrap, content_pane_height, content_pane_width,
        maximum_horizontal_offset,
    },
    preferences::DiffLayout,
    prepared::{PageIndex, PreparedDocument},
};

/// The scroll extent of the display an interaction shows, rebuilt only when its page,
/// layout, context count, or wrap changes.
pub struct Navigation {
    key: ExtentKey,
    extent: ScrollExtent,
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
    pub fn extent(&self) -> &ScrollExtent {
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
        // The focus is found among the current display's lines, before they are replaced.
        let focus = match policy {
            ViewportPolicy::Anchor(anchor) => Focus::of(interaction, anchor, &self.extent),
            ViewportPolicy::Keep | ViewportPolicy::Reset => None,
        };

        self.select(document, ExtentKey::of(interaction.page, target));
        let bounds = display_bounds(&self.extent, target);
        let current = interaction.viewport.scroll;
        let start = match policy {
            ViewportPolicy::Keep => current,
            ViewportPolicy::Reset => ScrollPosition::START,
            ViewportPolicy::Anchor(_) => ScrollPosition {
                vertical: focus.map_or(0, |focus| focus.offset_in(&self.extent)),
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
        Self {
            page,
            layout: target.preferences.layout,
            context_lines: target.preferences.context_lines,
            wrap: Wrap::of(target.preferences, target.screen.width),
        }
    }

    fn build(self, document: &PreparedDocument) -> ScrollExtent {
        document
            .page(self.page)
            .layout()
            .scroll_extent(self.layout, self.context_lines, self.wrap)
    }
}

/// The screen line on the diff-pane line that an anchored event keeps in place.
struct Focus {
    screen_line: ScreenLine,
    /// Pane line the screen line sits on.
    pane_line: usize,
}

impl Focus {
    /// No focus when the lines end above the anchor line; the display then fits the pane.
    fn of(interaction: &Interaction, anchor: AnchorLine, extent: &ScrollExtent) -> Option<Self> {
        let viewport = &interaction.viewport;
        let pane_line = match anchor {
            AnchorLine::PaneTop => 0,
            AnchorLine::PaneMiddle => {
                content_pane_height(viewport.screen.height, interaction.preferences.layout) / 2
            }
        };

        extent
            .lines(viewport.scroll.vertical + pane_line, 1)
            .next()
            .map(|screen_line| Self {
                screen_line,
                pane_line,
            })
    }

    /// The vertical offset in `extent` that puts the focused line back on its pane line.
    fn offset_in(&self, extent: &ScrollExtent) -> usize {
        let ScreenLine { row, line } = self.screen_line;

        extent.offset_of(row, line).saturating_sub(self.pane_line)
    }
}

/// The largest offsets that keep `extent`'s lines filling the panes of `target`.
fn display_bounds(extent: &ScrollExtent, target: Target) -> DisplayBounds {
    let pane_width = content_pane_width(target.screen.width, target.preferences);
    let pane_height = content_pane_height(target.screen.height, target.preferences.layout);

    DisplayBounds {
        maximum_vertical_offset: extent.line_count().saturating_sub(pane_height),
        maximum_horizontal_offset: maximum_horizontal_offset(extent.width(), pane_width),
    }
}
