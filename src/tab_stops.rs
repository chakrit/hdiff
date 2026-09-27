//! Tab expansion for interactive rows; see `docs/spec/architecture.md` (Lossless and safe
//! rendering) for the 2-column tab-stop rule.

use ratatui::text::Span;

const TAB_WIDTH: usize = 2;

/// Expands tabs to the next multiple of `TAB_WIDTH` cells from the start of the payload,
/// across consecutive segments of one row.
#[derive(Default)]
pub struct TabStops {
    column: usize,
}

impl TabStops {
    pub fn expand(&mut self, text: &str) -> String {
        let mut expanded = String::with_capacity(text.len());

        for (index, chunk) in text.split('\t').enumerate() {
            if index > 0 {
                expanded.extend(std::iter::repeat_n(' ', self.advance_to_tab_stop()));
            }
            expanded.push_str(chunk);
            self.column += Span::raw(chunk).width();
        }
        expanded
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
        tab_stops.column += Span::raw(chunk).width();
    }
    tab_stops.column
}
