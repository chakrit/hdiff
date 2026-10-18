//! How rows look: change backgrounds and dimming, alignment peers, syntax and character
//! detail colors, hunk headers, and continuity markers.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hdiff::preferences::DiffLayout;
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Size,
    style::{Color, Modifier},
};

use crate::harness::{Live, Mode, THEME};

/// architecture.md (Continuity markers): markers take the dimmed separator tone.
#[test]
fn draws_continuity_markers_in_the_dimmed_separator_tone() {
    let rows = (0..20)
        .map(|row| format!("+row_{row}\n"))
        .collect::<String>();
    let patch = format!("--- a/file\n+++ b/file\n@@ -0,0 +1,20 @@\n{rows}");
    let live = Live::sized(&patch, Mode::line(DiffLayout::Unified), Size::new(80, 8));

    let rendered = live.buffer();
    let marker = rendered
        .content()
        .iter()
        .find(|cell| cell.symbol() == "▾")
        .expect("lines continue below");
    assert_eq!(marker.fg, THEME.chrome.dim_separator);
}

/// architecture.md (View model): an alignment peer keeps its change's background, and only
/// a deletion's dimming.
#[test]
fn renders_vertical_alignment_peers_with_their_change_background() {
    let addition_peer = Live::sized(
        "--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        Mode::line(DiffLayout::Vertical),
        Size::new(80, 9),
    )
    .buffer();
    let deletion_peer = Live::sized(
        "--- a/file\n+++ b/file\n@@ -1,2 +1,3 @@\n-old first\n+new first\n+new second\n context\n",
        Mode::line(DiffLayout::Vertical),
        Size::new(80, 8),
    )
    .buffer();

    assert_eq!(
        addition_peer[(78, 5)].bg,
        THEME.rows.change.new.background,
        "alignment peer retains addition background"
    );
    assert!(
        !addition_peer[(78, 5)].modifier.contains(Modifier::DIM),
        "alignment peer does not inherit deletion dimming"
    );
    assert_eq!(
        deletion_peer[(28, 5)].symbol(),
        " ",
        "deletion peer has no marker"
    );
    assert_eq!(
        deletion_peer[(51, 5)].bg,
        THEME.rows.change.old.background,
        "deletion peer reaches the before pane edge"
    );
    assert!(
        deletion_peer[(51, 5)].modifier.contains(Modifier::DIM),
        "deletion peer retains deletion dimming"
    );
}

#[test]
fn renders_stacked_alignment_peers_as_markerless_rows() {
    let live = Live::sized(
        "--- a/file\n+++ b/file\n@@ -1,3 +1,2 @@\n-old first\n-old second\n+new first\n context\n",
        Mode::line(DiffLayout::Stacked),
        Size::new(80, 20),
    );

    let rendered = live.buffer();
    let context_row = (0..20)
        .rev()
        .find(|row| rendered[(30, *row)].symbol() == "c")
        .expect("after context row");
    let alignment_peer_row = context_row
        .checked_sub(1)
        .expect("alignment peer precedes context");

    assert_eq!(
        rendered[(30, alignment_peer_row)].symbol(),
        " ",
        "alignment peer is empty"
    );
    assert_eq!(
        rendered[(78, alignment_peer_row)].bg,
        THEME.rows.change.new.background,
        "alignment peer retains addition background"
    );
    assert!(
        !rendered[(78, alignment_peer_row)]
            .modifier
            .contains(Modifier::DIM),
        "alignment peer does not inherit deletion dimming"
    );
}

/// architecture.md (View model): syntax colors sit on low-contrast change rows whose
/// backgrounds reach the diff pane's edge.
#[test]
fn renders_syntax_and_low_contrast_record_styles() {
    let live = Live::sized(
        "--- a/source.rs\n+++ b/source.rs\n@@ -1,2 +1,2 @@\n context\n-fn removed() { let label = \"value\"; }\n+fn added() { let label = \"value\"; }\n",
        Mode::line(DiffLayout::Unified),
        Size::new(80, 9),
    );

    let rendered = live.buffer();
    let expected_symbols = [
        ((28, 1), " ", "header marker column"),
        ((29, 1), " ", "header marker spacing"),
        ((30, 1), "-", "header content"),
        ((28, 4), " ", "context marker column"),
        ((29, 4), " ", "context marker spacing"),
        ((29, 5), " ", "deletion marker spacing"),
        ((29, 6), " ", "addition marker spacing"),
    ];
    let expected_styles = [
        (
            (30, 4),
            THEME.rows.context_body,
            Color::Reset,
            "context payload",
        ),
        (
            (28, 5),
            THEME.rows.change.old.marker,
            THEME.rows.change.old.background,
            "deletion marker",
        ),
        (
            (28, 6),
            THEME.rows.change.new.marker,
            THEME.rows.change.new.background,
            "addition marker",
        ),
        (
            (30, 5),
            THEME.syntax.keyword,
            THEME.rows.change.old.background,
            "deletion keyword",
        ),
        (
            (30, 6),
            THEME.syntax.keyword,
            THEME.rows.change.new.background,
            "addition keyword",
        ),
        (
            (55, 6),
            THEME.syntax.string,
            THEME.rows.change.new.background,
            "addition string",
        ),
        (
            (78, 5),
            THEME.rows.change.old.body,
            THEME.rows.change.old.background,
            "deletion background reaches the diff pane edge",
        ),
        (
            (78, 6),
            THEME.rows.change.new.body,
            THEME.rows.change.new.background,
            "addition background reaches the diff pane edge",
        ),
    ];

    for ((x, y), expected, name) in expected_symbols {
        assert_eq!(rendered[(x, y)].symbol(), expected, "{name}");
    }
    for ((x, y), expected_foreground, expected_background, name) in expected_styles {
        let cell = &rendered[(x, y)];

        assert_eq!(cell.fg, expected_foreground, "{name} foreground");
        assert_eq!(cell.bg, expected_background, "{name} background");
    }
    assert!(
        rendered[(30, 5)].modifier.contains(Modifier::DIM),
        "deletion syntax inherits row-wide dimming"
    );
}

