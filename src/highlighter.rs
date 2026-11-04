//! The Tree-sitter second pass: a thread that re-colors each finished hunk with the grammar
//! compiled in for its file type, and sends the restyled lines to the main thread. Rules:
//! docs/spec/performance.md#preparation-boundary and
//! docs/spec/architecture.md#technology-direction.

use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender},
};

use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::{
    display_lines::{LineKind, RecordPart},
    languages::{Grammar, grammars::GRAMMARS},
    loading::Message,
    prepared::{PageIndex, PreparedPage},
    side::{BySide, Side},
    span::PayloadSpan,
    styling::LineSyntax,
    syntax::SyntaxClass,
};

/// A finished hunk's record lines, owned, with the page they sit on and the file type each
/// side selects.
pub struct Hunk {
    page: PageIndex,
    file_types: BySide<Option<&'static str>>,
    lines: Vec<HunkLine>,
}

/// One record payload of a hunk: its position among its page's lines, the side it
/// changes or none for context, and its text.
struct HunkLine {
    position: usize,
    changed: Option<Side>,
    payload: Box<str>,
}

/// The lines of one page the second pass re-colored, each by its position among the
/// page's lines.
pub struct Restyled {
    pub page: PageIndex,
    pub lines: Vec<(usize, LineSyntax)>,
}

impl Hunk {
    /// The hunks of `part`, the part of `page` whose first line sits at `first_position`
    /// among the page's lines; none when neither side of a file of `file_types` has a
    /// compiled grammar. Records before the part's first hunk header are left out.
    pub fn of_part(
        page: PageIndex,
        file_types: BySide<Option<&'static str>>,
        first_position: usize,
        part: &PreparedPage,
    ) -> Vec<Self> {
        if file_types
            .iter()
            .all(|file_type| grammar_of(*file_type).is_none())
        {
            return Vec::new();
        }

        let mut hunks: Vec<Self> = Vec::new();
        for (offset, (_, line)) in part.rows().lines().enumerate() {
            match line.kind() {
                LineKind::HunkHeader => hunks.push(Self {
                    page,
                    file_types,
                    lines: Vec::new(),
                }),
                LineKind::Record {
                    kind,
                    part: RecordPart::Payload,
                    ..
                } => {
                    if let Some(hunk) = hunks.last_mut() {
                        hunk.lines.push(HunkLine {
                            position: first_position + offset,
                            changed: kind.changed_side(),
                            payload: line.payload().into(),
                        });
                    }
                }
                LineKind::Record {
                    part: RecordPart::NoNewlineMarker,
                    ..
                }
                | LineKind::Text
                | LineKind::Metadata
                | LineKind::FileHeader
                | LineKind::Failure => {}
            }
        }

        hunks
    }
}

/// The grammar compiled in for `file_type`, if any.
fn grammar_of(file_type: Option<&str>) -> Option<&'static Grammar> {
    let file_type = file_type?;
    GRAMMARS
        .iter()
        .find(|grammar| grammar.file_types.contains(&file_type))
}

/// Re-colors each hunk `hunks` delivers and sends its lines over `sender`, until the
/// worker stops sending hunks or the main thread stops receiving.
pub fn run(hunks: Receiver<Hunk>, sender: Sender<Message>) {
    let mut highlighter = Highlighter::default();
    for hunk in hunks {
        let restyled = highlighter.restyle(&hunk);
        if restyled.lines.is_empty() {
            continue;
        }
        if sender.send(Message::Restyled(restyled)).is_err() {
            return;
        }
    }
}

/// Each grammar's parser and compiled query, made on first use; a grammar that cannot be
/// loaded is remembered as such and leaves the scanner's colors.
#[derive(Default)]
struct Highlighter {
    loaded: HashMap<&'static str, Option<Loaded>>,
}

/// A grammar ready to highlight: its parser, its query, and the syntax class each of the
/// query's captures folds to.
struct Loaded {
    parser: Parser,
    query: Query,
    classes: Box<[Option<SyntaxClass>]>,
    cursor: QueryCursor,
}

impl Highlighter {
    /// The new syntax of `hunk`'s lines: each changed line from its side's grammar, and
    /// each context line from the new side's when both sides select one file type. A
    /// side without a grammar keeps its colors.
    fn restyle(&mut self, hunk: &Hunk) -> Restyled {
        let shared = hunk.file_types.old == hunk.file_types.new;
        let mut lines = Vec::new();
        for side in [Side::Old, Side::New] {
            let Some(grammar) = grammar_of(*hunk.file_types.get(side)) else {
                continue;
            };
            let reads = |line: &&HunkLine| line.changed.is_none_or(|changed| changed == side);
            let read: Vec<&HunkLine> = hunk.lines.iter().filter(reads).collect();
            let Some(painted) = self.paint(grammar, &read) else {
                continue;
            };

            let styles = |line: &&HunkLine| match line.changed {
                Some(_) => true,
                None => shared && side == Side::New,
            };
            for (line, classes) in read.iter().zip(painted).filter(|(line, _)| styles(line)) {
                lines.push((
                    line.position,
                    LineSyntax::of_spans(&line.payload, runs(&line.payload, &classes)),
                ));
            }
        }

        Restyled {
            page: hunk.page,
            lines,
        }
    }

