//! The effective view: the view preferences laid out on a terminal screen. Navigation
//! bounds and drawing both read this one value. Rules: `docs/spec/architecture.md` (View
//! model, Viewport coordinates).

use ratatui::layout::Size;

use crate::{
    extent::WrapWidth,
    geometry::{PaneContent, PaneShape, ScreenGeometry},
    preferences::ViewPreferences,
    screen::ScreenLines,
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

#[cfg(test)]
mod tests {
    use ratatui::layout::Size;

    use super::{EffectiveView, TooSmall};
    use crate::preferences::{DiffLayout, ViewPreferences};

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
}
