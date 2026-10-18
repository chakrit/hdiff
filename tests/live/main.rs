//! Live interaction tests. Contract: docs/spec/architecture.md#interactive-loop; each
//! test drives a `Tui` one event at a time on a 120-by-36 `TestBackend` and asserts on
//! the buffer after each step.

/// Runs each scenario once per layout and detail mode, as a test module named after it.
macro_rules! matrix {
    ($($scenario:ident),* $(,)?) => {$(
        mod $scenario {
            use super::{DiffGranularity::*, DiffLayout::*, Mode};

            #[test]
            fn unified_line() {
                super::$scenario(Mode { layout: Unified, detail: Line });
            }

            #[test]
            fn unified_character() {
                super::$scenario(Mode { layout: Unified, detail: Character });
            }

            #[test]
            fn vertical_line() {
                super::$scenario(Mode { layout: Vertical, detail: Line });
            }

            #[test]
            fn vertical_character() {
                super::$scenario(Mode { layout: Vertical, detail: Character });
            }

            #[test]
            fn stacked_line() {
                super::$scenario(Mode { layout: Stacked, detail: Line });
            }

            #[test]
            fn stacked_character() {
                super::$scenario(Mode { layout: Stacked, detail: Character });
            }
        }
    )*};
}

mod file_list;
mod fixtures;
mod harness;
mod navigation;
mod rendering;
mod wrapping;
