//! Character detail: the spans of each changed line that differ from its peer. Rules:
//! `docs/spec/architecture.md` (View model).

use similar::{Algorithm, ChangeTag, utils::diff_slices};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    page_rows::{LineIndex, LineMap, PageRows, SideBySideRow},
    side::{BySide, Side},
    span::PayloadSpan,
};

const MAX_EQUAL_ISLAND_GRAPHEMES: usize = 3;

/// The character-detail spans of each `page_rows` line that has changed spans: a paired
/// row marks the spans that differ from its peer, and an unmatched row is changed as a
/// whole. Each changed line takes its side's detail from the pair holding it.
pub fn character_details(page_rows: &PageRows) -> LineMap<Option<ChangedDetail>> {
    let payload = |line: LineIndex| page_rows.line(line).payload();
    let whole = |line| Box::new([PayloadSpan::whole(payload(line))]) as Box<[_]>;

    let mut pair_details = page_rows.map_pairs(|_, row| match row {
        SideBySideRow::Paired(lines) => {
            let spans = changed_pair_spans(lines.map(payload));
            BySide {
                old: ChangedDetail::of(Side::Old, spans.old.into_boxed_slice()),
                new: ChangedDetail::of(Side::New, spans.new.into_boxed_slice()),
            }
        }
        SideBySideRow::OldOnly(line) => BySide {
            old: ChangedDetail::of(Side::Old, whole(line)),
            new: None,
        },
        SideBySideRow::NewOnly(line) => BySide {
            old: None,
            new: ChangedDetail::of(Side::New, whole(line)),
        },
        SideBySideRow::Shared(_) => BySide::default(),
    });

    page_rows.map_lines(|index, line| {
        let side = line.kind().changed_side()?;
        pair_details
            .get_mut(page_rows.pair_of(index))
            .get_mut(side)
            .take()
    })
}

/// The changed spans of a line that changes `side`.
#[derive(Debug, PartialEq, Eq)]
pub struct ChangedDetail {
    side: Side,
    spans: Box<[PayloadSpan]>,
}

impl ChangedDetail {
    /// The detail of a line that changes `side`, when any of `spans` is not empty.
    fn of(side: Side, spans: Box<[PayloadSpan]>) -> Option<Self> {
        let changes_text = spans.iter().any(|span| !span.is_empty());

        changes_text.then_some(Self { side, spans })
    }

    pub fn side(&self) -> Side {
        self.side
    }

    pub fn spans(&self) -> &[PayloadSpan] {
        &self.spans
    }
}

/// The spans that differ between a changed pair's payloads, found by diffing their
/// tokens.
fn changed_pair_spans(payloads: BySide<&str>) -> BySide<Vec<PayloadSpan>> {
    let tokens = payloads.map(tokens);
    let mut changes = diff_slices(Algorithm::Myers, &tokens.old, &tokens.new)
        .into_iter()
        .peekable();
    let mut cursor = PairCursor::new(payloads);

    while let Some((tag, tokens)) = changes.next() {
        let replacement = match tag {
            ChangeTag::Equal => {
                cursor.take(tag, tokens_width(tokens));
                continue;
            }
            ChangeTag::Delete => match changes.next_if(|(next, _)| *next == ChangeTag::Insert) {
                Some((_, inserted)) => Replacement::Replaced {
                    deleted: tokens,
                    inserted,
                },
                None => Replacement::Deleted(tokens),
            },
            ChangeTag::Insert => match changes.next_if(|(next, _)| *next == ChangeTag::Delete) {
                Some((_, deleted)) => Replacement::Replaced {
                    deleted,
                    inserted: tokens,
                },
                None => Replacement::Inserted(tokens),
            },
        };
        replacement.mark(&mut cursor);
    }

    cursor
        .spans
        .zip(payloads)
        .map(|(spans, payload)| coalesce_tiny_equal_islands(spans, payload))
}

/// Walks a changed pair's payloads together, marking the changed spans of each side.
struct PairCursor<'a> {
    payloads: BySide<&'a str>,
    offsets: BySide<usize>,
    spans: BySide<Vec<PayloadSpan>>,
}

impl<'a> PairCursor<'a> {
    fn new(payloads: BySide<&'a str>) -> Self {
        Self {
            payloads,
            offsets: BySide::default(),
            spans: BySide::default(),
        }
    }

