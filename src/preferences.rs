#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewPreferences {
    pub layout: DiffLayout,
    pub granularity: DiffGranularity,
    pub wrapping: Wrapping,
    pub context_lines: usize,
    pub file_list: FileList,
}

impl ViewPreferences {
    /// The preferences a session starts with.
    pub const INITIAL: Self = Self {
        layout: DiffLayout::Unified,
        granularity: DiffGranularity::Line,
        wrapping: Wrapping::Disabled,
        context_lines: 3,
        file_list: FileList::Shown,
    };

    pub const fn after(self, change: PreferenceChange) -> Self {
        match change {
            PreferenceChange::NextLayout => Self {
                layout: self.layout.next(),
                ..self
            },
            PreferenceChange::ToggleGranularity => Self {
                granularity: self.granularity.toggled(),
                ..self
            },
            PreferenceChange::ToggleWrapping => Self {
                wrapping: self.wrapping.toggled(),
                ..self
            },
            PreferenceChange::IncreaseContext => Self {
                context_lines: self.context_lines.saturating_add(1),
                ..self
            },
            PreferenceChange::DecreaseContext => Self {
                context_lines: self.context_lines.saturating_sub(1),
                ..self
            },
            PreferenceChange::ToggleFileList => Self {
                file_list: self.file_list.toggled(),
                ..self
            },
        }
    }
}

/// One session-local display choice a shortcut changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceChange {
    NextLayout,
    ToggleGranularity,
    ToggleWrapping,
    IncreaseContext,
    DecreaseContext,
    ToggleFileList,
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
