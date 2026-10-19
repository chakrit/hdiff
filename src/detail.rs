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
    let payload_line = |line: LineIndex| rows.line(line).kind().payload_record().map(|_| line);

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
    let mut changes = token_lists
        .as_ref()
        .merge(|old, new| diff_slices(Algorithm::Myers, old, new))
        .into_iter()
        .map(DiffRun::of)
        .peekable();
    let mut cursor = PairCursor::new(payloads);

    while let Some(run) = changes.next() {
        let replacement = run.changed.and_then(|side| {
            changes
                .next_if(|next| next.changed.is_some_and(|next_side| next_side != side))
                .map(|opposite| BySide::of_side(side, run.items, opposite.items))
        });
        let token_run = match replacement {
            Some(runs) => TokenRun::Replaced(runs),
            None => TokenRun::Marked(MarkedRun::of(run.changed, tokens_len(run.items))),
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

/// A run of tokens: kept on both sides or changed on one, measured in bytes, or deleted
/// and replaced.
enum TokenRun<'t, 'a> {
    Marked(MarkedRun),
    Replaced(BySide<&'t [Token<'a>]>),
}

impl TokenRun<'_, '_> {
    /// Moves `cursor` past the run, marking its changed text. A word replaced by one
    /// similar word marks only the graphemes that differ; any other change is marked
    /// whole.
    fn advance(self, cursor: &mut PairCursor<'_>) {
        match self {
            Self::Marked(run) => run.advance(cursor),
            Self::Replaced(runs) => match similar_word_changes(runs) {
                Some(changes) => {
                    for change in changes {
                        change.advance(cursor);
                    }
                }
                None => {
                    for (side, run) in runs.into_sides() {
                        cursor.mark(side, tokens_len(run));
                    }
                }
            },
        }
    }
}

/// One run of a diff between a pair's sequences: the side it changes, and its items.
struct DiffRun<'r, T> {
    /// The side the run changes; a kept run changes neither.
    changed: Option<Side>,
    items: &'r [T],
}

impl<'r, T> DiffRun<'r, T> {
    /// The run a `diff_slices` change names: its tag decides the side it changes.
    fn of((tag, items): (ChangeTag, &'r [T])) -> Self {
        let changed = match tag {
            ChangeTag::Equal => None,
            ChangeTag::Delete => Some(Side::Old),
            ChangeTag::Insert => Some(Side::New),
        };

        Self { changed, items }
    }
}

/// A run of a changed pair's text, measured in bytes, whether counted from tokens or
/// from the graphemes of a replaced word: kept on both sides, or changed on one.
enum MarkedRun {
    Kept(usize),
    Changed { side: Side, bytes: usize },
}

impl MarkedRun {
    /// The run of `bytes` that `changed` names: a run changing no side is kept, and one
    /// changing a side marks that side.
    fn of(changed: Option<Side>, bytes: usize) -> Self {
        match changed {
            None => Self::Kept(bytes),
            Some(side) => Self::Changed { side, bytes },
        }
    }