    /// The syntax class of each byte of each of `lines`, read in order as one text with
    /// `grammar`; none when the grammar cannot be loaded or cannot parse it.
    fn paint(
        &mut self,
        grammar: &'static Grammar,
        lines: &[&HunkLine],
    ) -> Option<Vec<Vec<Option<SyntaxClass>>>> {
        let loaded = self
            .loaded
            .entry(grammar.name)
            .or_insert_with(|| Loaded::of(grammar))
            .as_mut()?;
        let text = lines
            .iter()
            .map(|line| &*line.payload)
            .collect::<Vec<_>>()
            .join("\n");
        let tree = loaded.parser.parse(&text, None)?;

        let mut captured = Vec::new();
        let mut matches = loaded
            .cursor
            .matches(&loaded.query, tree.root_node(), text.as_bytes());
        while let Some(found) = matches.next() {
            for capture in found.captures() {
                if let Some(class) = loaded.classes[capture.index as usize] {
                    captured.push(Captured {
                        bytes: capture.node.byte_range(),
                        pattern: found.pattern_index,
                        class,
                    });
                }
            }
        }

        let painted = paint_bytes(text.len(), captured);
        let mut start = 0;
        let per_line = lines.iter().map(|line| {
            let end = start + line.payload.len();
            let classes = painted[start..end].to_vec();
            start = end + 1;
            classes
        });
        Some(per_line.collect())
    }
}

impl Loaded {
    /// `grammar` with its parser and query; none when its language or query is not one
    /// this `tree-sitter` can load.
    fn of(grammar: &Grammar) -> Option<Self> {
        let language = (grammar.language)();
        let mut parser = Parser::new();
        parser.set_language(&language).ok()?;
        let query = Query::new(&language, grammar.highlights).ok()?;
        let classes = query
            .capture_names()
            .iter()
            .map(|name| fold(name))
            .collect();

        Some(Self {
            parser,
            query,
            classes,
            cursor: QueryCursor::new(),
        })
    }
}

/// A capture's bytes, the pattern that made it, and the class it folds to.
struct Captured {
    bytes: std::ops::Range<usize>,
    pattern: usize,
    class: SyntaxClass,
}

/// The class of each of `length` bytes: the innermost capture covering it, and of
/// captures over the same bytes, the later pattern's.
fn paint_bytes(length: usize, mut captured: Vec<Captured>) -> Vec<Option<SyntaxClass>> {
    captured.sort_by_key(|capture| (std::cmp::Reverse(capture.bytes.len()), capture.pattern));

    let mut painted = vec![None; length];
    for capture in captured {
        painted[capture.bytes].fill(Some(capture.class));
    }

    painted
}

/// The spans of `payload` that `classes`, one per byte, color: each run of one class.
fn runs(payload: &str, classes: &[Option<SyntaxClass>]) -> Vec<(PayloadSpan, SyntaxClass)> {
    let mut spans = Vec::new();
    let mut start = 0;
    for end in 1..=classes.len() {
        if end < classes.len() && classes[end] == classes[start] {
            continue;
        }
        if let Some(class) = classes[start] {
            spans.push((PayloadSpan::within(payload, start..end), class));
        }
        start = end;
    }

    spans
}

