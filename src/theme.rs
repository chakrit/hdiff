//! The built-in palette for each terminal color depth. Rules: `docs/spec/architecture.md`
//! (View model, Failure and safety posture).

use ratatui::style::Color;

use crate::{file_identity::FileState, file_tree::Marker, syntax::SyntaxClass};

/// The colors a terminal can show, decided once when the session starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    Basic,
    Indexed,
    TrueColor,
}

impl ColorDepth {
    /// The depth for Crossterm's available color count, which reports truecolor as
    /// `u16::MAX`.
    pub fn from_color_count(color_count: u16) -> Self {
        match color_count {
            u16::MAX => Self::TrueColor,
            256.. => Self::Indexed,
            _ => Self::Basic,
        }
    }
}

/// Every color the viewer draws with, for one color depth.
pub struct Theme {
    pub syntax: SyntaxPalette,
    pub rows: RowPalette,
    pub chrome: ChromePalette,
}

impl Theme {
    pub const fn for_depth(depth: ColorDepth) -> &'static Self {
        match depth {
            ColorDepth::TrueColor => &TRUECOLOR,
            ColorDepth::Indexed => &INDEXED,
            ColorDepth::Basic => &BASIC,
        }
    }
}

/// Colors for semantic syntax classes; constants share the keyword color.
pub struct SyntaxPalette {
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub number: Color,
    pub function: Color,
    pub type_name: Color,
    pub operator: Color,
    pub property: Color,
    pub variable: Color,
}

impl SyntaxPalette {
    pub fn color(&self, class: SyntaxClass) -> Color {
        match class {
            SyntaxClass::Keyword | SyntaxClass::Constant => self.keyword,
            SyntaxClass::String => self.string,
            SyntaxClass::Comment => self.comment,
            SyntaxClass::Number => self.number,
            SyntaxClass::Function => self.function,
            SyntaxClass::Type => self.type_name,
            SyntaxClass::Operator => self.operator,
            SyntaxClass::Property => self.property,
            SyntaxClass::Variable => self.variable,
        }
    }
}

/// The `low` palette for diff rows.
pub struct RowPalette {
    pub addition_background: Color,
    pub addition_body: Color,
    pub unchanged_addition: Color,
    pub deletion_background: Color,
    pub deletion_body: Color,
    pub unchanged_deletion: Color,
    pub context_body: Color,
    pub deletion_marker: Color,
    pub addition_marker: Color,
    pub hunk_header: Color,
}

/// Colors for the file list, the help hint, separators, and continuity markers.
pub struct ChromePalette {
    pub directory_heading: Color,
    pub file_name: Color,
    pub modified_marker: Color,
    pub added_marker: Color,
    pub deleted_marker: Color,
    pub renamed_marker: Color,
    pub selected_file: Color,
    pub selected_file_background: Color,
    pub separator: Color,
    /// The inner separator between side-by-side panes and the continuity markers.
    pub dim_separator: Color,
    pub hint: Color,
}

impl ChromePalette {
    pub fn marker(&self, marker: Marker) -> Color {
        match marker {
            Marker::State(FileState::Modified) => self.modified_marker,
            Marker::State(FileState::Added) => self.added_marker,
            Marker::State(FileState::Deleted) => self.deleted_marker,
            Marker::State(FileState::Renamed) => self.renamed_marker,
            Marker::Unknown => self.file_name,
        }
    }
}

static TRUECOLOR: Theme = Theme {
    syntax: SyntaxPalette {
        keyword: Color::Rgb(86, 156, 214),
        string: Color::Rgb(206, 145, 120),
        comment: Color::Rgb(106, 153, 85),
        number: Color::Rgb(181, 206, 168),
        function: Color::Rgb(220, 220, 170),
        type_name: Color::Rgb(78, 201, 176),
        operator: Color::Rgb(212, 212, 212),
        property: Color::Rgb(156, 220, 254),
        variable: Color::Rgb(220, 220, 220),
    },
    rows: RowPalette {
        addition_background: Color::Rgb(35, 73, 43),
        addition_body: Color::Rgb(220, 235, 220),
        unchanged_addition: Color::Rgb(170, 185, 170),
        deletion_background: Color::Rgb(50, 30, 30),
        deletion_body: Color::Rgb(150, 150, 150),
        unchanged_deletion: Color::Rgb(120, 120, 120),
        context_body: Color::Rgb(180, 180, 180),
        deletion_marker: Color::Rgb(206, 74, 74),
        addition_marker: Color::Rgb(112, 195, 115),
        hunk_header: Color::Rgb(100, 155, 175),
    },
    chrome: ChromePalette {
        directory_heading: Color::Rgb(125, 125, 125),
        file_name: Color::Rgb(160, 160, 160),
        modified_marker: Color::Rgb(190, 170, 120),
        added_marker: Color::Rgb(130, 185, 130),
        deleted_marker: Color::Rgb(200, 130, 130),
        renamed_marker: Color::Rgb(130, 170, 205),
        selected_file: Color::Rgb(220, 220, 220),
        selected_file_background: Color::Rgb(65, 65, 65),
        separator: Color::Rgb(125, 125, 125),
        dim_separator: Color::Rgb(95, 95, 95),
        hint: Color::Rgb(110, 110, 110),
    },
};

