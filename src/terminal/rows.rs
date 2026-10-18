use crate::{
    document::Side,
    extent::ScrollExtent,
    page_rows::{LineIndex, PairIndex, SideBySideRow},
    preferences::DiffGranularity,
    prepared::PreparedPage,
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
    /// The before and after pane rows of this row; a one-sided change pairs with a blank
    /// peer toned as a change to the other side.
    pub(super) fn pane_pair(self) -> (PaneRow<'a>, PaneRow<'a>) {
        match self {
            Self::Shared(row) => (PaneRow::Text(row), PaneRow::Text(row)),
            Self::Paired { before, after } => (PaneRow::Text(before), PaneRow::Text(after)),
            Self::BeforeOnly(before) => (PaneRow::Text(before), PaneRow::AlignmentPeer(Side::New)),
            Self::AfterOnly(after) => (PaneRow::AlignmentPeer(Side::Old), PaneRow::Text(after)),
        }
    }
}

/// One screen line: line `line` of `row`.
pub(super) struct VisibleLine<R> {
    pub(super) row: R,
    pub(super) line: usize,
}

pub(super) fn visible_diff_rows<'a>(
    page: &'a PreparedPage,
    extent: &ScrollExtent<LineIndex>,
    first_line: usize,
    granularity: DiffGranularity,
    row_limit: usize,
) -> Vec<VisibleLine<StyledRow<'a>>> {
    extent
        .lines(first_line, row_limit)
        .map(|screen_line| VisibleLine {
            row: page.row(screen_line.row, granularity),
            line: screen_line.wrapped_line,
        })
        .collect()
}

pub(super) fn visible_side_by_side_rows<'a>(
    page: &'a PreparedPage,
    extent: &ScrollExtent<PairIndex>,
    first_line: usize,
    granularity: DiffGranularity,
    row_limit: usize,
) -> Vec<VisibleLine<SideBySideRow<StyledRow<'a>>>> {
    let rows = page.rows();

    extent
        .lines(first_line, row_limit)
        .map(|screen_line| VisibleLine {
            row: rows
                .pair(screen_line.row)
                .map(|line| page.row(line, granularity)),
            line: screen_line.wrapped_line,
        })
        .collect()
}
