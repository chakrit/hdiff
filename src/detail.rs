use similar::{Algorithm, ChangeTag, utils::diff_slices};
use unicode_segmentation::UnicodeSegmentation;

use crate::layout::{Layout, SideBySideRow};

const MAX_EQUAL_ISLAND_GRAPHEMES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedPairDetail {
    pub before: Vec<DetailSpan>,
    pub after: Vec<DetailSpan>,
}

pub fn changed_pair_detail(before: &str, after: &str) -> ChangedPairDetail {
    let before_tokens = tokens(before);
    let after_tokens = tokens(after);
    let changes = diff_slices(Algorithm::Myers, &before_tokens, &after_tokens);
    let mut before_offset = 0;
    let mut after_offset = 0;
    let mut detail = ChangedPairDetail {
        before: Vec::new(),
        after: Vec::new(),
    };

    let mut changes = changes.into_iter().peekable();
    while let Some((tag, tokens)) = changes.next() {
        match tag {
            ChangeTag::Equal => {
                let width = tokens_width(tokens);
                before_offset += width;
                after_offset += width;
            }
            ChangeTag::Delete => {
                let replacement = changes
                    .next_if(|(next_tag, _)| *next_tag == ChangeTag::Insert)
                    .map(|(_, replacement)| replacement);
                add_replacement_detail(
                    &mut detail,
                    Some(tokens),
                    replacement,
                    before_offset,
                    after_offset,
                );
                before_offset += tokens_width(tokens);
                after_offset += replacement
                    .as_ref()
                    .map_or(0, |tokens| tokens_width(tokens));
            }
            ChangeTag::Insert => {
                let replacement = changes
                    .next_if(|(next_tag, _)| *next_tag == ChangeTag::Delete)
                    .map(|(_, replacement)| replacement);
                add_replacement_detail(
                    &mut detail,
                    replacement,
                    Some(tokens),
                    before_offset,
                    after_offset,
                );
                before_offset += replacement
                    .as_ref()
                    .map_or(0, |tokens| tokens_width(tokens));
                after_offset += tokens_width(tokens);
            }
        }
    }

    coalesce_tiny_equal_islands(&mut detail.before, before);
    coalesce_tiny_equal_islands(&mut detail.after, after);

    detail
}

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

