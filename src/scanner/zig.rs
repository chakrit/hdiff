//! Zig's lexical pieces. A `\\` multiline string line reads as one string to the line's
//! end.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static ZIG: Spec = Spec {
    line_markers: &[
        Marker::anywhere("//", SyntaxClass::Comment),
        Marker::anywhere("\\\\", SyntaxClass::String),
    ],
    delimited: &[
        Pair::escaped("\"", SyntaxClass::String),
        Pair::character("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b"@"),
    words: &[
        (
            &[
                "addrspace",
                "align",
                "allowzero",
                "and",
                "anyframe",
                "anytype",
                "asm",
                "async",
                "await",
                "break",
                "callconv",
                "catch",
                "comptime",
                "const",
                "continue",
                "defer",
                "else",
                "enum",
                "errdefer",
                "error",
                "export",
                "extern",
                "fn",
                "for",
                "if",
                "inline",
                "linksection",
                "noalias",
                "noinline",
                "nosuspend",
                "opaque",
                "or",
                "orelse",
                "packed",
                "pub",
                "resume",
                "return",
                "struct",
                "suspend",
                "switch",
                "test",
                "threadlocal",
                "try",
                "union",
                "unreachable",
                "usingnamespace",
                "var",
                "volatile",
                "while",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "anyerror",
                "anyopaque",
                "bool",
                "c_char",
                "c_int",
                "c_long",
                "c_longdouble",
                "c_longlong",
                "c_short",
                "c_uint",
                "c_ulong",
                "c_ulonglong",
                "c_ushort",
                "comptime_float",
                "comptime_int",
                "f128",
                "f16",
                "f32",
                "f64",
                "f80",
                "i128",
                "i16",
                "i32",
                "i64",
                "i8",
                "isize",
                "noreturn",
                "type",
                "u128",
                "u16",
                "u32",
                "u64",
                "u8",
                "usize",
                "void",
            ],
            SyntaxClass::Type,
        ),
        (
            &["false", "null", "true", "undefined"],
            SyntaxClass::Constant,
        ),
    ],
    before: &[("(", SyntaxClass::Function)],
    after: &[("fn", SyntaxClass::Function), (".", SyntaxClass::Property)],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: Some(SyntaxClass::Type),
    other_words: Some(SyntaxClass::Variable),
    word_suffixes: b"",
    word_case: WordCase::Sensitive,
};

#[cfg(test)]
mod tests {
    use super::ZIG;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(
                &ZIG,
                "pub fn init(allocator: std.mem.Allocator) !Self { return .{ .items = try allocator.alloc(u8, 16) }; }"
            ),
            [
                ("pub", SyntaxClass::Keyword),
                ("fn", SyntaxClass::Keyword),
                ("init", SyntaxClass::Function),
                ("allocator", SyntaxClass::Variable),
                ("std", SyntaxClass::Variable),
                ("mem", SyntaxClass::Property),
                ("Allocator", SyntaxClass::Property),
                ("!", SyntaxClass::Operator),
                ("Self", SyntaxClass::Type),
                ("return", SyntaxClass::Keyword),
                ("items", SyntaxClass::Property),
                ("=", SyntaxClass::Operator),
                ("try", SyntaxClass::Keyword),
                ("allocator", SyntaxClass::Variable),
                ("alloc", SyntaxClass::Function),
                ("u8", SyntaxClass::Type),
                ("16", SyntaxClass::Number),
            ]
        );
    }
}
