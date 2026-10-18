//! Tab expansion for interactive rows; see `docs/spec/architecture.md` (Lossless and safe
//! rendering) for the 2-column tab-stop rule.

use crate::{
    compact::Count,
    screen::{Cells, NonZeroCells},
};

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
        self.walk(text, |padding, chunk| {
            expanded.extend(std::iter::repeat_n(' ', padding.get()));
            expanded.push_str(chunk);
        });
    }

    /// Visits each tab-free chunk of `text` with the spaces the tab before it expands to;
    /// the first chunk follows no tab.
    fn walk<'t>(&mut self, text: &'t str, mut visit: impl FnMut(Cells, &'t str)) {
        for (index, chunk) in text.split('\t').enumerate() {
            let padding = match index {
                0 => Cells::ZERO,
                _ => self.advance_to_tab_stop(),
            };
            visit(padding, chunk);
            self.column += Cells::of_text(chunk);
        }
    }

    fn advance_to_tab_stop(&mut self) -> Cells {
        let padding = self.column.to_next_multiple_of(TAB_WIDTH);
        self.column += padding;
        padding
    }
}

/// A borrowed [`MeasuredText`]: a payload and the terminal cells it covers once its tabs
/// are expanded.
#[derive(Clone, Copy, Debug)]
pub struct MeasuredTextRef<'a> {
    text: &'a str,
    cells: Cells,
}

impl<'a> MeasuredTextRef<'a> {
    /// `text`, measured once its tabs are expanded.
    pub fn new(text: &'a str) -> Self {
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
    pub fn new(text: &str) -> Self {
        let cells = MeasuredTextRef::new(text).cells();

        Self {
            text: text.into(),
            cells: Count::of(cells.get()),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn borrowed(&self) -> MeasuredTextRef<'_> {
        MeasuredTextRef {
            text: &self.text,
            cells: Cells::new(self.cells.get()),
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
        assert_eq!(MeasuredTextRef::new("ｶﾞ").cells(), Cells::new(2));
        assert_eq!(expanded, "ｶﾞ  x");
        assert_eq!(MeasuredTextRef::new("ｶﾞ\tx").cells(), Cells::new(5));
    }
}