    /// Takes the next `width` bytes `tag` names: equal text passes on both sides, deleted
    /// text is marked on the old side, and inserted text on the new side.
    fn take(&mut self, tag: ChangeTag, width: usize) {
        match tag {
            ChangeTag::Equal => {
                self.offsets.old += width;
                self.offsets.new += width;
            }
            ChangeTag::Delete => self.mark(Side::Old, width),
            ChangeTag::Insert => self.mark(Side::New, width),
        }
    }

    /// Marks the next `width` bytes of `side` changed.
    fn mark(&mut self, side: Side, width: usize) {
        let start = *self.offsets.get(side);
        let end = start + width;

        let span = PayloadSpan::within(self.payloads.get(side), start, end);
        self.spans.get_mut(side).push(span);
        *self.offsets.get_mut(side) = end;
    }
}

// Tokens

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum TokenKind {
    Word,
    Delimiter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Token<'a> {
    text: &'a str,
    kind: TokenKind,
}

fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut characters = text.char_indices();
    let Some((start, first)) = characters.next() else {
        return Vec::new();
    };
    let mut start = start;
    let mut previous = first;
    let mut kind = token_kind(first);
    let mut tokens = Vec::new();

    for (index, character) in characters {
        let next_kind = token_kind(character);
        let camel_case_boundary = kind == TokenKind::Word
            && next_kind == TokenKind::Word
            && previous.is_lowercase()
            && character.is_uppercase();
        if kind != next_kind || camel_case_boundary {
            tokens.push(Token {
                text: &text[start..index],
                kind,
            });
            start = index;
            kind = next_kind;
        }
        previous = character;
    }
    tokens.push(Token {
        text: &text[start..],
        kind,
    });

    tokens
}

fn token_kind(character: char) -> TokenKind {
    match character.is_alphanumeric() || character == '_' {
        true => TokenKind::Word,
        false => TokenKind::Delimiter,
    }
}

fn tokens_width(tokens: &[Token<'_>]) -> usize {
    tokens.iter().map(|token| token.text.len()).sum()
}

// Replacements

/// A run of changed tokens: deleted only, inserted only, or deleted and replaced.
enum Replacement<'t, 'a> {
    Deleted(&'t [Token<'a>]),
    Inserted(&'t [Token<'a>]),
    Replaced {
        deleted: &'t [Token<'a>],
        inserted: &'t [Token<'a>],
    },
}

impl Replacement<'_, '_> {
    /// Marks the changed text. A word replaced by one similar word marks only the
    /// graphemes that differ; any other change is marked whole.
    fn mark(&self, cursor: &mut PairCursor<'_>) {
        match self {
            Self::Deleted(deleted) => cursor.mark(Side::Old, tokens_width(deleted)),
            Self::Inserted(inserted) => cursor.mark(Side::New, tokens_width(inserted)),
            Self::Replaced { deleted, inserted } => match similar_word_changes(deleted, inserted) {
                Some(changes) => {
                    for (tag, width) in changes {
                        cursor.take(tag, width);
                    }
                }
                None => {
                    cursor.mark(Side::Old, tokens_width(deleted));
                    cursor.mark(Side::New, tokens_width(inserted));
                }
            },
        }
    }
}

