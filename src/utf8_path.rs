//! A filesystem path known to be valid UTF-8, so it has a text spelling as well as a
//! path.

use std::path::Path;

/// A path whose bytes were checked to be UTF-8 when it was made.
#[derive(Debug)]
pub struct Utf8Path(Box<str>);

impl Utf8Path {
    /// `path` when it is valid UTF-8; otherwise none.
    pub fn of(path: &Path) -> Option<Self> {
        path.to_str().map(|text| Self(text.into()))
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&*self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
