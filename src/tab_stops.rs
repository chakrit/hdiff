//! Tab expansion for interactive rows; see `docs/spec/architecture.md` (Lossless and safe
//! rendering) for the 2-column tab-stop rule.

use ratatui::buffer::CellWidth;
use unicode_segmentation::UnicodeSegmentation;

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
        for (index, chunk) in text.split('\t').enumerate() {
            if index > 0 {
                expanded.extend(std::iter::repeat_n(' ', self.advance_to_tab_stop()));
            }
            expanded.push_str(chunk);
            self.column += cells(chunk);
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

    for (index, chunk) in text.split('\t').enumerate() {
        if index > 0 {
            tab_stops.advance_to_tab_stop();
        }
        tab_stops.column += cells(chunk);
    }
    tab_stops.column
}

/// Terminal cells the row writer covers drawing `text`.
fn cells(text: &str) -> usize {
    text.graphemes(true).map(cluster_cells).sum()
}

/// Terminal cells the row writer covers drawing `cluster`, measured with Ratatui's
/// `CellWidth`; Ratatui skips control-containing clusters, so they cover none.
pub fn cluster_cells(cluster: &str) -> usize {
    match cluster.contains(char::is_control) {
        true => 0,
        false => usize::from(cluster.cell_width()),
    }
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
