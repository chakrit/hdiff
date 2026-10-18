use crate::{
    document::{BySide, Side},
    extent::{ScreenLine, ScrollExtent},
    page_rows::SideBySideRow,
    screen::ScreenLines,
    styling::{RowTone, StyledRow},
};

/// One pane's row: prepared text, or the blank peer that aligns a change to the other
/// side.
#[derive(Clone, Copy)]
pub(super) enum PaneRow<'a> {
    Text(StyledRow<'a>),
    /// Takes the tone of a change to this side.
    AlignmentPeer(Side),
}

impl PaneRow<'_> {
    pub(super) fn tone(&self) -> RowTone {
        match self {
            Self::Text(row) => row.tone,
            Self::AlignmentPeer(side) => RowTone::Changed(*side),
        }
    }
}

impl<'a> SideBySideRow<StyledRow<'a>> {
    /// The old and new pane rows of this row; a change to one side pairs with a blank
    /// peer toned as a change to the other side.
    pub(super) fn pane_pair(self) -> BySide<PaneRow<'a>> {
        match self {
            Self::Shared(row) => BySide {
                old: PaneRow::Text(row),
                new: PaneRow::Text(row),
            },
            Self::Paired(rows) => rows.map(PaneRow::Text),
            Self::OldOnly(old) => BySide {
                old: PaneRow::Text(old),
                new: PaneRow::AlignmentPeer(Side::New),
            },
            Self::NewOnly(new) => BySide {
                old: PaneRow::AlignmentPeer(Side::Old),
                new: PaneRow::Text(new),
            },
        }
    }
}

/// Up to `limit` screen lines of `extent` from line `first`, each with the row `row`
/// makes of its index.
pub(super) fn visible_rows<R: Copy + Ord, T>(
    extent: &ScrollExtent<R>,
    first: ScreenLines,
    limit: ScreenLines,
    row: impl Fn(R) -> T,
) -> Vec<ScreenLine<T>> {
    extent
        .lines(first, limit)
        .map(|screen_line| screen_line.map(&row))
        .collect()
}