/// The syntax class a Tree-sitter capture name folds to, by its leading parts; captures
/// with no class here, such as punctuation, stay plain.
fn fold(capture: &str) -> Option<SyntaxClass> {
    let first = capture.split('.').next().unwrap_or(capture);
    match first {
        "keyword" | "conditional" | "repeat" | "include" | "exception" => {
            Some(SyntaxClass::Keyword)
        }
        "string" | "character" => Some(SyntaxClass::String),
        "comment" => Some(SyntaxClass::Comment),
        "number" | "float" => Some(SyntaxClass::Number),
        "function" | "method" | "constructor" => Some(SyntaxClass::Function),
        "type" | "module" | "namespace" => Some(SyntaxClass::Type),
        "constant" | "boolean" => Some(SyntaxClass::Constant),
        "operator" => Some(SyntaxClass::Operator),
        "property" | "field" | "attribute" => Some(SyntaxClass::Property),
        "variable" | "parameter" => Some(SyntaxClass::Variable),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Captured, Highlighter, Hunk, fold, paint_bytes};
    use crate::{
        display_lines, languages,
        measurement::Unobserved,
        parser::parse_patch,
        prepared::{PageIndex, PreparedPage},
        span::PayloadSpan,
        styling::LineSyntax,
        syntax::{DisplayScan, SyntaxClass},
    };

    /// The hunks of `patch`'s only file, as the worker makes them for its first page.
    fn hunks(patch: &str) -> Vec<Hunk> {
        let document = parse_patch(patch.as_bytes()).expect("valid diff");
        let view = document.file_views().next().expect("one file");
        let file_types = languages::of_file(view.file);
        let mut scan = DisplayScan::of_file_types(file_types);
        let part = PreparedPage::of_lines(
            display_lines::of_file_view(view),
            &mut scan,
            &mut Unobserved,
        );

        Hunk::of_part(PageIndex::FIRST, file_types, 0, &part)
    }

    /// `payload` with each `(text, class)` piece spanning the first match of its text.
    fn syntax(payload: &str, pieces: &[(&str, SyntaxClass)]) -> LineSyntax {
        let spans = pieces.iter().map(|(text, class)| {
            let start = payload.find(text).expect("the piece is in the payload");
            (
                PayloadSpan::within(payload, start..start + text.len()),
                *class,
            )
        });
        LineSyntax::of_spans(payload, spans)
    }

    /// docs/spec/architecture.md#technology-direction: changed lines take their side's
    /// grammar, and context takes the new side's when both sides select one file type.
    #[test]
    fn restyles_changed_lines_by_side_and_context_from_the_new_side() {
        let [hunk] = hunks(
            "--- a/main.go\n+++ b/main.go\n@@ -1,2 +1,2 @@\n-x := 1\n+y := strings.ToUpper(\"a\")\n return y\n",
        )
        .try_into()
        .unwrap_or_else(|_| panic!("one hunk"));

        let restyled = Highlighter::default().restyle(&hunk);

        assert_eq!(
            restyled.lines,
            [
                (
                    3,
                    syntax(
                        "x := 1",
                        &[
                            ("x", SyntaxClass::Variable),
                            (":=", SyntaxClass::Operator),
                            ("1", SyntaxClass::Number)
                        ]
                    )
                ),
                (
                    4,
                    syntax(
                        "y := strings.ToUpper(\"a\")",
                        &[
                            ("y", SyntaxClass::Variable),
                            (":=", SyntaxClass::Operator),
                            ("strings", SyntaxClass::Variable),
                            ("ToUpper", SyntaxClass::Function),
                            ("\"a\"", SyntaxClass::String),
                        ]
                    )
                ),
                (
                    5,
                    syntax(
                        "return y",
                        &[
                            ("return", SyntaxClass::Keyword),
                            ("y", SyntaxClass::Variable)
                        ]
                    )
                ),
            ]
        );
    }

    /// docs/spec/architecture.md#technology-direction: context stays textual when the
    /// sides select different file types, and a side without a grammar keeps its colors.
    #[test]
    fn leaves_context_of_different_file_types_and_sides_without_a_grammar() {
        let [hunk] =
            hunks("--- a/main.go\n+++ b/main.rs\n@@ -1,2 +1,2 @@\n-x := 1\n+let y = 1;\n kept\n")
                .try_into()
                .unwrap_or_else(|_| panic!("one hunk"));

        let restyled = Highlighter::default().restyle(&hunk);

        let positions: Vec<_> = restyled
            .lines
            .iter()
            .map(|(position, _)| *position)
            .collect();
        assert_eq!(positions, [3]);
    }

    #[test]
    fn makes_no_hunks_for_a_file_without_a_grammar() {
        assert!(hunks("--- a/notes.txt\n+++ b/notes.txt\n@@ -1 +1 @@\n-a\n+b\n").is_empty());
    }

    /// docs/spec/architecture.md#technology-direction: overlaps resolve innermost first,
    /// then the later pattern.
    #[test]
    fn paints_the_innermost_capture_then_the_later_pattern() {
        let captured = vec![
            Captured {
                bytes: 0..8,
                pattern: 3,
                class: SyntaxClass::Keyword,
            },
            Captured {
                bytes: 2..4,
                pattern: 0,
                class: SyntaxClass::String,
            },
            Captured {
                bytes: 5..7,
                pattern: 2,
                class: SyntaxClass::Type,
            },
            Captured {
                bytes: 5..7,
                pattern: 1,
                class: SyntaxClass::Function,
            },
        ];

        let painted = paint_bytes(9, captured);

        use SyntaxClass::{Keyword, String, Type};
        assert_eq!(
            painted,
            [
                Some(Keyword),
                Some(Keyword),
                Some(String),
                Some(String),
                Some(Keyword),
                Some(Type),
                Some(Type),
                Some(Keyword),
                None
            ]
        );
    }

    #[test]
    fn folds_captures_by_their_leading_part() {
        assert_eq!(fold("keyword.function"), Some(SyntaxClass::Keyword));
        assert_eq!(fold("function.method.call"), Some(SyntaxClass::Function));
        assert_eq!(fold("string.escape"), Some(SyntaxClass::String));
        assert_eq!(fold("punctuation.bracket"), None);
        assert_eq!(fold("spell"), None);
    }
}
