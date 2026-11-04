//! A sequence that holds at least one item, so its first and last items, its length, and
//! a walk over it need no empty case.

use std::{
    iter,
    num::NonZeroUsize,
    ops::{Index, IndexMut},
};

/// One or more items in order.
#[derive(Debug, PartialEq, Eq)]
pub struct NonEmpty<T> {
    first: T,
    rest: Box<[T]>,
}

impl<T> NonEmpty<T> {
    /// `first` followed by `rest`.
    pub fn new(first: T, rest: impl IntoIterator<Item = T>) -> Self {
        Self {
            first,
            rest: rest.into_iter().collect(),
        }
    }

    /// `items` in order, or `None` when there are none.
    pub fn of(items: impl IntoIterator<Item = T>) -> Option<Self> {
        let mut item_iter = items.into_iter();
        let first = item_iter.next()?;

        Some(Self::new(first, item_iter))
    }

    pub fn first(&self) -> &T {
        &self.first
    }

    pub fn len(&self) -> NonZeroUsize {
        NonZeroUsize::MIN.saturating_add(self.rest.len())
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.into_iter()
    }

    /// Each item mapped by `convert`, in order.
    pub fn map<'a, U>(&'a self, mut convert: impl FnMut(&'a T) -> U) -> NonEmpty<U> {
        let first = convert(&self.first);

        NonEmpty::new(first, self.rest.iter().map(convert))
    }

    /// Each item mapped by `convert` in order, except the last, which `convert_last`
    /// maps.
    pub fn map_with_last<'a, U>(
        &'a self,
        mut convert: impl FnMut(&'a T) -> U,
        convert_last: impl FnOnce(&'a T) -> U,
    ) -> NonEmpty<U> {
        let Some((last, between)) = self.rest.split_last() else {
            return NonEmpty::new(convert_last(&self.first), []);
        };

        let first = convert(&self.first);
        let rest = between
            .iter()
            .map(convert)
            .chain(iter::once_with(|| convert_last(last)));

        NonEmpty::new(first, rest)
    }
}

/// Panics when `index` is not below [`len`](NonEmpty::len), as a slice does.
impl<T> Index<usize> for NonEmpty<T> {
    type Output = T;

    fn index(&self, index: usize) -> &T {
        match index.checked_sub(1) {
            None => &self.first,
            Some(rest_index) => &self.rest[rest_index],
        }
    }
}

impl<'a, T> IntoIterator for &'a NonEmpty<T> {
    type Item = &'a T;
    type IntoIter = iter::Chain<iter::Once<&'a T>, std::slice::Iter<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        iter::once(&self.first).chain(self.rest.iter())
    }
}

impl<T> IntoIterator for NonEmpty<T> {
    type Item = T;
    type IntoIter = iter::Chain<iter::Once<T>, std::vec::IntoIter<T>>;

    fn into_iter(self) -> Self::IntoIter {
        iter::once(self.first).chain(self.rest.into_vec())
    }
}

/// One or more items in order, to which more can be appended, such as the pages of a
/// document whose files are still arriving.
pub struct Growing<T> {
    first: T,
    rest: Vec<T>,
}

impl<T> Growing<T> {
    /// `first`, with more to come.
    pub fn new(first: T) -> Self {
        Self {
            first,
            rest: Vec::new(),
        }
    }

    pub fn push(&mut self, item: T) {
        self.rest.push(item);
    }

    pub fn len(&self) -> NonZeroUsize {
        NonZeroUsize::MIN.saturating_add(self.rest.len())
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        iter::once(&self.first).chain(&self.rest)
    }

    pub fn last_mut(&mut self) -> &mut T {
        self.rest.last_mut().unwrap_or(&mut self.first)
    }
}

impl<T> From<NonEmpty<T>> for Growing<T> {
    fn from(items: NonEmpty<T>) -> Self {
        Self {
            first: items.first,
            rest: items.rest.into_vec(),
        }
    }
}

/// Panics when `index` is not below [`len`](Growing::len), as a slice does.
impl<T> Index<usize> for Growing<T> {
    type Output = T;

    fn index(&self, index: usize) -> &T {
        match index {
            0 => &self.first,
            later => &self.rest[later - 1],
        }
    }
}

/// Panics when `index` is not below [`len`](Growing::len), as a slice does.
impl<T> IndexMut<usize> for Growing<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        match index {
            0 => &mut self.first,
            later => &mut self.rest[later - 1],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NonEmpty;

    #[test]
    fn maps_the_last_item_apart_from_the_others() {
        let several = NonEmpty::new('a', ['b', 'c']);
        let one = NonEmpty::new('a', []);

        let mapped = several.map_with_last(|item| item.to_ascii_uppercase(), |item| *item);
        let only = one.map_with_last(|item| item.to_ascii_uppercase(), |item| *item);

        assert_eq!(mapped.iter().collect::<Vec<_>>(), [&'A', &'B', &'c']);
        assert_eq!(only.iter().collect::<Vec<_>>(), [&'a']);
    }

    #[test]
    fn holds_collected_items_only_when_there_is_at_least_one() {
        let none = NonEmpty::<u8>::of([]);
        let some = NonEmpty::of([1, 2]).expect("two items");

        assert!(none.is_none(), "{none:?}");
        assert_eq!(some.iter().collect::<Vec<_>>(), [&1, &2]);
    }

    #[test]
    fn indexes_and_counts_the_first_item_with_the_rest() {
        let items = NonEmpty::new('a', ['b', 'c']);

        assert_eq!([items[0], items[1], items[2]], ['a', 'b', 'c']);
        assert_eq!(items.len().get(), 3);
    }
}
