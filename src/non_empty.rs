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

    pub fn first(&self) -> &T {
        &self.first
    }

    /// The items after the first.
    pub fn rest(&self) -> &[T] {
        &self.rest
    }

    pub fn last_mut(&mut self) -> &mut T {
        self.rest.last_mut().unwrap_or(&mut self.first)
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
    fn the_last_item_is_the_first_when_there_is_no_other() {
        let mut alone = NonEmpty::new(1, []);
        *alone.last_mut() = 2;

        let mut several = NonEmpty::new(1, [2, 3]);
        *several.last_mut() = 4;

        assert_eq!(alone.iter().collect::<Vec<_>>(), [&2]);
        assert_eq!(several.iter().collect::<Vec<_>>(), [&1, &2, &4]);
    }

    #[test]
    fn indexes_and_counts_the_first_item_with_the_rest() {
        let items = NonEmpty::new('a', ['b', 'c']);

        assert_eq!([items[0], items[1], items[2]], ['a', 'b', 'c']);
        assert_eq!(items.len().get(), 3);
    }
}
