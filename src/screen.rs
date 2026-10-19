//! Terminal measures in the two unit spaces that must not mix: cell columns across a row
//! and screen lines down a pane, and the cells text covers when the row writer draws it.
//! They convert from Ratatui's `u16` where a frame's size enters and back only where a
//! frame is drawn. Rules: docs/spec/architecture.md#viewport-coordinates and
//! docs/spec/architecture.md#lossless-and-safe-rendering.

use std::{
    iter::Sum,
    marker::PhantomData,
    num::NonZeroUsize,
    ops::{Add, AddAssign},
};

use ratatui::buffer::CellWidth;
use unicode_segmentation::UnicodeSegmentation;

use crate::compact::Count;

/// Clusters of the marker gutter that precedes every row's payload: the marker column,
/// then the payload spacer. Each gutter cluster covers one cell.
pub const MARKER_GUTTER_CLUSTERS: usize = 2;

/// Cells of the marker gutter: one per gutter cluster.
pub const MARKER_GUTTER_WIDTH: Cells = Cells::new(MARKER_GUTTER_CLUSTERS);

/// A count or offset in the terminal space `Space`. The space keeps a cell count from
/// mixing with a screen-line count while both share their arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Measure<Space>(usize, PhantomData<Space>);

/// The space of cell columns across a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CellSpace {}

/// The space of screen lines down a pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScreenLineSpace {}

/// A count or offset of terminal cells across a row.
pub type Cells = Measure<CellSpace>;

/// A count or offset of screen lines down a pane.
pub type ScreenLines = Measure<ScreenLineSpace>;

impl<Space> Measure<Space> {
    pub const ZERO: Self = Self::new(0);
    pub const ONE: Self = Self::new(1);

    pub const fn new(count: usize) -> Self {
        Self(count, PhantomData)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub const fn saturating_add(self, other: Self) -> Self {
        Self::new(self.0.saturating_add(other.0))
    }

    pub const fn saturating_sub(self, other: Self) -> Self {
        Self::new(self.0.saturating_sub(other.0))
    }
}

impl<Space> Add for Measure<Space> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.0 + other.0)
    }
}

impl<Space> AddAssign for Measure<Space> {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl<Space> From<u16> for Measure<Space> {
    fn from(count: u16) -> Self {
        Self::new(usize::from(count))
    }
}

impl Cells {
    /// Cells the row writer covers drawing `text`.
    pub fn of_text(text: &str) -> Self {
        text.graphemes(true).map(Self::of_cluster).sum()
    }

    /// Cells the row writer covers drawing `cluster`, measured with Ratatui's
    /// `CellWidth`; Ratatui skips control-containing clusters, so they cover none.
    pub fn of_cluster(cluster: &str) -> Self {
        match cluster.contains(char::is_control) {
            true => Self::ZERO,
            false => Self::new(usize::from(cluster.cell_width())),
        }
    }

    /// These cells in the compact form a retained record keeps.
    pub fn retained(self) -> Count {
        Count::of(self.0)
    }

    /// Cells a retained record kept in compact form.
    pub fn of_retained(retained: Count) -> Self {
        Self::new(retained.get())
    }

    /// These cells less `other`, or `None` when `other` is more.
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(cells) => Some(Self::new(cells)),
            None => None,
        }
    }

    /// Cells from this column to the next multiple of `interval` after it.
    pub const fn padding_to_next_multiple_of(self, interval: NonZeroCells) -> Self {
        let stop = interval.0.get();
        Self::new(stop - self.0 % stop)
    }

    /// Lines these cells fill at `width` when every line but the last is full; none still
    /// take one line.
    pub const fn lines_filled_at(self, width: NonZeroCells) -> ScreenLines {
        match self.0.div_ceil(width.0.get()) {
            0 => ScreenLines::ONE,
            lines => ScreenLines::new(lines),
        }
    }

    /// Each cell column from this one up to, but not including, `end`.
    pub fn columns_until(self, end: Self) -> impl Iterator<Item = Self> {
        (self.0..end.0).map(Self::new)
    }

    /// These cells as a frame's `u16` columns: the one narrowing where a frame is drawn.
    ///
    /// Panics when they do not fit a frame.
    pub fn frame_columns(self) -> u16 {
        u16::try_from(self.0).expect("cells drawn on a frame fit u16")
    }
}

impl Default for Cells {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Sum for Cells {
    fn sum<I: Iterator<Item = Self>>(cells: I) -> Self {
        cells.fold(Self::ZERO, Add::add)
    }
}

/// A nonzero count of terminal cells: the width payloads wrap at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NonZeroCells(NonZeroUsize);

impl NonZeroCells {
    /// `cells`, or `None` when there are none.
    pub const fn new(cells: Cells) -> Option<Self> {
        match NonZeroUsize::new(cells.0) {
            Some(cells) => Some(Self(cells)),
            None => None,
        }
    }

    pub const fn get(self) -> Cells {
        Cells::new(self.0.get())
    }

    /// A test's width of `cells`; panics on zero.
    #[cfg(test)]
    pub const fn of(cells: usize) -> Self {
        Self::new(Cells::new(cells)).expect("a nonzero test width")
    }
}

impl ScreenLines {
    /// Half of these lines, rounded down.
    pub const fn half(self) -> Self {
        Self::new(self.0 / 2)
    }

    /// These lines as a frame's `u16` rows: the one narrowing where a frame is drawn.
    ///
    /// Panics when they do not fit a frame.
    pub fn frame_rows(self) -> u16 {
        u16::try_from(self.0).expect("lines drawn on a frame fit u16")
    }
}
