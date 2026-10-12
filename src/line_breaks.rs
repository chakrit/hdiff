//! Breaks interactive row payloads into wrapped lines; see `docs/spec/architecture.md`
//! (View model) for the wrapping rules.

use unicode_segmentation::UnicodeSegmentation;

use crate::tab_stops::{TabStops, cluster_cells};

/// Places grapheme clusters on lines `width` cells wide, in order: a cluster that does
/// not fit the rest of its line starts the next one.
pub struct LineBreaks {
    width: usize,
    line: usize,
    column: usize,
}

impl LineBreaks {
    pub fn new(width: usize) -> Self {
        Self {
            width,
            line: 0,
            column: 0,
        }
    }

    /// Places the next cluster, `cells` wide, and returns its line.
    pub fn place(&mut self, cells: usize) -> usize {
        if self.column > 0 && self.column + cells > self.width {
            self.line += 1;
            self.column = 0;
        }

        self.column += cells;
        self.line
    }

    /// Lines the clusters placed so far occupy; an empty payload still takes one.
    pub fn line_count(&self) -> usize {
        self.line + 1
    }
}

/// Lines `text` wraps to at `width` cells once its tabs are expanded; `text_width` is the
/// expanded text's width in cells.
pub fn wrapped_line_count(text: &str, text_width: usize, width: usize) -> usize {
    if text.bytes().all(|byte| matches!(byte, b'\t' | b' '..=b'~')) {
        // Every cluster is one cell, so every line but the last is full.
        return text_width.div_ceil(width).max(1);
    }

    let mut expanded = String::new();
    TabStops::default().expand_into(text, &mut expanded);

    let mut breaks = LineBreaks::new(width);
    for cells in expanded.graphemes(true).map(cluster_cells) {
        if cells > 0 {
            breaks.place(cells);
        }
    }
    breaks.line_count()
}

#[cfg(test)]
mod tests {
    use super::{LineBreaks, wrapped_line_count};
    use crate::tab_stops::expanded_width;

    fn line_count(text: &str, width: usize) -> usize {
        wrapped_line_count(text, expanded_width(text), width)
    }

    #[test]
    fn breaks_before_a_cluster_that_does_not_fit_the_rest_of_its_line() {
        let mut breaks = LineBreaks::new(4);

        let lines = [1, 1, 1, 2, 2, 2].map(|cells| breaks.place(cells));

        assert_eq!(lines, [0, 0, 0, 1, 1, 2]);
        assert_eq!(breaks.line_count(), 3);
    }

    #[test]
    fn keeps_a_cluster_wider_than_the_line_on_a_line_of_its_own() {
        let mut breaks = LineBreaks::new(1);

        assert_eq!(breaks.place(1), 0);
        assert_eq!(breaks.place(2), 1);
        assert_eq!(breaks.place(1), 2);
    }

    #[test]
    fn counts_wrapped_lines_of_expanded_payloads() {
        assert_eq!(line_count("", 4), 1);
        assert_eq!(line_count("abcd", 4), 1);
        assert_eq!(line_count("abcde", 4), 2);
        assert_eq!(line_count("a\tbc", 4), 1, "the tab expands to one cell");
        assert_eq!(line_count("ab\tc", 4), 2, "the tab expands to two cells");
        assert_eq!(line_count("abc界", 4), 2, "the wide cluster moves down");
        assert_eq!(line_count("e\u{301}e\u{301}e\u{301}e\u{301}", 4), 1);
    }

    #[test]
    fn counts_single_cell_text_as_the_cluster_walk_does() {
        let text = "\tlet x = 1;\t// ascii";
        let mut breaks = LineBreaks::new(3);
        for _ in 0..expanded_width(text) {
            breaks.place(1);
        }

        assert_eq!(line_count(text, 3), breaks.line_count());
        assert_eq!(line_count(text, 300), 1);
    }
}