/// architecture.md (View model, Interaction model): changed spans take undimmed change
/// colors over text one step dimmer, and the selected file's row is one highlight.
#[test]
fn renders_character_detail_with_bright_changes_over_muted_text_and_selects_files_cohesively() {
    let live = Live::sized(
        "--- a/source.txt\n+++ b/source.txt\n@@ -1 +1 @@\n-one\n+ore\n",
        Mode::character(DiffLayout::Unified),
        Size::new(80, 8),
    );

    let rendered = live.buffer();
    let selected_marker = &rendered[(0, 1)];
    let selected_label = &rendered[(2, 1)];
    let selected_row_end = &rendered[(23, 1)];

    assert_eq!(selected_marker.symbol(), "~");
    assert_eq!(selected_marker.fg, selected_label.fg);
    assert_eq!(selected_marker.bg, selected_label.bg);
    assert_eq!(
        selected_row_end.bg, selected_label.bg,
        "the highlight spans the whole row"
    );
    assert_eq!(rendered[(31, 4)].fg, THEME.rows.change.old.marker);
    assert_eq!(rendered[(31, 5)].fg, THEME.rows.change.new.marker);
    assert!(
        !rendered[(31, 4)].modifier.contains(Modifier::DIM),
        "changed deletion span is not dimmed"
    );
    assert!(
        !rendered[(31, 5)].modifier.contains(Modifier::DIM),
        "changed addition span is not dimmed"
    );
    assert_eq!(
        rendered[(30, 4)].fg,
        THEME.rows.change.old.unchanged,
        "unchanged deletion text is one step below the deletion body"
    );
    assert_eq!(
        rendered[(30, 5)].fg,
        THEME.rows.change.new.unchanged,
        "unchanged addition text is one step below the addition body"
    );
}

/// architecture.md (View model): in character mode, unmatched source payloads take their
/// change color with no unchanged characters, and line mode restores their syntax colors.
#[test]
fn toggles_unmatched_payload_colors_in_every_layout() {
    for layout in [
        DiffLayout::Unified,
        DiffLayout::Vertical,
        DiffLayout::Stacked,
    ] {
        assert_unmatched_payload_toggle(layout);
    }
}

fn assert_unmatched_payload_toggle(layout: DiffLayout) {
    let patch = "--- a/first\n+++ b/first\n@@ -1 +1 @@\n-old\n+new\n\
                 --- a/source.rs\n+++ b/source.rs\n@@ -1 +0,0 @@\n-fn removed() {}\n\
                 @@ -2,0 +2 @@\n+fn added() {}\n";
    let mut live = Live::sized(patch, Mode::line(layout), Size::new(160, 30));
    live.send(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let line_rendered = live.buffer();
    let payloads = [
        ("fn removed() {}", THEME.rows.change.old.marker),
        ("fn added() {}", THEME.rows.change.new.marker),
    ];
    for (payload, _) in payloads {
        let cells = unique_payload_cells(&line_rendered, payload);
        assert_eq!(cells[0].fg, THEME.syntax.keyword, "Rust keyword");
    }

    live.press('c');
    for (payload, color) in payloads {
        let character = live.buffer();
        for cell in unique_payload_cells(&character, payload) {
            assert_eq!(cell.fg, color, "{layout:?}: {payload}");
            assert!(!cell.modifier.contains(Modifier::DIM), "{payload}");
        }
    }

    live.press('c');
    assert_eq!(live.buffer(), line_rendered, "line mode restores the frame");
}

fn unique_payload_cells<'a>(buffer: &'a Buffer, payload: &str) -> &'a [Cell] {
    let mut occurrences = buffer.content.windows(payload.len()).filter(|cells| {
        cells
            .iter()
            .map(Cell::symbol)
            .eq(payload.split("").filter(|symbol| !symbol.is_empty()))
    });
    let cells = occurrences.next().expect("full source payload is visible");

    assert!(occurrences.next().is_none(), "source payload appears once");
    cells
}

/// architecture.md (View model): hunk headers are muted cyan-blue; file headers stay
/// neutral.
#[test]
fn renders_hunk_headers_as_muted_cyan_blue() {
    let live = Live::sized(
        "--- a/source.rs\n+++ b/source.rs\n@@ -1 +1 @@\n-old\n+new\n@@ -3 +3 @@\n-before\n+after\n",
        Mode::line(DiffLayout::Unified),
        Size::new(80, 9),
    );

    let rendered = live.buffer();
    for y in [3, 6] {
        let hunk_header = &rendered[(30, y)];

        assert_eq!(hunk_header.symbol(), "@");
        assert_eq!(hunk_header.fg, THEME.rows.hunk_header);
        assert_eq!(hunk_header.bg, Color::Reset);
    }
    assert_eq!(
        rendered[(30, 1)].fg,
        Color::Reset,
        "file header stays neutral"
    );
}
