use ratatui::style::Color;

use crate::syntax::SyntaxClass;

pub static DEFAULT_SYNTAX_THEME: SyntaxTheme = SyntaxTheme;

pub struct SyntaxTheme;

impl SyntaxTheme {
    pub fn color(&self, class: SyntaxClass, color_count: u16) -> Color {
        match color_count {
            u16::MAX => match class {
                SyntaxClass::Keyword | SyntaxClass::Constant => Color::Rgb(86, 156, 214),
                SyntaxClass::String => Color::Rgb(206, 145, 120),
                SyntaxClass::Comment => Color::Rgb(106, 153, 85),
                SyntaxClass::Number => Color::Rgb(181, 206, 168),
                SyntaxClass::Function => Color::Rgb(220, 220, 170),
                SyntaxClass::Type => Color::Rgb(78, 201, 176),
                SyntaxClass::Operator => Color::Rgb(212, 212, 212),
                SyntaxClass::Property => Color::Rgb(156, 220, 254),
                SyntaxClass::Variable => Color::Rgb(220, 220, 220),
            },
            256.. => match class {
                SyntaxClass::Keyword | SyntaxClass::Constant => Color::Indexed(75),
                SyntaxClass::String => Color::Indexed(180),
                SyntaxClass::Comment => Color::Indexed(107),
                SyntaxClass::Number => Color::Indexed(151),
                SyntaxClass::Function => Color::Indexed(187),
                SyntaxClass::Type => Color::Indexed(80),
                SyntaxClass::Operator => Color::Indexed(252),
                SyntaxClass::Property => Color::Indexed(153),
                SyntaxClass::Variable => Color::Indexed(253),
            },
            _ => match class {
                SyntaxClass::Keyword | SyntaxClass::Constant => Color::Blue,
                SyntaxClass::String => Color::Yellow,
                SyntaxClass::Comment | SyntaxClass::Number => Color::Green,
                SyntaxClass::Function | SyntaxClass::Type => Color::Cyan,
                SyntaxClass::Operator | SyntaxClass::Variable => Color::White,
                SyntaxClass::Property => Color::Magenta,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_SYNTAX_THEME;
    use crate::syntax::SyntaxClass;
    use ratatui::style::Color;

    #[test]
    fn maps_semantic_classes_for_each_terminal_color_capability() {
        assert_eq!(
            DEFAULT_SYNTAX_THEME.color(SyntaxClass::Keyword, u16::MAX),
            Color::Rgb(86, 156, 214)
        );
        assert_eq!(
            DEFAULT_SYNTAX_THEME.color(SyntaxClass::String, 256),
            Color::Indexed(180)
        );
        assert_eq!(
            DEFAULT_SYNTAX_THEME.color(SyntaxClass::Type, 8),
            Color::Cyan
        );
        assert_ne!(
            DEFAULT_SYNTAX_THEME.color(SyntaxClass::Keyword, 8),
            DEFAULT_SYNTAX_THEME.color(SyntaxClass::String, 8)
        );
    }
}
