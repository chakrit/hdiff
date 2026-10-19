//! Tab expansion for interactive rows, to 2-column tab stops. Rules:
//! docs/spec/architecture.md#lossless-and-safe-rendering.

use crate::{
    compact::Count,
    screen::{Cells, NonZeroCells},
};

/// The cells between tab stops.
const TAB_WIDTH: NonZeroCells = NonZeroCells::new(Cells::new(2)).expect("tab stops are apart");

/// Expands tabs to the next multiple of `TAB_WIDTH` cells from the start of the payload,
/// across consecutive segments of one row.
#[derive(Default)]
pub struct TabStops {
    column: Cells,
}

impl TabStops {
    /// Appends `text` to `expanded` with its tabs replaced by spaces.
    pub fn expand_into(&mut self, text: &str, expanded: &mut String) {
        self.walk(text, |chunk, padding| {
            expanded.push_str(chunk);
            expanded.extend(std::iter::repeat_n(' ', padding.get()));
        });
    }

    /// Visits each tab-free chunk of `text`, then the spaces the tab that ends it expands
    /// to; a chunk without a tab ends `text`.
    fn walk<'t>(&mut self, text: &'t str, mut visit: impl FnMut(&'t str, Cells)) {
        for piece in text.split_inclusive('\t') {
            let chunk = piece.strip_suffix('\t');
            let text = chunk.unwrap_or(piece);
            self.column += Cells::of_text(text);

            let padding = match chunk {
                Some(_) => self.advance_to_tab_stop(),
                None => Cells::ZERO,
            };
            visit(text, padding);
        }
    }

    /// Moves to the next tab stop, returning the cells a tab there covers.
    fn advance_to_tab_stop(&mut self) -> Cells {
        let padding = self.column.padding_to_next_multiple_of(TAB_WIDTH);
        self.column += padding;
        padding
    }
}

/// A borrowed [`MeasuredText`]: a payload and the terminal cells it covers once its tabs
/// are expanded.
#[derive(Clone, Copy)]
pub struct MeasuredTextRef<'a> {
    text: &'a str,
    cells: Cells,
}

impl<'a> MeasuredTextRef<'a> {
    /// `text`, measured once its tabs are expanded.
    pub fn of_text(text: &'a str) -> Self {
        let mut tab_stops = TabStops::default();
        tab_stops.walk(text, |_, _| {});

        Self {
            text,
            cells: tab_stops.column,
        }
    }

    pub fn text(self) -> &'a str {
        self.text
    }

    pub fn cells(self) -> Cells {
        self.cells
    }
}

/// A payload kept with the terminal cells it covers once its tabs are expanded; only
/// measuring the text builds one, so the two cannot disagree.
#[derive(Debug, PartialEq, Eq)]
pub struct MeasuredText {
    text: Box<str>,
    cells: Count,
}

impl MeasuredText {
    /// `text`, measured once its tabs are expanded.
    pub fn of_text(text: &str) -> Self {
        let cells = MeasuredTextRef::of_text(text).cells();

        Self {
            text: text.into(),
            cells: cells.retained(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn borrowed(&self) -> MeasuredTextRef<'_> {
        MeasuredTextRef {
            text: &self.text,
            cells: Cells::of_retained(self.cells),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MeasuredTextRef, TabStops};
    use crate::screen::Cells;

    #[test]
    fn measures_halfwidth_sound_marks_as_the_frame_draws_them() {
        let mut expanded = String::new();
        TabStops::default().expand_into("ｶﾞ\tx", &mut expanded);

        // Terminals draw U+FF9E in its own cell, so `ｶﾞ` covers two cells.
        assert_eq!(MeasuredTextRef::of_text("ｶﾞ").cells(), Cells::new(2));
        assert_eq!(expanded, "ｶﾞ  x");
        assert_eq!(MeasuredTextRef::of_text("ｶﾞ\tx").cells(), Cells::new(5));
    }
}
