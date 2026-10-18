//! The two files a diff compares, and values held once for each.

/// One of the two files a diff compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Old,
    New,
}

impl Side {
    pub const BOTH: [Self; 2] = [Self::Old, Self::New];
}

/// One value for each side of a change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BySide<T> {
    pub old: T,
    pub new: T,
}

impl<T> BySide<T> {
    pub fn get(&self, side: Side) -> &T {
        match side {
            Side::Old => &self.old,
            Side::New => &self.new,
        }
    }

    pub fn get_mut(&mut self, side: Side) -> &mut T {
        match side {
            Side::Old => &mut self.old,
            Side::New => &mut self.new,
        }
    }

    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> BySide<U> {
        BySide {
            old: f(self.old),
            new: f(self.new),
        }
    }

    /// Each side's value transformed by `f`, which is told the side.
    pub fn map_sides<U>(self, mut f: impl FnMut(Side, T) -> U) -> BySide<U> {
        BySide {
            old: f(Side::Old, self.old),
            new: f(Side::New, self.new),
        }
    }

    pub fn as_ref(&self) -> BySide<&T> {
        BySide {
            old: &self.old,
            new: &self.new,
        }
    }

    /// Each side's value paired with `other`'s value for that side.
    pub fn zip<U>(self, other: BySide<U>) -> BySide<(T, U)> {
        BySide {
            old: (self.old, other.old),
            new: (self.new, other.new),
        }
    }
}
