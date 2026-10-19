//! Character detail: the spans of each changed line that differ from its peer. Rules:
//! docs/spec/architecture.md#palette-and-character-detail.

use std::iter;

use similar::{Algorithm, ChangeTag, utils::diff_slices};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    page_rows::{LineIndex, LineMap, PageRows},
    side::{BySide, Side},
    span::PayloadSpan,
};

/// The most graphemes an unchanged gap between two changed spans can hold and still be
/// absorbed into one span.
const MAX_EQUAL_ISLAND_GRAPHEMES: usize = 3;

/// The character-detail spans of each `rows` line that has changed spans: a changed
/// payload beside its peer's payload marks the spans that differ from it, and a changed
/// payload without one is changed as a whole. Each changed line takes its side's detail
/// from the pair holding it. A no-newline marker says something about its record, not a
/// change, so it has none and is no payload's peer.
pub fn character_details(rows: &PageRows) -> LineMap<Option<ChangedDetail>> {
    let payload_line = |line: LineIndex| {
        let is_payload = !rows.line(line).kind().is_no_newline_marker();
        is_payload.then_some(line)
    };

    rows.line_values_by_pair(|row| {
        let payload_lines = row.changed_lines().map(|line| line.and_then(payload_line));

        changed_payload_details(rows, payload_lines)
    })
}

/// The detail of a changed row's payload `lines`: a payload beside its peer marks the
/// spans that differ, and a payload alone is changed as a whole.
fn changed_payload_details(
    rows: &PageRows,
    lines: BySide<Option<LineIndex>>,
) -> BySide<Option<ChangedDetail>> {
    let payload = |line: LineIndex| rows.line(line).payload();
    let whole = |side, line| ChangedDetail::of(side, Box::new([PayloadSpan::whole(payload(line))]));

    match lines.transpose() {
        Some(pair) => changed_pair_spans(pair.map(payload))
            .map_sides(|side, spans| ChangedDetail::of(side, spans.into_boxed_slice())),
        None => lines.map_sides(|side, line| line.and_then(|line| whole(side, line))),
    }
}

/// The changed spans of a line that changes `side`.
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
    let token_lists = payloads.map(tokens);
    let mut changes = diff_slices(Algorithm::Myers, &token_lists.old, &token_lists.new)
        .into_iter()
        .peekable();
    let mut cursor = PairCursor::new(payloads);

    while let Some((tag, run)) = changes.next() {
        let token_run = match tag {
            ChangeTag::Equal => TokenRun::Kept(run),
            ChangeTag::Delete => match changes.next_if(|(next, _)| *next == ChangeTag::Insert) {
                Some((_, inserted)) => TokenRun::Replaced {
                    deleted: run,
                    inserted,
                },
                None => TokenRun::Deleted(run),
            },
            ChangeTag::Insert => match changes.next_if(|(next, _)| *next == ChangeTag::Delete) {
                Some((_, deleted)) => TokenRun::Replaced {
                    deleted,
                    inserted: run,
                },
                None => TokenRun::Inserted(run),
            },
        };
        token_run.advance(&mut cursor);
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

    /// Passes the next `bytes` bytes of both sides, which are equal, unmarked.
    fn pass(&mut self, bytes: usize) {
        self.offsets = self.offsets.map(|offset| offset + bytes);
    }

    /// Marks the next `bytes` bytes of `side` changed.
    fn mark(&mut self, side: Side, bytes: usize) {
        let start = *self.offsets.get(side);
        let end = start + bytes;

        let span = PayloadSpan::within(self.payloads.get(side), start..end);
        self.spans.get_mut(side).push(span);
        *self.offsets.get_mut(side) = end;
    }
}

// Tokens

/// Whether a token is part of a word or a run of delimiters between words.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum TokenKind {
    Word,
    Delimiter,
}

/// A run of a payload's characters of one kind: the unit a changed pair is diffed by.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Token<'a> {
    text: &'a str,
    kind: TokenKind,
}

/// Where a token starts in its text, and its kind.
struct TokenStart {
    at: usize,
    kind: TokenKind,
}