    /// Moves `cursor` past the run, marking it when it is changed.
    fn advance(self, cursor: &mut PairCursor<'_>) {
        match self {
            Self::Kept(bytes) => cursor.pass(bytes),
            Self::Changed { side, bytes } => cursor.mark(side, bytes),
        }
    }
}

/// The grapheme changes when one word replaces one similar word: one sharing at least
/// half of the shorter word's graphemes. Any other replacement has none.
fn similar_word_changes(runs: BySide<&[Token<'_>]>) -> Option<Vec<MarkedRun>> {
    let words = runs.map(one_word).transpose()?;

    let graphemes = words.map(|word| word.graphemes(true).collect::<Vec<_>>());
    let changes = graphemes
        .as_ref()
        .merge(|old, new| diff_slices(Algorithm::Myers, old, new))
        .into_iter()
        .map(DiffRun::of)
        .collect::<Vec<_>>();
    let common = changes
        .iter()
        .filter(|run| run.changed.is_none())
        .map(|run| run.items.len())
        .sum::<usize>();
    let shortest = graphemes.as_ref().map(Vec::len).merge(usize::min);

    let similar = common.saturating_mul(2) >= shortest;
    similar.then(|| {
        changes
            .into_iter()
            .map(|run| {
                let bytes = run.items.iter().map(|grapheme| grapheme.len()).sum();
                MarkedRun::of(run.changed, bytes)
            })
            .collect()
    })
}

/// The run's text when it is exactly one word token; any other run has none.
fn one_word<'t>(run: &[Token<'t>]) -> Option<&'t str> {
    let [token] = run else {
        return None;
    };

    (token.kind == TokenKind::Word).then_some(token.text)
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

    /// The span of `payload` that its first `changed` text covers.
    fn changed_span(payload: &str, changed: &str) -> PayloadSpan {
        let start = payload
            .find(changed)
            .expect("the payload holds the changed text");
        PayloadSpan::within(payload, start..start + changed.len())
    }

    #[test]
    fn changes_unmatched_lines_whole_and_paired_lines_by_span() {
        for (hunk, records, expected) in [
            ("-0,0 +1,2", "+café\r\n+\r\n", vec![("+café", "café")]),
            ("-1,2 +0,0", "-café\r\n-\r\n", vec![("-café", "café")]),
            (
                "-1 +1,2",
                "-let café = old;\n+let café = new;\n+café\n",
                vec![
                    ("-let café = old;", "old"),
                    ("+let café = new;", "new"),
                    ("+café", "café"),
                ],
            ),
            (
                "-1,2 +1",
                "-let café = old;\n-café\n+let café = new;\n",
                vec![
                    ("-let café = old;", "old"),
                    ("-café", "café"),
                    ("+let café = new;", "new"),
                ],
            ),
            (
                "-1,2 +1",
                "-let café = old;\n-café\n\\ No newline at end of file\n+let café = new;\n",
                vec![
                    ("-let café = old;", "old"),
                    ("-café", "café"),
                    ("+let café = new;", "new"),
                ],
            ),
            (
                "-1,2 +1,3",
                "-old a\n-old b\n\\ No newline at end of file\n+new a\n+new b\n+café\n",
                vec![
                    ("-old a", "old"),
                    ("+new a", "new"),
                    ("-old b", "old"),
                    ("+new b", "new"),
                    ("+café", "café"),
                ],
            ),
        ] {
            let patch = format!("--- a/file\n+++ b/file\n@@ {hunk} @@\n{records}");
            let document = parse_patch(patch.as_bytes()).expect("valid unmatched diff");
            let views = document.file_views().collect::<Vec<_>>();
            let [view] = views.as_slice() else {
                panic!("{hunk}: the patch holds one file, not {}", views.len());
            };
            let rows = PageRows::of_file_view(*view);
            let details = character_details(&rows);

            for (index, line) in rows.lines() {
                let found = details
                    .get(index)
                    .as_ref()
                    .map_or(&[][..], |detail| detail.spans());
                let shown = format!("{}{}", line.kind().marker(), line.payload());
                let wanted = expected.iter().find(|(text, _)| *text == shown);
                let wanted_span = wanted.map(|(_, changed)| changed_span(line.payload(), changed));
                assert_eq!(found, wanted_span.as_slice(), "{hunk}: {shown:?}");
            }
        }
    }

    /// docs/spec/architecture.md#palette-and-character-detail: grapheme spans mark only
    /// what differs between similar replacement words.
    #[test]
    fn marks_only_the_replaced_graphemes_in_a_changed_pair() {
        let (old, new) = ("let café = call;", "let café = cell;");
        let differing_grapheme = |payload: &str, word: &str, grapheme: &str| {
            let word_start = payload.find(word).expect("the payload holds the word");
            let in_word = word.find(grapheme).expect("the word holds the grapheme");
            let start = word_start + in_word;
            PayloadSpan::within(payload, start..start + grapheme.len())
        };

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: vec![differing_grapheme(old, "call", "a")],
                new: vec![differing_grapheme(new, "cell", "e")],
            }
        );
    }

    /// docs/spec/architecture.md#palette-and-character-detail: an unrelated word that
    /// replaces another inside an unchanged line is marked whole.
    #[test]
    fn marks_an_unrelated_replacement_word_whole_inside_an_unchanged_line() {
        let (old, new) = ("let café = old;", "let café = new;");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: vec![changed_span(old, "old")],
                new: vec![changed_span(new, "new")],
            }
        );
    }

    #[test]
    fn keeps_unrelated_identifier_replacements_whole() {
        let (old, new) = ("DiffLayout", "ViewPreferences");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: vec![changed_span(old, old)],
                new: vec![changed_span(new, new)],
            }
        );
    }

    #[test]
    fn preserves_matching_camel_case_tokens() {
        let (old, new) = ("configPath", "configFile");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: vec![changed_span(old, "Path")],
                new: vec![changed_span(new, "File")],
            }
        );
    }

    #[test]
    fn coalesces_tiny_equal_islands_between_changed_spans() {
        let (old, new) = ("side-by-side", "vertical-split");

        assert_eq!(
            spans_of(old, new),
            BySide {
                old: vec![changed_span(old, old)],
                new: vec![changed_span(new, new)],
            }
        );
    }
}
