//! The two files a diff compares, and values held once for each.

use std::array;

/// One of the two files a diff compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Old,
    New,
}

impl Side {
    /// Both sides, old first.
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

    /// Each side's value, old then new.
    pub fn iter(&self) -> array::IntoIter<&T, 2> {
        self.as_ref().into_iter()
    }

    /// Each side's value paired with `other`'s value for that side.
    pub fn zip<U>(self, other: BySide<U>) -> BySide<(T, U)> {
        BySide {
            old: (self.old, other.old),
            new: (self.new, other.new),
        }
    }
}

impl<T> BySide<Option<T>> {
    /// `value` on `side`, and nothing on the other side.
    pub fn only(side: Side, value: T) -> Self {
        match side {
            Side::Old => BySide {
                old: Some(value),
                new: None,
            },
            Side::New => BySide {
                old: None,
                new: Some(value),
            },
        }
    }

    /// Both sides' values when both sides hold one.
    pub fn transpose(self) -> Option<BySide<T>> {
        Some(BySide {
            old: self.old?,
            new: self.new?,
        })
    }
}

impl BySide<bool> {
    /// Each side that holds in `self`, in `other`, or in both.
    pub fn or(self, other: Self) -> Self {
        self.zip(other).map(|(this, that)| this || that)
    }
}

/// Each side's value, old then new.
impl<T> IntoIterator for BySide<T> {
    type Item = T;
    type IntoIter = array::IntoIter<T, 2>;

    fn into_iter(self) -> Self::IntoIter {
        [self.old, self.new].into_iter()
    }
}
