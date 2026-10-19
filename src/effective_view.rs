//! The effective view: the view preferences laid out on a terminal screen. Navigation
//! bounds and drawing both read this one value. Rules:
//! docs/spec/architecture.md#view-model and
//! docs/spec/architecture.md#viewport-coordinates.

use ratatui::layout::Size;

use crate::{
    geometry::{PaneContent, PaneShape, ScreenGeometry},
    preferences::{ViewPreferences, Wrapping},
    screen::{Cells, NonZeroCells},
};

/// The renderable combination of the view preferences and the screen size: the screen's
/// areas for the pane shape, which also say whether the screen is too small to show
/// panes, and the width rows wrap at, each derived once.
pub struct EffectiveView {
    geometry: ScreenGeometry,
    wrap: WrapWidth,
}

impl EffectiveView {
    /// The view `preferences` give on a `screen`-sized terminal.
    pub fn of(preferences: ViewPreferences, screen: Size) -> Self {
        let shape = PaneShape::of(preferences.layout);
        let geometry = ScreenGeometry::of(screen, preferences.file_list, shape);

        let wrap = WrapWidth::of_panes(preferences.wrapping, geometry.pane_content());

        Self { geometry, wrap }
    }

    pub fn geometry(&self) -> &ScreenGeometry {
        &self.geometry
    }

    pub fn wrap(&self) -> WrapWidth {
        self.wrap
    }
}

/// The width at which rows break into screen lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapWidth {
    /// Wrapping is off: every row is one line, scrolled horizontally.
    Off,
    /// Payloads wrap at this many cells.
    At(NonZeroCells),
    /// Wrapping is on, but a screen too narrow to show its panes leaves them no payload
    /// column to wrap in: every row is one line, never scrolled.
    NoPayload,
}

impl WrapWidth {
    /// The wrap width `wrapping` selects for panes showing `content`: both side-by-side
    /// panes wrap at the narrower pane's payload width.
    fn of_panes(wrapping: Wrapping, content: PaneContent) -> Self {
        match (wrapping, content.payload_width()) {
            (Wrapping::Disabled, _) => Self::Off,
            (Wrapping::Enabled, Some(width)) => Self::At(width),
            (Wrapping::Enabled, None) => Self::NoPayload,
        }
    }

    /// How far rows `rows_width` cells wide scroll horizontally in panes `content_width`
    /// cells wide: only unwrapped rows scroll, until their end fills the pane.
    pub(crate) fn horizontal_range(self, rows_width: Cells, content_width: Cells) -> Cells {
        match self {
            Self::Off => rows_width.saturating_sub(content_width),
            Self::At(_) | Self::NoPayload => Cells::ZERO,
        }
    }

