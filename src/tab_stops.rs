//! Tab expansion for interactive rows; see `docs/spec/architecture.md` (Lossless and safe
//! rendering) for the 2-column tab-stop rule.

use crate::screen::Cells;

const TAB_WIDTH: usize = 2;

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
            expanded.extend(std::iter::repeat_n(' ', padding));
            expanded.push_str(chunk);
        });
    }

    /// Visits each tab-free chunk of `text` with the spaces the tab before it expands to;
    /// the first chunk follows no tab.
    fn walk<'t>(&mut self, text: &'t str, mut visit: impl FnMut(usize, &'t str)) {
        for (index, chunk) in text.split('\t').enumerate() {
            let padding = match index {
                0 => 0,
                _ => self.advance_to_tab_stop(),
            };
            visit(padding, chunk);
            self.column += Cells::of_text(chunk);
        }
    }

    fn advance_to_tab_stop(&mut self) -> usize {
        let padding = TAB_WIDTH - self.column.get() % TAB_WIDTH;
        self.column += Cells::new(padding);
        padding
    }
}

/// A payload and the terminal cells it covers once its tabs are expanded.
#[derive(Clone, Copy, Debug)]
pub struct MeasuredPayload<'a> {
    text: &'a str,
    cells: Cells,
}

impl<'a> MeasuredPayload<'a> {
    /// `text`, measured once its tabs are expanded.
    pub fn new(text: &'a str) -> Self {
        let mut tab_stops = TabStops::default();
        tab_stops.walk(text, |_, _| {});

        Self {
            text,
            cells: tab_stops.column,
        }
    }

    /// `text`, which an earlier measurement found to cover `cells`.
    pub fn measured(text: &'a str, cells: Cells) -> Self {
        Self { text, cells }
    }

    pub fn text(self) -> &'a str {
        self.text
    }

    pub fn cells(self) -> Cells {
        self.cells
    }
}

#[cfg(test)]
mod tests {
    use super::{MeasuredPayload, TabStops};
    use crate::screen::Cells;

    #[test]
    fn measures_halfwidth_sound_marks_as_the_frame_draws_them() {
        let mut expanded = String::new();
        TabStops::default().expand_into("ｶﾞ\tx", &mut expanded);

        // Terminals draw U+FF9E in its own cell, so `ｶﾞ` covers two cells.
        assert_eq!(MeasuredPayload::new("ｶﾞ").cells(), Cells::new(2));
        assert_eq!(expanded, "ｶﾞ  x");
        assert_eq!(MeasuredPayload::new("ｶﾞ\tx").cells(), Cells::new(5));
    }
}
