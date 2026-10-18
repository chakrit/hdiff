//! Positions and widths retained as `u32` to keep per-line records small, and used as
//! `usize`. Rules: `docs/spec/architecture.md` (Viewport coordinates).

/// `value` as retained.
///
/// Panics when `value` does not fit in `u32`.
pub fn narrow(value: usize) -> u32 {
    u32::try_from(value).expect("retained positions fit in u32")
}

/// A retained `value` as used.
pub fn widen(value: u32) -> usize {
    usize::try_from(value).expect("u32 values fit in usize")
}
