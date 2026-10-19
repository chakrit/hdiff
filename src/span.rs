//! Byte ranges of a displayed line's payload that syntax and character detail annotate.
//! Rules: docs/spec/architecture.md#lossless-and-safe-rendering.

use std::ops::Range;

/// A byte range of a payload that starts and ends on character boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadSpan {
    start: usize,
    end: usize,
}

impl PayloadSpan {
    /// The `bytes` of `payload`.
    ///
    /// Panics when the range starts after it ends, or when either bound is not a
    /// character boundary of `payload`.
    pub fn within(payload: &str, bytes: Range<usize>) -> Self {
        let Range { start, end } = bytes;
        assert!(start <= end, "a payload span starts before it ends");
        assert!(
            payload.is_char_boundary(start) && payload.is_char_boundary(end),
            "a payload span starts and ends on character boundaries"
        );

        Self { start, end }
    }

    /// The whole of `payload`.
    pub fn whole(payload: &str) -> Self {
        Self {
            start: 0,
            end: payload.len(),
        }
    }

    pub fn start(self) -> usize {
        self.start
    }

    pub fn end(self) -> usize {
        self.end
    }

    pub fn len(self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The span from this span's start to the end of `later`, a span of the same payload
    /// that ends no earlier.
    pub fn through(self, later: Self) -> Self {
        assert!(
            later.end >= self.end,
            "a payload span extends only through a span that ends no earlier"
        );

        Self {
            start: self.start,
            end: later.end,
        }
    }
}
