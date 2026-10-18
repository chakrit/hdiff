//! The effective view: the view preferences laid out on a terminal screen. Navigation
//! bounds and drawing both read this one value. Rules: `docs/spec/architecture.md` (View
//! model, Viewport coordinates).

use ratatui::layout::Size;

use crate::{
    geometry::{PaneContent, PaneShape, ScreenGeometry},
    preferences::{ViewPreferences, Wrapping},
    screen::{Cells, MARKER_GUTTER_WIDTH, NonZeroCells, ScreenLines},
};

/// The renderable combination of the view preferences and the screen size: the pane shape,
/// the screen's areas, the content each diff pane shows, the width rows wrap at, and
/// whether the screen is too small to show panes, each derived once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveView {
    shape: PaneShape,
    geometry: ScreenGeometry,
    pane_content: PaneContent,
    wrap: WrapWidth,
    too_small: Option<TooSmall>,
}

/// Why a screen shows only a message instead of its panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TooSmall {
    Narrow,
    /// A diff pane would have no content line.
    Short,
}

impl EffectiveView {
    pub fn of(preferences: ViewPreferences, screen: Size) -> Self {
        let shape = PaneShape::of(preferences.layout);
        let geometry = ScreenGeometry::of(screen, preferences.file_list);
        let pane_content = geometry.pane_content(shape);

        let too_small = match (geometry.is_narrow(), pane_content.height) {
            (true, _) => Some(TooSmall::Narrow),
            (false, ScreenLines::ZERO) => Some(TooSmall::Short),
            (false, _) => None,
        };
        Self {
            shape,
            geometry,
            pane_content,
            wrap: WrapWidth::for_panes(preferences.wrapping, pane_content.width),
            too_small,
        }
    }

    pub fn shape(&self) -> PaneShape {
        self.shape
    }

    pub fn geometry(&self) -> &ScreenGeometry {
        &self.geometry
    }

    /// The content each diff pane of the shape shows.
    pub fn pane_content(&self) -> PaneContent {
        self.pane_content
    }

    pub fn wrap(&self) -> WrapWidth {
        self.wrap
    }

    pub fn too_small(&self) -> Option<TooSmall> {
        self.too_small
    }
}

/// The width at which rows break into screen lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapWidth {
    /// Every row is one line, scrolled horizontally.
    Unbounded,
    /// Payloads wrap at this many cells.
    At(NonZeroCells),
}

impl WrapWidth {
    /// The wrap width `wrapping` selects for panes whose narrowest content is
    /// `content_width` wide: both side-by-side panes wrap at the narrower pane's payload
    /// width. A pane with no room for payload wraps one cell per line.
    fn for_panes(wrapping: Wrapping, content_width: Cells) -> Self {
        let payload_width = content_width.saturating_sub(MARKER_GUTTER_WIDTH);
        let wrap_width = NonZeroCells::new(payload_width).unwrap_or(NonZeroCells::MIN);

        match wrapping {
            Wrapping::Disabled => Self::Unbounded,
            Wrapping::Enabled => Self::At(wrap_width),
        }
    }

    /// Cells of a payload `width` cells wide that the pane must scroll through.
    pub(crate) fn visible_width(self, width: Cells) -> Cells {
        match self {
            Self::Unbounded => width,
            Self::At(cells) => width.min(cells.get()),
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Size;

    use super::{EffectiveView, TooSmall, WrapWidth};
    use crate::{
        preferences::{DiffLayout, ViewPreferences, Wrapping},
        screen::{Cells, NonZeroCells},
    };

    /// What a screen shows: a message, or its panes with or without the file list.
    #[derive(Debug, PartialEq, Eq)]
    enum Shown {
        Message(TooSmall),
        DiffOnly,
        Split,
    }

    #[test]
    fn derives_compact_pane_visibility_from_terminal_size() {
        let cases = [
            (
                (19, 3),
                DiffLayout::Unified,
                Shown::Message(TooSmall::Narrow),
            ),
            (
                (20, 2),
                DiffLayout::Unified,
                Shown::Message(TooSmall::Short),
            ),
            ((27, 3), DiffLayout::Unified, Shown::DiffOnly),
            ((28, 3), DiffLayout::Unified, Shown::Split),
            ((28, 3), DiffLayout::Vertical, Shown::Split),
            (
                (28, 6),
                DiffLayout::Stacked,
                Shown::Message(TooSmall::Short),
            ),
            ((28, 7), DiffLayout::Stacked, Shown::Split),
        ];

        // architecture.md: the minimum layout shows one content line in each diff pane.
        for ((width, height), layout, expected) in cases {
            let preferences = ViewPreferences {
                layout,
                ..ViewPreferences::INITIAL
            };
            let view = EffectiveView::of(preferences, Size::new(width, height));
            let shown = match (view.too_small(), view.geometry().file_column()) {
                (Some(too_small), _) => Shown::Message(too_small),
                (None, None) => Shown::DiffOnly,
                (None, Some(_)) => Shown::Split,
            };

            assert_eq!(shown, expected, "{width}×{height} {layout:?}");
        }
    }

    #[test]
    fn wraps_both_side_by_side_panes_at_the_narrower_payload_width() {
        let wrap = |wrapping, width| WrapWidth::for_panes(wrapping, Cells::new(width));
        let wrap_at = |cells| {
            WrapWidth::At(NonZeroCells::new(Cells::new(cells)).expect("a nonzero test wrap width"))
        };

        assert_eq!(wrap(Wrapping::Enabled, 24), wrap_at(22));
        assert_eq!(wrap(Wrapping::Enabled, 51), wrap_at(49));
        assert_eq!(wrap(Wrapping::Disabled, 51), WrapWidth::Unbounded);
        assert_eq!(wrap(Wrapping::Enabled, 78), wrap_at(76));
    }
}
