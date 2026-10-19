use crate::compact::Count;

/// The session-local display choices: layout, detail, wrapping, context and file list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewPreferences {
    pub layout: DiffLayout,
    pub granularity: DiffGranularity,
    pub wrapping: Wrapping,
    pub context: ContextRecords,
    pub file_list: FileList,
}

impl ViewPreferences {
    /// The preferences a session starts with.
    pub const INITIAL: Self = Self {
        layout: DiffLayout::Unified,
        granularity: DiffGranularity::Line,
        wrapping: Wrapping::Disabled,
        context: ContextRecords::new(3),
        file_list: FileList::Shown,
    };

    /// The preferences `change` produces from these, with every other choice kept.
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
                context: self.context.increased(),
                ..self
            },
            PreferenceChange::DecreaseContext => Self {
                context: self.context.decreased(),
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

/// How a diff's two sides are laid out: one unified pane, or two panes side by side
/// (vertical split) or one above the other (stacked).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLayout {
    Unified,
    Vertical,
    Stacked,
}

impl DiffLayout {
    /// The layout the layout shortcut switches to, cycling back to unified.
    pub const fn next(self) -> Self {
        match self {
            Self::Unified => Self::Vertical,
            Self::Vertical => Self::Stacked,
            Self::Stacked => Self::Unified,
        }
    }

    /// The layout's name in benchmark movement records.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unified => "unified",
            Self::Vertical => "vertical",
            Self::Stacked => "stacked",
        }
    }
}

/// Whether changed lines show whole-line changes or character detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffGranularity {
    Line,
    Character,
}

impl DiffGranularity {
    /// The granularity's name in benchmark movement records.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Character => "character",
        }
    }

    /// The other granularity.
    pub const fn toggled(self) -> Self {
        match self {
            Self::Line => Self::Character,
            Self::Character => Self::Line,
        }
    }
}

/// Whether long rows wrap within their pane or scroll horizontally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrapping {
    Disabled,
    Enabled,
}

impl Wrapping {
    /// The wrapping state's name in benchmark movement records.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "off",
            Self::Enabled => "on",
        }
    }

    /// The other wrapping state.
    pub const fn toggled(self) -> Self {
        match self {
            Self::Disabled => Self::Enabled,
            Self::Enabled => Self::Disabled,
        }
    }
}

/// How many context records show on either side of a change within its hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextRecords(usize);

impl ContextRecords {
    /// A count of `records` context records.
    pub const fn new(records: usize) -> Self {
        Self(records)
    }

    /// One more context record.
    pub const fn increased(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// One fewer context record, stopping at none.
    pub const fn decreased(self) -> Self {
        Self(self.0.saturating_sub(1))
    }

    /// Whether a context record `distance` records from the nearest change shows.
    pub fn admits(self, distance: Count) -> bool {
        distance.get() <= self.0
    }
}

/// Whether the file list column shows beside the diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileList {
    Shown,
    Hidden,
}

impl FileList {
    /// The other file-list state.
    pub const fn toggled(self) -> Self {
        match self {
            Self::Shown => Self::Hidden,
            Self::Hidden => Self::Shown,
        }
    }
}
