//! Tab expansion for interactive rows; see `docs/spec/architecture.md` (Lossless and safe
//! rendering) for the 2-column tab-stop rule.

use crate::cells::text_cells;

const TAB_WIDTH: usize = 2;

/// Expands tabs to the next multiple of `TAB_WIDTH` cells from the start of the payload,
/// across consecutive segments of one row.
#[derive(Default)]
pub struct TabStops {
    column: usize,
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
            self.column += text_cells(chunk);
        }
    }

    fn advance_to_tab_stop(&mut self) -> usize {
        let padding = TAB_WIDTH - self.column % TAB_WIDTH;
        self.column += padding;
        padding
    }
}

/// Terminal cells `text` occupies once its tabs are expanded.
pub fn expanded_width(text: &str) -> usize {
    let mut tab_stops = TabStops::default();
    tab_stops.walk(text, |_, _| {});
    tab_stops.column
}

#[cfg(test)]
mod tests {
    use super::{TabStops, expanded_width};

    #[test]
    fn measures_halfwidth_sound_marks_as_the_frame_draws_them() {
        let mut expanded = String::new();
        TabStops::default().expand_into("ｶﾞ\tx", &mut expanded);

        // Terminals draw U+FF9E in its own cell, so `ｶﾞ` covers two cells.
        assert_eq!(expanded_width("ｶﾞ"), 2);
        assert_eq!(expanded, "ｶﾞ  x");
        assert_eq!(expanded_width("ｶﾞ\tx"), 5);
    }
}