/// The grapheme changes, as tags and byte widths, when one word replaces one similar
/// word: one sharing at least half of the shorter word's graphemes. Any other replacement
/// has none.
fn similar_word_changes(
    deleted: &[Token<'_>],
    inserted: &[Token<'_>],
) -> Option<Vec<(ChangeTag, usize)>> {
    let ([before], [after]) = (deleted, inserted) else {
        return None;
    };
    if before.kind != TokenKind::Word || after.kind != TokenKind::Word {
        return None;
    }
    let before_graphemes = before.text.graphemes(true).collect::<Vec<_>>();
    let after_graphemes = after.text.graphemes(true).collect::<Vec<_>>();
    let changes = diff_slices(Algorithm::Myers, &before_graphemes, &after_graphemes);

    let common = changes
        .iter()
        .filter(|(tag, _)| *tag == ChangeTag::Equal)
        .map(|(_, graphemes)| graphemes.len())
        .sum::<usize>();
    let shortest = before_graphemes.len().min(after_graphemes.len());
    if common.saturating_mul(2) < shortest {
        return None;
    }

    let widths = changes.into_iter().map(|(tag, graphemes)| {
        let width = graphemes.iter().map(|grapheme| grapheme.len()).sum();
        (tag, width)
    });
    Some(widths.collect())
}

// Spans

/// `spans` with each gap of at most `MAX_EQUAL_ISLAND_GRAPHEMES` graphemes of `payload`
/// between neighbors absorbed into one span.
fn coalesce_tiny_equal_islands(spans: Vec<PayloadSpan>, payload: &str) -> Vec<PayloadSpan> {
    spans.into_iter().fold(Vec::new(), |mut coalesced, span| {
        match coalesced.last_mut() {
            Some(previous) if is_tiny_island(&payload[previous.end()..span.start()]) => {
                *previous = previous.through(span);
            }
            _ => coalesced.push(span),
        }
        coalesced
    })
}

fn is_tiny_island(island: &str) -> bool {
    island.graphemes(true).count() <= MAX_EQUAL_ISLAND_GRAPHEMES
}

#[cfg(test)]
mod tests {
    use super::{PayloadSpan, changed_pair_spans, character_details};
    use crate::{page_rows::PageRows, parser::parse_patch, side::BySide};

    fn spans_of(old: &str, new: &str) -> BySide<Vec<PayloadSpan>> {
        changed_pair_spans(BySide { old, new })
    }

    fn spans(text: &str, ranges: &[(usize, usize)]) -> Vec<PayloadSpan> {
        ranges
            .iter()
            .map(|(start, end)| PayloadSpan::within(text, *start, *end))
            .collect()
    }

    #[test]
    fn unmatched_payloads_are_fully_changed_in_character_mode() {
        for (hunk, records, expected) in [
            ("-0,0 +1,2", "+café\r\n+\r\n", vec![("+café", 0, 5)]),
            ("-1,2 +0,0", "-café\r\n-\r\n", vec![("-café", 0, 5)]),
            (
                "-1 +1,2",
                "-let café = old;\n+let café = new;\n+café\n",
                vec![
                    ("-let café = old;", 12, 15),
                    ("+let café = new;", 12, 15),
                    ("+café", 0, 5),
                ],
            ),
            (
                "-1,2 +1",
                "-let café = old;\n-café\n+let café = new;\n",
                vec![
                    ("-let café = old;", 12, 15),
                    ("-café", 0, 5),
                    ("+let café = new;", 12, 15),
                ],
            ),
        ] {
            let patch = format!("--- a/file\n+++ b/file\n@@ {hunk} @@\n{records}");
            let document = parse_patch(patch.as_bytes()).expect("valid unmatched diff");
            let page_rows = PageRows::of_file(&document.file_views().next().expect("first file"));
            let details = character_details(&page_rows);

            for (row, line) in page_rows.lines() {
                let found = details
                    .get(row)
                    .as_ref()
                    .map_or(&[][..], |detail| detail.spans());
                let shown = format!("{}{}", line.kind().marker(), line.payload());
                let wanted = expected.iter().find(|(text, _, _)| *text == shown);
                let wanted = wanted.map(|(_, start, end)| (*start, *end));
                assert_eq!(
                    found,
                    spans(line.payload(), wanted.as_slice()),
                    "{hunk}: {shown:?}"
                );
            }
        }
    }

    #[test]
    fn marks_only_the_replaced_graphemes_in_a_changed_pair() {
        let (old, new) = ("let café = old;", "let café = new;");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: spans(old, &[(12, 15)]),
                new: spans(new, &[(12, 15)]),
            }
        );
    }

    #[test]
    fn keeps_unrelated_identifier_replacements_whole() {
        let (old, new) = ("DiffLayout", "ViewPreferences");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: spans(old, &[(0, 10)]),
                new: spans(new, &[(0, 15)]),
            }
        );
    }

    #[test]
    fn preserves_matching_camel_case_tokens() {
        let (old, new) = ("configPath", "configFile");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: spans(old, &[(6, 10)]),
                new: spans(new, &[(6, 10)]),
            }
        );
    }

    #[test]
    fn coalesces_tiny_equal_islands_between_changed_spans() {
        let (old, new) = ("side-by-side", "vertical-split");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: spans(old, &[(0, 12)]),
                new: spans(new, &[(0, 14)]),
            }
        );
    }
}
