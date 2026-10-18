//! Breaks interactive row payloads into wrapped lines; see `docs/spec/architecture.md`
//! (View model) for the wrapping rules.

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    screen::{Cells, NonZeroCells, ScreenLines},
    tab_stops::{MeasuredPayload, TabStops},
};

/// Places grapheme clusters on lines `width` cells wide, in order: a cluster that does
/// not fit the rest of its line starts the next one.
pub struct LineBreaks {
    width: NonZeroCells,
    line: ScreenLines,
    column: Cells,
}

impl LineBreaks {
    pub fn new(width: NonZeroCells) -> Self {
        Self {
            width,
            line: ScreenLines::ZERO,
            column: Cells::ZERO,
        }
    }

    /// Places the next cluster and returns its line; a cluster that covers no cells takes
    /// no place.
    pub fn place(&mut self, cluster: &str) -> Option<ScreenLines> {
        let cells = Cells::of_cluster(cluster);
        if cells == Cells::ZERO {
            return None;
        }

        if self.column > Cells::ZERO && self.column + cells > self.width.get() {
            self.line = self.line + ScreenLines::ONE;
            self.column = Cells::ZERO;
        }
        self.column += cells;
        Some(self.line)
    }

    /// Lines the clusters placed so far occupy; an empty payload still takes one.
    pub fn line_count(&self) -> ScreenLines {
        self.line + ScreenLines::ONE
    }
}

/// Lines `payload` wraps to at `width` cells once its tabs are expanded.
pub fn wrapped_line_count(payload: MeasuredPayload<'_>, width: NonZeroCells) -> ScreenLines {
    let text = payload.text();
    if text.bytes().all(|byte| matches!(byte, b'\t' | b' '..=b'~')) {
        // Every cluster is one cell, so every line but the last is full.
        let lines = payload.cells().get().div_ceil(width.get().get()).max(1);
        return ScreenLines::new(lines);
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
    use super::{LineBreaks, wrapped_line_count};
    use crate::{
        screen::{Cells, NonZeroCells, ScreenLines},
        tab_stops::MeasuredPayload,
    };

    fn width(cells: usize) -> NonZeroCells {
        NonZeroCells::new(Cells::new(cells)).expect("a nonzero test width")
    }

    fn line_count(text: &str, cells: usize) -> usize {
        wrapped_line_count(MeasuredPayload::new(text), width(cells)).get()
    }

    fn placed(line: usize) -> Option<ScreenLines> {
        Some(ScreenLines::new(line))
    }

    #[test]
    fn breaks_before_a_cluster_that_does_not_fit_the_rest_of_its_line() {
        let mut breaks = LineBreaks::new(width(4));

        let lines = ["a", "a", "a", "界", "界", "界"].map(|cluster| breaks.place(cluster));

        assert_eq!(lines, [0, 0, 0, 1, 1, 2].map(placed));
        assert_eq!(breaks.line_count(), ScreenLines::new(3));
    }

    #[test]
    fn keeps_a_cluster_wider_than_the_line_on_a_line_of_its_own() {
        let mut breaks = LineBreaks::new(width(1));

        assert_eq!(breaks.place("a"), placed(0));
        assert_eq!(breaks.place("界"), placed(1));
        assert_eq!(breaks.place("a"), placed(2));
    }

    #[test]
    fn gives_clusters_without_cells_no_place() {
        let mut breaks = LineBreaks::new(width(1));

        assert_eq!(breaks.place("a"), placed(0));
        assert_eq!(breaks.place("\u{7}"), None);
        assert_eq!(breaks.line_count(), ScreenLines::new(1));
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
        let mut breaks = LineBreaks::new(width(3));
        for _ in 0..MeasuredPayload::new(text).cells().get() {
            breaks.place("a");
        }

        assert_eq!(line_count(text, 3), breaks.line_count().get());
        assert_eq!(line_count(text, 300), 1);
    }
}
