//! A sequence that holds at least one item, so its first and last items, its length, and
//! a walk over it need no empty case.

use std::{iter, num::NonZeroUsize, ops::Index};

/// One or more items in order.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        let mut items = items.into_iter();
        let first = items.next()?;

        Some(Self::new(first, items))
    }

    pub fn first(&self) -> &T {
        &self.first
    }

    /// The items after the first.
    pub fn rest(&self) -> &[T] {
        &self.rest
    }

    pub fn len(&self) -> NonZeroUsize {
        NonZeroUsize::MIN.saturating_add(self.rest.len())
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        iter::once(&self.first).chain(&self.rest)
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
        let middle = between.iter().map(&mut convert).collect::<Vec<_>>();

        NonEmpty::new(
            first,
            middle.into_iter().chain(iter::once(convert_last(last))),
        )
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

impl<T> IntoIterator for NonEmpty<T> {
    type Item = T;
    type IntoIter = iter::Chain<iter::Once<T>, std::vec::IntoIter<T>>;

    fn into_iter(self) -> Self::IntoIter {
        iter::once(self.first).chain(self.rest.into_vec())
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
    fn holds_collected_items_only_when_there_is_one() {
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
