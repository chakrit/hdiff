//! Live interaction tests. Contract: docs/spec/architecture.md#interactive-loop; each
//! test drives a `Tui` one event at a time on a 120-by-36 `TestBackend` and asserts on
//! the buffer after each step.

/// Runs each scenario once per layout and detail mode, as a test module named after it.
macro_rules! matrix {
    ($($scenario:ident),* $(,)?) => {$(
        mod $scenario {
            use super::{DiffLayout::*, Mode};

            #[test]
            fn unified_line() {
                super::$scenario(Mode::line(Unified));
            }

            #[test]
            fn unified_character() {
                super::$scenario(Mode::character(Unified));
            }

            #[test]
            fn vertical_line() {
                super::$scenario(Mode::line(Vertical));
            }

            #[test]
            fn vertical_character() {
                super::$scenario(Mode::character(Vertical));
            }

            #[test]
            fn stacked_line() {
                super::$scenario(Mode::line(Stacked));
            }

            #[test]
            fn stacked_character() {
                super::$scenario(Mode::character(Stacked));
            }
        }
    )*};
}

mod file_list;
mod fixtures;
mod harness;
mod navigation;
mod panes;
mod rendering;
mod styles;
mod wrapping;
