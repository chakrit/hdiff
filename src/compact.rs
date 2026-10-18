//! Counts and positions retained in 32 bits to keep per-line records small. Rules:
//! `docs/spec/architecture.md` (Viewport coordinates).

/// A count, or a position counted from the start, retained as `u32` and used as `usize`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Count(u32);

impl Count {
    pub const ZERO: Self = Self(0);

    /// `value` as retained.
    ///
    /// Panics when `value` does not fit in `u32`.
    pub fn of(value: usize) -> Self {
        Self(u32::try_from(value).expect("retained counts fit in u32"))
    }

    /// The retained value as used.
    pub fn get(self) -> usize {
        usize::try_from(self.0).expect("u32 values fit in usize")
    }

    /// The distance between this count and `other`.
    pub fn abs_diff(self, other: Self) -> Self {
        Self(self.0.abs_diff(other.0))
    }
}
