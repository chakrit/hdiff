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

    /// Lines `clusters` occupy when placed in order on lines `width` cells wide: one past
    /// the last line a cluster lands on, so an empty payload still takes one.
    pub fn count_lines<'a>(
        width: NonZeroCells,
        clusters: impl IntoIterator<Item = &'a str>,
    ) -> ScreenLines {
        clusters
            .into_iter()
            .scan(Self::new(width), |breaks, cluster| {
                Some(breaks.place(cluster))
            })
            .flatten()
            .last()
            .map_or(ScreenLines::ONE, |line| line + ScreenLines::ONE)
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

    LineBreaks::count_lines(width, expanded.graphemes(true))
}

#[cfg(test)]
mod tests {
    use std::iter;

    use super::{LineBreaks, wrapped_line_count};
    use crate::{
        screen::{NonZeroCells, ScreenLines},
        tab_stops::MeasuredTextRef,
    };

    fn wrapped_lines(text: &str, cells: usize) -> ScreenLines {
        wrapped_line_count(MeasuredTextRef::of_text(text), NonZeroCells::of(cells))
    }

    fn placed(line: usize) -> Option<ScreenLines> {
        Some(ScreenLines::new(line))
    }

    #[test]
    fn breaks_before_a_cluster_that_does_not_fit_the_rest_of_its_line() {
        let clusters = ["a", "a", "a", "界", "界", "界"];

        let lines: Vec<_> = clusters
            .into_iter()
            .scan(LineBreaks::new(NonZeroCells::of(4)), |breaks, cluster| {
                Some(breaks.place(cluster))
            })
            .collect();

        assert_eq!(lines, [0, 0, 0, 1, 1, 2].map(placed));
        assert_eq!(wrapped_lines(&clusters.concat(), 4), ScreenLines::new(3));
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
        assert_eq!(
            LineBreaks::count_lines(NonZeroCells::of(1), ["a", "\u{7}"]),
            ScreenLines::new(1)
        );
    }

    #[test]
    fn counts_wrapped_lines_of_expanded_payloads() {
        assert_eq!(wrapped_lines("", 4), ScreenLines::new(1));
        assert_eq!(wrapped_lines("abcd", 4), ScreenLines::new(1));
        assert_eq!(wrapped_lines("abcde", 4), ScreenLines::new(2));
        assert_eq!(
            wrapped_lines("a\tbc", 4),
            ScreenLines::new(1),
            "the tab expands to one cell"
        );
        assert_eq!(
            wrapped_lines("ab\tc", 4),
            ScreenLines::new(2),
            "the tab expands to two cells"
        );
        assert_eq!(
            wrapped_lines("abc界", 4),
            ScreenLines::new(2),
            "the wide cluster moves down"
        );
        assert_eq!(
            wrapped_lines("e\u{301}e\u{301}e\u{301}e\u{301}", 4),
            ScreenLines::new(1)
        );
    }

    #[test]
    fn counts_single_cell_text_as_the_cluster_walk_does() {
        let text = "\tlet x = 1;\t// ascii";
        let cells = MeasuredTextRef::of_text(text).cells().get();
        let walked = LineBreaks::count_lines(NonZeroCells::of(3), iter::repeat_n("a", cells));

        assert_eq!(wrapped_lines(text, 3), walked);
        assert_eq!(wrapped_lines(text, 300), ScreenLines::new(1));
    }
}