fn add_replacement_detail(
    detail: &mut ChangedPairDetail,
    before: Option<&[Token<'_>]>,
    after: Option<&[Token<'_>]>,
    before_offset: usize,
    after_offset: usize,
) {
    let Some(before_tokens) = before else {
        let Some(after_tokens) = after else {
            return;
        };
        add_span(&mut detail.after, after_offset, tokens_width(after_tokens));
        return;
    };
    let Some(after_tokens) = after else {
        add_span(
            &mut detail.before,
            before_offset,
            tokens_width(before_tokens),
        );
        return;
    };
    let [before] = before_tokens else {
        add_span(
            &mut detail.before,
            before_offset,
            tokens_width(before_tokens),
        );
        add_span(&mut detail.after, after_offset, tokens_width(after_tokens));
        return;
    };
    let [after] = after_tokens else {
        add_span(
            &mut detail.before,
            before_offset,
            tokens_width(before_tokens),
        );
        add_span(&mut detail.after, after_offset, tokens_width(after_tokens));
        return;
    };
    if before.kind == TokenKind::Word
        && after.kind == TokenKind::Word
        && similar_words(before.text, after.text)
    {
        let detail_spans = grapheme_detail(before.text, after.text);
        add_offset_spans(&mut detail.before, &detail_spans.before, before_offset);
        add_offset_spans(&mut detail.after, &detail_spans.after, after_offset);
        return;
    }

    add_span(&mut detail.before, before_offset, before.text.len());
    add_span(&mut detail.after, after_offset, after.text.len());
}

fn tokens_width(tokens: &[Token<'_>]) -> usize {
    tokens.iter().map(|token| token.text.len()).sum()
}

fn similar_words(before: &str, after: &str) -> bool {
    let before_graphemes = before.graphemes(true).collect::<Vec<_>>();
    let after_graphemes = after.graphemes(true).collect::<Vec<_>>();
    let common = diff_slices(Algorithm::Myers, &before_graphemes, &after_graphemes)
        .iter()
        .filter(|(tag, _)| *tag == ChangeTag::Equal)
        .map(|(_, graphemes)| graphemes.len())
        .sum::<usize>();
    let shortest = before_graphemes.len().min(after_graphemes.len());

    common.saturating_mul(2) >= shortest
}

fn grapheme_detail(before: &str, after: &str) -> ChangedPairDetail {
    let before_graphemes = before.graphemes(true).collect::<Vec<_>>();
    let after_graphemes = after.graphemes(true).collect::<Vec<_>>();
    let changes = diff_slices(Algorithm::Myers, &before_graphemes, &after_graphemes);
    let mut before_offset = 0;
    let mut after_offset = 0;
    let mut detail = ChangedPairDetail {
        before: Vec::new(),
        after: Vec::new(),
    };

    for (tag, graphemes) in changes {
        let width = graphemes
            .iter()
            .map(|grapheme| grapheme.len())
            .sum::<usize>();
        match tag {
            ChangeTag::Equal => {
                before_offset += width;
                after_offset += width;
            }
            ChangeTag::Delete => {
                add_span(&mut detail.before, before_offset, width);
                before_offset += width;
            }
            ChangeTag::Insert => {
                add_span(&mut detail.after, after_offset, width);
                after_offset += width;
            }
        }
    }

    detail
}

fn add_offset_spans(target: &mut Vec<DetailSpan>, spans: &[DetailSpan], offset: usize) {
    target.extend(spans.iter().map(|span| DetailSpan {
        start: span.start + offset,
        end: span.end + offset,
    }));
}

fn coalesce_tiny_equal_islands(spans: &mut Vec<DetailSpan>, text: &str) {
    let mut coalesced = Vec::new();

    for span in spans.drain(..) {
        let Some(previous) = coalesced.last_mut() else {
            coalesced.push(span);
            continue;
        };
        let island = &text[previous.end..span.start];
        if island.graphemes(true).count() <= MAX_EQUAL_ISLAND_GRAPHEMES {
            previous.end = span.end;
        } else {
            coalesced.push(span);
        }
    }

    *spans = coalesced;
}

fn add_span(target: &mut Vec<DetailSpan>, start: usize, width: usize) {
    target.push(DetailSpan {
        start,
        end: start + width,
    });
}

/// One row's character-detail spans.
#[derive(Debug, PartialEq, Eq)]
pub struct RowDetail {
    pub row: usize,
    pub spans: Vec<DetailSpan>,
}

/// The character detail of each `layout` row that has changed spans: a paired row marks
/// the spans that differ from its peer, and an unmatched row is changed as a whole.
pub fn character_details(layout: &Layout) -> Vec<RowDetail> {
    let payload = |row: usize| layout.diff_lines[row].payload();
    let mut details = Vec::new();

    for row in &layout.side_by_side_rows {
        match row {
            SideBySideRow::Paired { before, after } => {
                let pair = changed_pair_detail(payload(*before), payload(*after));
                details.push(RowDetail {
                    row: *before,
                    spans: pair.before,
                });
                details.push(RowDetail {
                    row: *after,
                    spans: pair.after,
                });
            }
            SideBySideRow::BeforeOnly(row) | SideBySideRow::AfterOnly(row) => {
                let mut spans = Vec::new();
                add_span(&mut spans, 0, payload(*row).len());
                details.push(RowDetail { row: *row, spans });
            }
            SideBySideRow::Shared(_) => {}
        }
    }

    details.retain(|detail| detail.spans.iter().any(|span| span.start < span.end));
    details
}

#[cfg(test)]
mod tests {
    use super::{ChangedPairDetail, DetailSpan, changed_pair_detail, character_details};
    use crate::{layout::file_layout, parser::parse_unified_diff};

    #[test]
    fn unmatched_payloads_are_fully_changed_in_character_mode() {
        for (hunk, records, expected) in [
            ("-0,0 +1,2", "+café\r\n+\r\n", vec![("+café\r\n", 0, 5)]),
            ("-1,2 +0,0", "-café\r\n-\r\n", vec![("-café\r\n", 0, 5)]),
            (
                "-1 +1,2",
                "-let café = old;\n+let café = new;\n+café\n",
                vec![
                    ("-let café = old;\n", 12, 15),
                    ("+let café = new;\n", 12, 15),
                    ("+café\n", 0, 5),
                ],
            ),
            (
                "-1,2 +1",
                "-let café = old;\n-café\n+let café = new;\n",
                vec![
                    ("-let café = old;\n", 12, 15),
                    ("-café\n", 0, 5),
                    ("+let café = new;\n", 12, 15),
                ],
            ),
        ] {
            let patch = format!("--- a/file\n+++ b/file\n@@ {hunk} @@\n{records}");
            let document = parse_unified_diff(patch.as_bytes()).expect("valid unmatched diff");
            let layout = file_layout(&document.file_views().next().expect("first file"));
            let details = character_details(&layout);

            for (row, line) in layout.diff_lines.iter().enumerate() {
                let spans = details
                    .iter()
                    .find(|detail| detail.row == row)
                    .map_or(&[][..], |detail| detail.spans.as_slice());
                let wanted = expected.iter().find(|(text, _, _)| *text == line.text());
                let wanted = wanted.map(|(_, start, end)| DetailSpan {
                    start: *start,
                    end: *end,
                });
                assert_eq!(
                    spans,
                    wanted.into_iter().collect::<Vec<_>>(),
                    "{hunk}: {:?}",
                    line.text()
                );
            }
        }
    }

    #[test]
    fn marks_only_the_replaced_graphemes_in_a_changed_pair() {
        let detail = changed_pair_detail("let café = old;", "let café = new;");

        assert_eq!(
            detail,
            ChangedPairDetail {
                before: vec![DetailSpan { start: 12, end: 15 }],
                after: vec![DetailSpan { start: 12, end: 15 }],
            }
        );
    }

    #[test]
    fn keeps_unrelated_identifier_replacements_whole() {
        let detail = changed_pair_detail("DiffLayout", "ViewPreferences");

        assert_eq!(
            detail,
            ChangedPairDetail {
                before: vec![DetailSpan { start: 0, end: 10 }],
                after: vec![DetailSpan { start: 0, end: 15 }],
            }
        );
    }

    #[test]
    fn preserves_matching_camel_case_tokens() {
        let detail = changed_pair_detail("configPath", "configFile");

        assert_eq!(
            detail,
            ChangedPairDetail {
                before: vec![DetailSpan { start: 6, end: 10 }],
                after: vec![DetailSpan { start: 6, end: 10 }],
            }
        );
    }

    #[test]
    fn coalesces_tiny_equal_islands_between_changed_spans() {
        let detail = changed_pair_detail("side-by-side", "vertical-split");

        assert_eq!(
            detail,
            ChangedPairDetail {
                before: vec![DetailSpan { start: 0, end: 12 }],
                after: vec![DetailSpan { start: 0, end: 14 }],
            }
        );
    }
}