    /// A test's wrap width of `cells`; panics on zero.
    #[cfg(test)]
    pub const fn at(cells: usize) -> Self {
        Self::At(NonZeroCells::of(cells))
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Size;

    use super::{EffectiveView, WrapWidth};
    use crate::{
        geometry::{PaneContent, SplitDirection, SplitPanes, TooSmall},
        preferences::{DiffLayout, FileList, ViewPreferences, Wrapping},
        screen::{Cells, ScreenLines},
        side::BySide,
    };

    /// What a screen shows: a message, or its panes with or without the file list.
    #[derive(Debug, PartialEq, Eq)]
    enum Shown {
        Message(TooSmall),
        DiffOnly,
        WithFileList,
    }

    /// docs/spec/architecture.md#interaction-model: the minimum layout shows one content
    /// line in each diff pane, and the file list collapses before the diff view.
    #[test]
    fn derives_compact_pane_visibility_from_terminal_size() {
        let cases = [
            (
                Size::new(19, 3),
                DiffLayout::Unified,
                Shown::Message(TooSmall::Narrow),
            ),
            (
                Size::new(20, 2),
                DiffLayout::Unified,
                Shown::Message(TooSmall::Short),
            ),
            (Size::new(27, 3), DiffLayout::Unified, Shown::DiffOnly),
            (Size::new(28, 3), DiffLayout::Unified, Shown::DiffOnly),
            (Size::new(28, 3), DiffLayout::Vertical, Shown::DiffOnly),
            (
                Size::new(28, 6),
                DiffLayout::Stacked,
                Shown::Message(TooSmall::Short),
            ),
            (Size::new(28, 7), DiffLayout::Stacked, Shown::DiffOnly),
            (Size::new(80, 7), DiffLayout::Stacked, Shown::WithFileList),
        ];

        for (screen, layout, expected) in cases {
            let preferences = ViewPreferences {
                layout,
                ..ViewPreferences::INITIAL
            };
            let view = EffectiveView::of(preferences, screen);
            let shown = match (view.geometry().too_small(), view.geometry().file_column()) {
                (Some(too_small), _) => Shown::Message(too_small),
                (None, None) => Shown::DiffOnly,
                (None, Some(_)) => Shown::WithFileList,
            };

            assert_eq!(
                shown, expected,
                "{}×{} {layout:?}",
                screen.width, screen.height
            );
        }
    }

    /// docs/spec/architecture.md#interaction-model: as width decreases the file list
    /// collapses before the diff view, so no pane it leaves lacks a payload column.
    #[test]
    fn collapses_the_file_list_before_any_pane_loses_its_payload_column() {
        let layouts = [
            DiffLayout::Unified,
            DiffLayout::Vertical,
            DiffLayout::Stacked,
        ];
        let view_at = |layout, width| {
            let preferences = ViewPreferences {
                layout,
                ..ViewPreferences::INITIAL
            };
            EffectiveView::of(preferences, Size::new(width, 30))
        };

        for layout in layouts {
            for width in 20..=80 {
                let view = view_at(layout, width);
                let geometry = view.geometry();
                if geometry.file_column().is_some() {
                    assert!(
                        geometry.pane_content().payload_width().is_some(),
                        "{layout:?} at {width} columns shows the file list over a pane \
                         without a payload column"
                    );
                }
            }
            assert!(
                view_at(layout, 120).geometry().file_column().is_some(),
                "{layout:?} shows the file list where it fits"
            );
        }
    }

    #[test]
    fn wraps_both_side_by_side_panes_at_the_narrower_payload_width() {
        let preferences = ViewPreferences {
            layout: DiffLayout::Vertical,
            wrapping: Wrapping::Enabled,
            file_list: FileList::Hidden,
            ..ViewPreferences::INITIAL
        };
        let view = EffectiveView::of(preferences, Size::new(80, 10));
        let panes = SplitPanes::of(view.geometry().diff(), SplitDirection::Vertical).panes();

        // 79 columns beside the separator split into a 39-column and a 40-column pane,
        // whose frames leave 37 content columns, two of them the marker gutter.
        assert_eq!(panes.map(|pane| pane.width), BySide { old: 39, new: 40 });
        assert_eq!(view.wrap(), WrapWidth::at(35));
    }

    /// docs/spec/architecture.md#view-model: while wrapping is enabled the maximum
    /// horizontal offset is zero; unwrapped rows scroll until their end fills the pane.
    #[test]
    fn scrolls_horizontally_only_while_wrapping_is_off() {
        let range = |wrap: WrapWidth| wrap.horizontal_range(Cells::new(47), Cells::new(22));

        assert_eq!(range(WrapWidth::Off), Cells::new(25));
        assert_eq!(range(WrapWidth::at(20)), Cells::ZERO);
        assert_eq!(range(WrapWidth::NoPayload), Cells::ZERO);
    }

    #[test]
    fn wraps_payloads_inside_the_marker_gutter_unless_wrapping_is_off() {
        let wrap = |wrapping, width| {
            let content = PaneContent {
                width: Cells::new(width),
                height: ScreenLines::ONE,
            };
            WrapWidth::of_panes(wrapping, content)
        };

        assert_eq!(wrap(Wrapping::Enabled, 24), WrapWidth::at(22));
        assert_eq!(wrap(Wrapping::Enabled, 51), WrapWidth::at(49));
        assert_eq!(wrap(Wrapping::Disabled, 51), WrapWidth::Off);
        assert_eq!(wrap(Wrapping::Enabled, 78), WrapWidth::at(76));
        assert_eq!(wrap(Wrapping::Enabled, 2), WrapWidth::NoPayload);
    }
}