/// `text` split into tokens, in order: a token ends where the next one starts.
fn tokens(text: &str) -> Vec<Token<'_>> {
    let Some(first) = text.chars().next() else {
        return Vec::new();
    };

    let later_starts = text
        .chars()
        .zip(text.char_indices().skip(1))
        .filter(|&(previous, (_, current))| starts_token(previous, current))
        .map(|(_, (at, current))| TokenStart {
            at,
            kind: token_kind(current),
        });
    let starts = iter::once(TokenStart {
        at: 0,
        kind: token_kind(first),
    })
    .chain(later_starts)
    .collect::<Vec<_>>();
    let ends = starts
        .iter()
        .skip(1)
        .map(|start| start.at)
        .chain(iter::once(text.len()));

    starts
        .iter()
        .zip(ends)
        .map(|(start, end)| Token {
            text: &text[start.at..end],
            kind: start.kind,
        })
        .collect()
}

/// Whether `current` starts a new token after `previous`: the token kind changes, or an
/// uppercase letter follows a lowercase one inside a word.
fn starts_token(previous: char, current: char) -> bool {
    let previous_kind = token_kind(previous);
    let current_kind = token_kind(current);
    let camel_case_boundary = previous_kind == TokenKind::Word
        && current_kind == TokenKind::Word
        && previous.is_lowercase()
        && current.is_uppercase();

    previous_kind != current_kind || camel_case_boundary
}

/// Letters, digits and `_` make words; every other character is a delimiter.
fn token_kind(character: char) -> TokenKind {
    match character.is_alphanumeric() || character == '_' {
        true => TokenKind::Word,
        false => TokenKind::Delimiter,
    }
}

/// The bytes `tokens` cover together.
fn tokens_len(tokens: &[Token<'_>]) -> usize {
    tokens.iter().map(|token| token.text.len()).sum()
}

// Token runs

/// A run of tokens: kept on both sides, deleted only, inserted only, or deleted and
/// replaced.
enum TokenRun<'t, 'a> {
    Kept(&'t [Token<'a>]),
    Deleted(&'t [Token<'a>]),
    Inserted(&'t [Token<'a>]),
    Replaced {
        deleted: &'t [Token<'a>],
        inserted: &'t [Token<'a>],
    },
}

impl TokenRun<'_, '_> {
    /// Moves `cursor` past the run, marking its changed text. A word replaced by one
    /// similar word marks only the graphemes that differ; any other change is marked
    /// whole.
    fn advance(&self, cursor: &mut PairCursor<'_>) {
        match self {
            Self::Kept(kept) => cursor.pass(tokens_len(kept)),
            Self::Deleted(deleted) => cursor.mark(Side::Old, tokens_len(deleted)),
            Self::Inserted(inserted) => cursor.mark(Side::New, tokens_len(inserted)),
            Self::Replaced { deleted, inserted } => match similar_word_changes(deleted, inserted) {
                Some(changes) => {
                    for change in changes {
                        change.advance(cursor);
                    }
                }
                None => {
                    cursor.mark(Side::Old, tokens_len(deleted));
                    cursor.mark(Side::New, tokens_len(inserted));
                }
            },
        }
    }
}

/// A run of graphemes within a replaced word, measured in bytes: kept on both sides, or
/// changed on one.
enum GraphemeChange {
    Kept(usize),
    Changed(Side, usize),
}

impl GraphemeChange {
    /// The run `tag` names: an equal run is kept, a deletion changes the old side, and an
    /// insertion changes the new side.
    fn of(tag: ChangeTag, bytes: usize) -> Self {
        match tag {
            ChangeTag::Equal => Self::Kept(bytes),
            ChangeTag::Delete => Self::Changed(Side::Old, bytes),
            ChangeTag::Insert => Self::Changed(Side::New, bytes),
        }
    }

    /// Moves `cursor` past the run, marking it when it is changed.
    fn advance(self, cursor: &mut PairCursor<'_>) {
        match self {
            Self::Kept(bytes) => cursor.pass(bytes),
            Self::Changed(side, bytes) => cursor.mark(side, bytes),
        }
    }
}