static INDEXED: Theme = Theme {
    syntax: SyntaxPalette {
        keyword: Color::Indexed(75),
        string: Color::Indexed(180),
        comment: Color::Indexed(107),
        number: Color::Indexed(151),
        function: Color::Indexed(187),
        type_name: Color::Indexed(80),
        operator: Color::Indexed(252),
        property: Color::Indexed(153),
        variable: Color::Indexed(253),
    },
    rows: RowPalette {
        addition_background: Color::Indexed(22),
        addition_body: Color::Indexed(255),
        unchanged_addition: Color::Indexed(250),
        deletion_background: Color::Indexed(52),
        deletion_body: Color::Indexed(245),
        unchanged_deletion: Color::Indexed(242),
        context_body: Color::Indexed(250),
        deletion_marker: Color::Indexed(167),
        addition_marker: Color::Indexed(71),
        hunk_header: Color::Indexed(109),
    },
    chrome: ChromePalette {
        directory_heading: Color::Indexed(243),
        file_name: Color::Indexed(246),
        modified_marker: Color::Indexed(144),
        added_marker: Color::Indexed(108),
        deleted_marker: Color::Indexed(138),
        renamed_marker: Color::Indexed(110),
        selected_file: Color::Indexed(255),
        selected_file_background: Color::Indexed(238),
        separator: Color::Indexed(243),
        dim_separator: Color::Indexed(238),
        hint: Color::Indexed(240),
    },
};

static BASIC: Theme = Theme {
    syntax: SyntaxPalette {
        keyword: Color::Blue,
        string: Color::Yellow,
        comment: Color::Green,
        number: Color::Green,
        function: Color::Cyan,
        type_name: Color::Cyan,
        operator: Color::White,
        property: Color::Magenta,
        variable: Color::White,
    },
    rows: RowPalette {
        addition_background: Color::Green,
        addition_body: Color::White,
        unchanged_addition: Color::Gray,
        deletion_background: Color::Red,
        deletion_body: Color::DarkGray,
        unchanged_deletion: Color::DarkGray,
        context_body: Color::Gray,
        deletion_marker: Color::Red,
        addition_marker: Color::Green,
        hunk_header: Color::Cyan,
    },
    chrome: ChromePalette {
        directory_heading: Color::DarkGray,
        file_name: Color::Gray,
        modified_marker: Color::Yellow,
        added_marker: Color::Green,
        deleted_marker: Color::Red,
        renamed_marker: Color::Blue,
        selected_file: Color::White,
        selected_file_background: Color::DarkGray,
        separator: Color::DarkGray,
        dim_separator: Color::DarkGray,
        hint: Color::DarkGray,
    },
};

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::{ChromePalette, ColorDepth, RowPalette, SyntaxPalette, Theme};

    #[test]
    fn reads_crossterm_color_counts_as_depths() {
        assert_eq!(
            ColorDepth::from_color_count(u16::MAX),
            ColorDepth::TrueColor
        );
        assert_eq!(ColorDepth::from_color_count(256), ColorDepth::Indexed);
        assert_eq!(ColorDepth::from_color_count(255), ColorDepth::Basic);
        assert_eq!(ColorDepth::from_color_count(8), ColorDepth::Basic);
        assert_eq!(ColorDepth::from_color_count(0), ColorDepth::Basic);
    }

    /// architecture.md (Failure and safety posture): the palette falls back by capability,
    /// so no depth draws a color its terminal cannot show.
    #[test]
    fn draws_each_depth_only_with_colors_that_depth_shows() {
        let shows = |depth, color| match (depth, color) {
            (ColorDepth::TrueColor, Color::Rgb(..)) => true,
            (ColorDepth::Indexed, Color::Indexed(_)) => true,
            (ColorDepth::Basic, Color::Rgb(..) | Color::Indexed(_)) => false,
            (ColorDepth::Basic, _) => true,
            (ColorDepth::TrueColor | ColorDepth::Indexed, _) => false,
        };

        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Indexed,
            ColorDepth::Basic,
        ] {
            for color in colors(Theme::for_depth(depth)) {
                assert!(shows(depth, color), "{depth:?} draws {color:?}");
            }
        }
    }

    /// Every color of `theme`; destructured so a new color cannot be left out.
    fn colors(theme: &Theme) -> Vec<Color> {
        let Theme {
            syntax,
            rows,
            chrome,
        } = theme;
        let SyntaxPalette {
            keyword,
            string,
            comment,
            number,
            function,
            type_name,
            operator,
            property,
            variable,
        } = syntax;
        let RowPalette {
            addition_background,
            addition_body,
            unchanged_addition,
            deletion_background,
            deletion_body,
            unchanged_deletion,
            context_body,
            deletion_marker,
            addition_marker,
            hunk_header,
        } = rows;
        let ChromePalette {
            directory_heading,
            file_name,
            modified_marker,
            added_marker,
            deleted_marker,
            renamed_marker,
            selected_file,
            selected_file_background,
            separator,
            dim_separator,
            hint,
        } = chrome;

        vec![
            *keyword,
            *string,
            *comment,
            *number,
            *function,
            *type_name,
            *operator,
            *property,
            *variable,
            *addition_background,
            *addition_body,
            *unchanged_addition,
            *deletion_background,
            *deletion_body,
            *unchanged_deletion,
            *context_body,
            *deletion_marker,
            *addition_marker,
            *hunk_header,
            *directory_heading,
            *file_name,
            *modified_marker,
            *added_marker,
            *deleted_marker,
            *renamed_marker,
            *selected_file,
            *selected_file_background,
            *separator,
            *dim_separator,
            *hint,
        ]
    }
}
