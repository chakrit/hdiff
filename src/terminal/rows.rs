use crate::{
    page_rows::SideBySideRow,
    side::{BySide, Side},
    styling::{RowTone, StyledRow},
};

/// One pane's row: prepared text, or the blank peer that aligns a change to the other
/// side.
#[derive(Clone, Copy)]
pub(super) enum PaneRow<'a> {
    /// A prepared row, drawn in its own style.
    Text(StyledRow<'a>),
    /// Takes the tone of a change to this side.
    AlignmentPeer(Side),
}

impl PaneRow<'_> {
    /// The row's tone: prepared text keeps its own, and an alignment peer takes the tone
    /// of a change to its side.
    pub(super) fn tone(&self) -> RowTone {
        match self {
            Self::Text(row) => row.tone(),
            Self::AlignmentPeer(side) => RowTone::Changed(*side),
        }
    }
}

impl<'a> SideBySideRow<StyledRow<'a>> {
    /// The old and new pane rows of this row; a change to one side pairs with a blank
    /// peer toned as a change to the other side.
    pub(super) fn pane_pair(self) -> BySide<PaneRow<'a>> {
        match self {
            Self::Shared(row) => BySide::both(PaneRow::Text(row)),
            Self::Paired(rows) => rows.map(PaneRow::Text),
            Self::Unmatched { side, line } => BySide::only(side, line)
                .map_sides(|pane, row| row.map_or(PaneRow::AlignmentPeer(pane), PaneRow::Text)),
        }
    }
}
