//! An iterator over whichever of two iterators a caller chose, so a function can return
//! one of two iterator types with the same item as one type.

/// One of two iterators over the same item, yielding whichever was chosen.
pub enum OneOf<A, B> {
    First(A),
    Second(B),
}

impl<A, B> Iterator for OneOf<A, B>
where
    A: Iterator,
    B: Iterator<Item = A::Item>,
{
    type Item = A::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::First(items) => items.next(),
            Self::Second(items) => items.next(),
        }
    }
}