/// The grapheme changes when one word replaces one similar word: one sharing at least
/// half of the shorter word's graphemes. Any other replacement has none.
fn similar_word_changes(
    deleted: &[Token<'_>],
    inserted: &[Token<'_>],
) -> Option<Vec<GraphemeChange>> {
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

    let grapheme_changes = changes.into_iter().map(|(tag, graphemes)| {
        let bytes = graphemes.iter().map(|grapheme| grapheme.len()).sum();
        GraphemeChange::of(tag, bytes)
    });
    Some(grapheme_changes.collect())
}

// Spans

/// `spans` with each gap of at most `MAX_EQUAL_ISLAND_GRAPHEMES` graphemes of `payload`
/// between neighbors absorbed into one span.
fn coalesce_tiny_equal_islands(spans: Vec<PayloadSpan>, payload: &str) -> Vec<PayloadSpan> {
    spans.into_iter().fold(Vec::new(), |mut coalesced, span| {
        let absorbing = coalesced
            .last_mut()
            .filter(|previous| is_tiny_island(&payload[previous.end()..span.start()]));

        match absorbing {
            Some(previous) => *previous = previous.through(span),
            None => coalesced.push(span),
        }
        coalesced
    })
}

/// Whether the unchanged `island` between two changed spans is small enough to absorb.
fn is_tiny_island(island: &str) -> bool {
    island.graphemes(true).count() <= MAX_EQUAL_ISLAND_GRAPHEMES
}

#[cfg(test)]
mod tests {
    use super::{changed_pair_spans, character_details};
    use crate::{page_rows::PageRows, parser::parse_patch, side::BySide, span::PayloadSpan};

    /// The changed spans of the pair `old` and `new`.
    fn spans_of(old: &str, new: &str) -> BySide<Vec<PayloadSpan>> {
        changed_pair_spans(BySide { old, new })
    }

    #[test]
    fn unmatched_payloads_are_fully_changed_in_character_mode() {
        for (hunk, records, expected) in [
            ("-0,0 +1,2", "+café\r\n+\r\n", vec![("+café", 0..5)]),
            ("-1,2 +0,0", "-café\r\n-\r\n", vec![("-café", 0..5)]),
            (
                "-1 +1,2",
                "-let café = old;\n+let café = new;\n+café\n",
                vec![
                    ("-let café = old;", 12..15),
                    ("+let café = new;", 12..15),
                    ("+café", 0..5),
                ],
            ),
            (
                "-1,2 +1",
                "-let café = old;\n-café\n+let café = new;\n",
                vec![
                    ("-let café = old;", 12..15),
                    ("-café", 0..5),
                    ("+let café = new;", 12..15),
                ],
            ),
            (
                "-1,2 +1",
                "-let café = old;\n-café\n\\ No newline at end of file\n+let café = new;\n",
                vec![
                    ("-let café = old;", 12..15),
                    ("-café", 0..5),
                    ("+let café = new;", 12..15),
                ],
            ),
            (
                "-1,2 +1,3",
                "-old a\n-old b\n\\ No newline at end of file\n+new a\n+new b\n+café\n",
                vec![
                    ("-old a", 0..3),
                    ("+new a", 0..3),
                    ("-old b", 0..3),
                    ("+new b", 0..3),
                    ("+café", 0..5),
                ],
            ),
        ] {
            let patch = format!("--- a/file\n+++ b/file\n@@ {hunk} @@\n{records}");
            let document = parse_patch(patch.as_bytes()).expect("valid unmatched diff");
            let rows = PageRows::of_file_view(document.file_views().next().expect("first file"));
            let details = character_details(&rows);

            for (index, line) in rows.lines() {
                let found = details
                    .get(index)
                    .as_ref()
                    .map_or(&[][..], |detail| detail.spans());
                let shown = format!("{}{}", line.kind().marker(), line.payload());
                let wanted = expected.iter().find(|(text, _)| *text == shown);
                let wanted_span =
                    wanted.map(|(_, bytes)| PayloadSpan::within(line.payload(), bytes.clone()));
                assert_eq!(found, wanted_span.as_slice(), "{hunk}: {shown:?}");
            }
        }
    }

    #[test]
    fn marks_only_the_replaced_graphemes_in_a_changed_pair() {
        let (old, new) = ("let café = old;", "let café = new;");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: vec![PayloadSpan::within(old, 12..15)],
                new: vec![PayloadSpan::within(new, 12..15)],
            }
        );
    }

    #[test]
    fn keeps_unrelated_identifier_replacements_whole() {
        let (old, new) = ("DiffLayout", "ViewPreferences");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: vec![PayloadSpan::within(old, 0..10)],
                new: vec![PayloadSpan::within(new, 0..15)],
            }
        );
    }

    #[test]
    fn preserves_matching_camel_case_tokens() {
        let (old, new) = ("configPath", "configFile");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: vec![PayloadSpan::within(old, 6..10)],
                new: vec![PayloadSpan::within(new, 6..10)],
            }
        );
    }

    #[test]
    fn coalesces_tiny_equal_islands_between_changed_spans() {
        let (old, new) = ("side-by-side", "vertical-split");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: vec![PayloadSpan::within(old, 0..12)],
                new: vec![PayloadSpan::within(new, 0..14)],
            }
        );
    }
}
