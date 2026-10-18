//! Terminal cells that text covers when the row writer draws it. Rules:
//! `docs/spec/architecture.md` (Lossless and safe rendering).

use ratatui::buffer::CellWidth;
use unicode_segmentation::UnicodeSegmentation;

/// Terminal cells the row writer covers drawing `text`.
pub fn text_cells(text: &str) -> usize {
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
