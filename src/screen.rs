//! Terminal measures in the two unit spaces that must not mix: cell columns across a row and
//! screen lines down a pane, and the cells text covers when the row writer draws it. They
//! convert from Ratatui's `u16` where a frame's size enters and back only where a frame is
//! drawn. Rules: `docs/spec/architecture.md` (Viewport coordinates, Lossless and safe
//! rendering).

use std::{
    num::NonZeroUsize,
    ops::{Add, AddAssign},
};

use ratatui::buffer::CellWidth;
use unicode_segmentation::UnicodeSegmentation;

/// Cells of the marker gutter that precedes every row's payload.
pub const MARKER_GUTTER_WIDTH: Cells = Cells::new(2);

/// A count or offset of terminal cells across a row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cells(usize);

impl Cells {
    pub const ZERO: Self = Self(0);

    pub const fn new(cells: usize) -> Self {
        Self(cells)
    }

    /// Cells the row writer covers drawing `text`.
    pub fn of_text(text: &str) -> Self {
        text.graphemes(true).map(Self::of_cluster).sum()
    }

    /// Cells the row writer covers drawing `cluster`, measured with Ratatui's `CellWidth`;
    /// Ratatui skips control-containing clusters, so they cover none.
    pub fn of_cluster(cluster: &str) -> Self {
        match cluster.contains(char::is_control) {
            true => Self::ZERO,
            false => Self(usize::from(cluster.cell_width())),
        }
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    /// These cells less `other`, or `None` when `other` is more.
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(cells) => Some(Self(cells)),
            None => None,
        }
    }
}

impl Add for Cells {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl AddAssign for Cells {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl std::iter::Sum for Cells {
    fn sum<I: Iterator<Item = Self>>(cells: I) -> Self {
        cells.fold(Self::ZERO, Add::add)
    }
}

impl From<u16> for Cells {
    fn from(cells: u16) -> Self {
        Self(usize::from(cells))
    }
}

/// A nonzero count of terminal cells: the width payloads wrap at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonZeroCells(NonZeroUsize);

impl NonZeroCells {
    pub const MIN: Self = Self(NonZeroUsize::MIN);

    /// `cells`, or `None` when there are none.
    pub const fn new(cells: Cells) -> Option<Self> {
        match NonZeroUsize::new(cells.0) {
            Some(cells) => Some(Self(cells)),
            None => None,
        }
    }

    pub const fn get(self) -> Cells {
        Cells(self.0.get())
    }
}

/// A count or offset of screen lines down a pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScreenLines(usize);

impl ScreenLines {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);

    pub const fn new(lines: usize) -> Self {
        Self(lines)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    /// Half of these lines, rounded down.
    pub const fn half(self) -> Self {
        Self(self.0 / 2)
    }
}

impl Add for ScreenLines {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl From<u16> for ScreenLines {
    fn from(lines: u16) -> Self {
        Self(usize::from(lines))
    }
}
