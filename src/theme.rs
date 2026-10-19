//! The built-in palette for each terminal color depth. Rules:
//! docs/spec/architecture.md#view-model,
//! docs/spec/architecture.md#palette-and-character-detail, and
//! docs/spec/architecture.md#failure-and-safety-posture.

use ratatui::style::{Color, Modifier};

use crate::{file_identity::FileState, file_tree::StateMarker, side::BySide, syntax::SyntaxClass};

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
    pub fn of_color_count(color_count: u16) -> Self {
        match color_count {
            u16::MAX => Self::TrueColor,
            256.. => Self::Indexed,
            ..=255 => Self::Basic,
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
    /// The built-in theme for `depth`.
    pub const fn of_depth(depth: ColorDepth) -> &'static Self {
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
    /// The color text of `class` is drawn in.
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
    /// The colors of rows that change each side: deletions, then additions.
    pub change: BySide<ChangeColors>,
    /// The text of unchanged records.
    pub context_body: Color,
    /// The text of hunk headers.
    pub hunk_header: Color,
}

/// The colors of rows that change one side.
pub struct ChangeColors {
    /// Behind the whole row.
    pub background: Color,
    /// The row's text without syntax or character detail.
    pub body: Color,
    /// Text between the changed spans of a row with character detail.
    pub unchanged: Color,
    /// The gutter marker and the changed spans of a row with character detail.
    pub marker: Color,
    /// How the row's text is drawn besides its colors.
    pub body_modifier: Modifier,
}

/// Colors for the file list, the help hint, separators, and continuity markers.
pub struct ChromePalette {
    /// A directory heading in the file list.
    pub directory_heading: Color,
    /// A file name in the file list, and the marker of a file in an unknown state.
    pub file_name: Color,
    /// The state marker of a modified file.
    pub modified_marker: Color,
    /// The state marker of an added file.
    pub added_marker: Color,
    /// The state marker of a deleted file.
    pub deleted_marker: Color,
    /// The state marker of a renamed file.
    pub renamed_marker: Color,
    /// The text of the selected file's row.
    pub selected_file: Color,
    /// Behind the selected file's row.
    pub selected_file_background: Color,
    /// The outer separator beside the file list.
    pub separator: Color,
    /// The inner separator between side-by-side panes and the continuity markers.
    pub dim_separator: Color,
    /// The help hint.
    pub hint: Color,
}

impl ChromePalette {
    /// The color of a file list row's state `marker`.
    pub fn marker(&self, marker: StateMarker) -> Color {
        match marker {
            StateMarker::State(FileState::Modified) => self.modified_marker,
            StateMarker::State(FileState::Added) => self.added_marker,
            StateMarker::State(FileState::Deleted) => self.deleted_marker,
            StateMarker::State(FileState::Renamed) => self.renamed_marker,
            StateMarker::Unknown => self.file_name,
        }
    }
}

/// The theme for terminals that show 24-bit color.
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
        change: BySide {
            old: ChangeColors {
                background: Color::Rgb(50, 30, 30),
                body: Color::Rgb(150, 150, 150),
                unchanged: Color::Rgb(120, 120, 120),
                marker: Color::Rgb(206, 74, 74),
                body_modifier: Modifier::DIM,
            },
            new: ChangeColors {
                background: Color::Rgb(35, 73, 43),
                body: Color::Rgb(220, 235, 220),
                unchanged: Color::Rgb(170, 185, 170),
                marker: Color::Rgb(112, 195, 115),
                body_modifier: Modifier::empty(),
            },
        },
        context_body: Color::Rgb(180, 180, 180),
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

/// The theme for terminals that show the 256-color palette.
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
        change: BySide {
            old: ChangeColors {
                background: Color::Indexed(52),
                body: Color::Indexed(245),
                unchanged: Color::Indexed(242),
                marker: Color::Indexed(167),
                body_modifier: Modifier::DIM,
            },
            new: ChangeColors {
                background: Color::Indexed(22),
                body: Color::Indexed(255),
                unchanged: Color::Indexed(250),
                marker: Color::Indexed(71),
                body_modifier: Modifier::empty(),
            },
        },
        context_body: Color::Indexed(250),
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

/// The theme for terminals that show only the 16 basic colors.
static BASIC: Theme = Theme {
    syntax: SyntaxPalette {
        keyword: Color::Blue,
        string: Color::Yellow,
        comment: Color::DarkGray,
        number: Color::LightMagenta,
        function: Color::Cyan,
        type_name: Color::Cyan,
        operator: Color::White,
        property: Color::Magenta,
        variable: Color::White,
    },
    rows: RowPalette {
        change: BySide {
            old: ChangeColors {
                background: Color::Red,
                body: Color::DarkGray,
                unchanged: Color::Black,
                marker: Color::LightRed,
                body_modifier: Modifier::DIM,
            },
            new: ChangeColors {
                background: Color::Green,
                body: Color::White,
                unchanged: Color::Gray,
                marker: Color::LightGreen,
                body_modifier: Modifier::empty(),
            },
        },
        context_body: Color::Gray,
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

    use super::{ChangeColors, ChromePalette, ColorDepth, RowPalette, SyntaxPalette, Theme};

    /// How bright a gray text color is, comparable within one color depth.
    fn gray_level(color: Color) -> u16 {
        match color {
            Color::Rgb(red, green, blue) => u16::from(red) + u16::from(green) + u16::from(blue),
            Color::Indexed(index @ 232..=255) => u16::from(index),
            Color::Black => 0,
            Color::DarkGray => 1,
            Color::Gray => 2,
            Color::White => 3,
            _ => panic!("{color:?} is not a gray text color"),
        }
    }

    /// Every depth a session can draw with: `of_color_count` is the only constructor a
    /// session uses, so its image over every color count is every reachable depth.
    fn every_depth() -> Vec<ColorDepth> {
        let mut depths = (0..=u16::MAX)
            .map(ColorDepth::of_color_count)
            .collect::<Vec<_>>();
        depths.dedup();

        depths
    }

    /// Every color of `syntax`; destructured so a new class color cannot be left out.
    fn syntax_colors(syntax: &SyntaxPalette) -> [Color; 9] {
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

        [
            *keyword, *string, *comment, *number, *function, *type_name, *operator, *property,
            *variable,
        ]
    }

    /// Every color of `theme`; destructured so a new color cannot be left out.
    fn colors(theme: &Theme) -> Vec<Color> {
        let Theme {
            syntax,
            rows,
            chrome,
        } = theme;
        let RowPalette {
            change,
            context_body,
            hunk_header,
        } = rows;
        let change_colors = |colors: &ChangeColors| {
            let ChangeColors {
                background,
                body,
                unchanged,
                marker,
                body_modifier: _,
            } = colors;
            [*background, *body, *unchanged, *marker]
        };
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

        let syntax_colors = syntax_colors(syntax);
        let row_colors = [*context_body, *hunk_header];
        let chrome_colors = [
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
        ];

        [syntax_colors.as_slice(), &row_colors, &chrome_colors]
            .into_iter()
            .flatten()
            .copied()
            .chain(change.iter().flat_map(change_colors))
            .collect()
    }

    #[test]
    fn reads_crossterm_color_counts_as_depths() {
        assert_eq!(ColorDepth::of_color_count(u16::MAX), ColorDepth::TrueColor);
        assert_eq!(ColorDepth::of_color_count(256), ColorDepth::Indexed);
        assert_eq!(ColorDepth::of_color_count(255), ColorDepth::Basic);
        assert_eq!(ColorDepth::of_color_count(8), ColorDepth::Basic);
        assert_eq!(ColorDepth::of_color_count(0), ColorDepth::Basic);
    }

    /// docs/spec/architecture.md#failure-and-safety-posture: the palette falls back by
    /// capability, so no depth draws a color its terminal cannot show.
    #[test]
    fn draws_each_depth_only_with_colors_that_depth_shows() {
        let shows = |depth, color| match (depth, color) {
            (ColorDepth::TrueColor, Color::Rgb(..)) => true,
            (ColorDepth::Indexed, Color::Indexed(_)) => true,
            (ColorDepth::Basic, Color::Rgb(..) | Color::Indexed(_)) => false,
            (ColorDepth::Basic, _) => true,
            (ColorDepth::TrueColor | ColorDepth::Indexed, _) => false,
        };

        for depth in every_depth() {
            for color in colors(Theme::of_depth(depth)) {
                assert!(shows(depth, color), "{depth:?} draws {color:?}");
            }
        }
    }

    /// docs/spec/architecture.md#palette-and-character-detail: unchanged text on rows
    /// with detail spans is one step dimmer than the row's line-mode text.
    #[test]
    fn draws_unchanged_detail_text_dimmer_than_line_mode_text() {
        for depth in every_depth() {
            let change = &Theme::of_depth(depth).rows.change;
            for (side, colors) in change.as_ref().into_sides() {
                assert!(
                    gray_level(colors.unchanged) < gray_level(colors.body),
                    "{depth:?} {side:?}: {:?} is not dimmer than {:?}",
                    colors.unchanged,
                    colors.body
                );
            }
        }
    }

    /// docs/spec/architecture.md#palette-and-character-detail: markers, changed spans and
    /// row text are drawn over the change background, so each must differ from it.
    #[test]
    fn draws_every_change_row_color_apart_from_its_background() {
        for depth in every_depth() {
            let change = &Theme::of_depth(depth).rows.change;
            for (side, colors) in change.as_ref().into_sides() {
                let ChangeColors {
                    background,
                    body,
                    unchanged,
                    marker,
                    body_modifier: _,
                } = colors;
                for (role, color) in [("body", body), ("unchanged", unchanged), ("marker", marker)]
                {
                    assert_ne!(
                        color, background,
                        "{depth:?} {side:?}: {role} is drawn in its background color"
                    );
                }
            }
        }
    }

    /// docs/spec/architecture.md#palette-and-character-detail: rows without detail spans
    /// keep syntax styling, drawn over the change background, so each syntax color must
    /// differ from both change backgrounds.
    #[test]
    fn draws_every_syntax_color_apart_from_each_change_background() {
        for depth in every_depth() {
            let theme = Theme::of_depth(depth);
            for (side, colors) in theme.rows.change.as_ref().into_sides() {
                for color in syntax_colors(&theme.syntax) {
                    assert_ne!(
                        color, colors.background,
                        "{depth:?} {side:?}: a syntax color is drawn in its background color"
                    );
                }
            }
        }
    }
}
