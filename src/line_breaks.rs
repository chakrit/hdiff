//! Breaks interactive row payloads into wrapped lines; see `docs/spec/architecture.md`
//! (View model) for the wrapping rules.

use std::num::NonZeroUsize;

use unicode_segmentation::UnicodeSegmentation;

use crate::{cells::cluster_cells, page_rows::MeasuredPayload, tab_stops::TabStops};

/// Places grapheme clusters on lines `width` cells wide, in order: a cluster that does
/// not fit the rest of its line starts the next one.
pub struct LineBreaks {
    width: NonZeroUsize,
    line: usize,
    column: usize,
}

impl LineBreaks {
    pub fn new(width: NonZeroUsize) -> Self {
        Self {
            width,
            line: 0,
            column: 0,
        }
    }

    /// Places the next cluster and returns its line; a cluster that covers no cells takes
    /// no place.
    pub fn place(&mut self, cluster: &str) -> Option<usize> {
        let cells = cluster_cells(cluster);
        if cells == 0 {
            return None;
        }

        if self.column > 0 && self.column + cells > self.width.get() {
            self.line += 1;
            self.column = 0;
        }
        self.column += cells;
        Some(self.line)
    }

    /// Lines the clusters placed so far occupy; an empty payload still takes one.
    pub fn line_count(&self) -> usize {
        self.line + 1
    }
}

/// Lines `payload` wraps to at `width` cells once its tabs are expanded.
pub fn wrapped_line_count(payload: MeasuredPayload<'_>, width: NonZeroUsize) -> usize {
    let text = payload.text();
    if text.bytes().all(|byte| matches!(byte, b'\t' | b' '..=b'~')) {
        // Every cluster is one cell, so every line but the last is full.
        return payload.cells().div_ceil(width.get()).max(1);
    }

    let mut expanded = String::new();
    TabStops::default().expand_into(text, &mut expanded);

    let mut breaks = LineBreaks::new(width);
    for cluster in expanded.graphemes(true) {
        breaks.place(cluster);
    }
    breaks.line_count()
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::{LineBreaks, wrapped_line_count};
    use crate::{page_rows::MeasuredPayload, tab_stops::expanded_width};

    fn cells(width: usize) -> NonZeroUsize {
        NonZeroUsize::new(width).expect("a nonzero test width")
    }

    fn line_count(text: &str, width: usize) -> usize {
        wrapped_line_count(MeasuredPayload::new(text), cells(width))
    }

    #[test]
    fn breaks_before_a_cluster_that_does_not_fit_the_rest_of_its_line() {
        let mut breaks = LineBreaks::new(cells(4));

        let lines = ["a", "a", "a", "界", "界", "界"].map(|cluster| breaks.place(cluster));

        assert_eq!(lines, [0, 0, 0, 1, 1, 2].map(Some));
        assert_eq!(breaks.line_count(), 3);
    }

    #[test]
    fn keeps_a_cluster_wider_than_the_line_on_a_line_of_its_own() {
        let mut breaks = LineBreaks::new(cells(1));

        assert_eq!(breaks.place("a"), Some(0));
        assert_eq!(breaks.place("界"), Some(1));
        assert_eq!(breaks.place("a"), Some(2));
    }

    #[test]
    fn gives_clusters_without_cells_no_place() {
        let mut breaks = LineBreaks::new(cells(1));

        assert_eq!(breaks.place("a"), Some(0));
        assert_eq!(breaks.place("\u{7}"), None);
        assert_eq!(breaks.line_count(), 1);
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
        let mut breaks = LineBreaks::new(cells(3));
        for _ in 0..expanded_width(text) {
            breaks.place("a");
        }

        assert_eq!(line_count(text, 3), breaks.line_count());
        assert_eq!(line_count(text, 300), 1);
    }
}
