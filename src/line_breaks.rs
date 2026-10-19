//! Breaks interactive row payloads into wrapped lines. Rules:
//! docs/spec/architecture.md#view-model.

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    screen::{Cells, NonZeroCells, ScreenLines},
    tab_stops::{MeasuredTextRef, TabStops},
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
            self.line += ScreenLines::ONE;
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
pub fn wrapped_line_count(payload: MeasuredTextRef<'_>, width: NonZeroCells) -> ScreenLines {
    let text = payload.text();
    if text.bytes().all(|byte| matches!(byte, b'\t' | b' '..=b'~')) {
        // Every cluster is one cell, so every line but the last is full.
        return payload.cells().lines_filled_at(width);
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
        screen::{NonZeroCells, ScreenLines},
        tab_stops::MeasuredTextRef,
    };

    fn line_count(text: &str, cells: usize) -> ScreenLines {
        wrapped_line_count(MeasuredTextRef::new(text), NonZeroCells::of(cells))
    }

    fn placed(line: usize) -> Option<ScreenLines> {
        Some(ScreenLines::new(line))
    }

    #[test]
    fn breaks_before_a_cluster_that_does_not_fit_the_rest_of_its_line() {
        let mut breaks = LineBreaks::new(NonZeroCells::of(4));

        let lines = ["a", "a", "a", "界", "界", "界"].map(|cluster| breaks.place(cluster));

        assert_eq!(lines, [0, 0, 0, 1, 1, 2].map(placed));
        assert_eq!(breaks.line_count(), ScreenLines::new(3));
    }

    #[test]
    fn keeps_a_cluster_wider_than_the_line_on_a_line_of_its_own() {
        let mut breaks = LineBreaks::new(NonZeroCells::of(1));

        assert_eq!(breaks.place("a"), placed(0));
        assert_eq!(breaks.place("界"), placed(1));
        assert_eq!(breaks.place("a"), placed(2));
    }

    #[test]
    fn gives_clusters_without_cells_no_place() {
        let mut breaks = LineBreaks::new(NonZeroCells::of(1));

        assert_eq!(breaks.place("a"), placed(0));
        assert_eq!(breaks.place("\u{7}"), None);
        assert_eq!(breaks.line_count(), ScreenLines::new(1));
    }

    #[test]
    fn counts_wrapped_lines_of_expanded_payloads() {
        assert_eq!(line_count("", 4), ScreenLines::new(1));
        assert_eq!(line_count("abcd", 4), ScreenLines::new(1));
        assert_eq!(line_count("abcde", 4), ScreenLines::new(2));
        assert_eq!(
            line_count("a\tbc", 4),
            ScreenLines::new(1),
            "the tab expands to one cell"
        );
        assert_eq!(
            line_count("ab\tc", 4),
            ScreenLines::new(2),
            "the tab expands to two cells"
        );
        assert_eq!(
            line_count("abc界", 4),
            ScreenLines::new(2),
            "the wide cluster moves down"
        );
        assert_eq!(
            line_count("e\u{301}e\u{301}e\u{301}e\u{301}", 4),
            ScreenLines::new(1)
        );
    }

    #[test]
    fn counts_single_cell_text_as_the_cluster_walk_does() {
        let text = "\tlet x = 1;\t// ascii";
        let mut breaks = LineBreaks::new(NonZeroCells::of(3));
        for _ in 0..MeasuredTextRef::new(text).cells().get() {
            breaks.place("a");
        }

        assert_eq!(line_count(text, 3), breaks.line_count());
        assert_eq!(line_count(text, 300), ScreenLines::new(1));
    }
}
