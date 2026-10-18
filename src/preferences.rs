#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewPreferences {
    pub layout: DiffLayout,
    pub granularity: DiffGranularity,
    pub wrapping: Wrapping,
    pub context_lines: usize,
    pub file_list: FileList,
}

impl ViewPreferences {
    /// The preferences a session starts with in `layout`.
    pub const fn initial(layout: DiffLayout) -> Self {
        Self {
            layout,
            granularity: DiffGranularity::Line,
            wrapping: Wrapping::Disabled,
            context_lines: 3,
            file_list: FileList::Shown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLayout {
    Unified,
    Vertical,
    Stacked,
}

impl DiffLayout {
    pub const fn next(self) -> Self {
        match self {
            Self::Unified => Self::Vertical,
            Self::Vertical => Self::Stacked,
            Self::Stacked => Self::Unified,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Unified => "unified",
            Self::Vertical => "vertical",
            Self::Stacked => "stacked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffGranularity {
    Line,
    Character,
}

impl DiffGranularity {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Character => "character",
        }
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Line => Self::Character,
            Self::Character => Self::Line,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrapping {
    Disabled,
    Enabled,
}

impl Wrapping {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "off",
            Self::Enabled => "on",
        }
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Disabled => Self::Enabled,
            Self::Enabled => Self::Disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileList {
    Shown,
    Hidden,
}

impl FileList {
    pub const fn toggled(self) -> Self {
        match self {
            Self::Shown => Self::Hidden,
            Self::Hidden => Self::Shown,
        }
    }
}
