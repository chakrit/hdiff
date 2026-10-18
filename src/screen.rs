//! Terminal measures in the two unit spaces that must not mix: cell columns across a row and
//! screen lines down a pane. They convert from Ratatui's `u16` where a frame's size enters
//! and back only where a frame is drawn. Rules: `docs/spec/architecture.md` (Viewport
//! coordinates).

/// A count or offset of terminal cells across a row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cells(usize);

impl Cells {
    pub const ZERO: Self = Self(0);

    pub const fn new(cells: usize) -> Self {
        Self(cells)
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
}

impl From<u16> for Cells {
    fn from(cells: u16) -> Self {
        Self(usize::from(cells))
    }
}

/// A count or offset of screen lines down a pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScreenLines(usize);

impl ScreenLines {
    pub const ZERO: Self = Self(0);

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

impl From<u16> for ScreenLines {
    fn from(lines: u16) -> Self {
        Self(usize::from(lines))
    }
}
