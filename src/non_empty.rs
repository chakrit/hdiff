//! A sequence that holds at least one item, so its first and last items, its length, and
//! a walk over it need no empty case.

use std::{iter, num::NonZeroUsize, ops::Index};

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

    /// The items stably sorted by `key`: reordered with none lost, and items with equal
    /// keys in their original order.
    pub fn sorted_by_key<K: Ord>(self, mut key: impl FnMut(&T) -> K) -> Self {
        let Self { first, rest } = self;
        let mut rest = rest.into_vec();
        rest.sort_by_key(&mut key);

        // `first` came before every item in `rest`, so it leads the items its key ties.
        let first_key = key(&first);
        let before_first = rest.partition_point(|item| key(item) < first_key);
        let behind = rest.split_off(before_first);
        let mut ahead = rest.into_iter();

        match ahead.next() {
            None => Self::new(first, behind),
            Some(head) => Self::new(head, ahead.chain(iter::once(first)).chain(behind)),
        }
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
    fn sorts_stably_whether_or_not_the_first_item_moves() {
        let by_tens = |item: &u8| item / 10;

        let first_stays = NonEmpty::new(1, [21, 2, 11]).sorted_by_key(by_tens);
        let first_moves = NonEmpty::new(21, [3, 22, 4]).sorted_by_key(by_tens);
        let ties_keep_order = NonEmpty::new(5, [6, 7]).sorted_by_key(by_tens);

        assert_eq!(first_stays.iter().collect::<Vec<_>>(), [&1, &2, &11, &21]);
        assert_eq!(first_moves.iter().collect::<Vec<_>>(), [&3, &4, &21, &22]);
        assert_eq!(ties_keep_order.iter().collect::<Vec<_>>(), [&5, &6, &7]);
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
