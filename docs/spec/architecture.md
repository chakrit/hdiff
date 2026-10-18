---
status: accepted
---

# hdiff architecture

This document records the accepted first-release architecture for `hdiff`. Statements under
“Open decisions” remain intentionally unresolved.

[performance.md](performance.md) is authoritative for preparation and performance
optimization. [roadmap.md](roadmap.md) owns delivery status and pending work.
Commit provenance belongs in raw evidence, not these durable documents.

## Product intent

`hdiff` is an interactive terminal diff viewer for understanding a change set quickly. It
should make the structure of a diff—files, hunks, additions, deletions, and context—easy to
navigate without requiring the reader to remember a command-line sequence or leave the
terminal.

The primary reader is a developer reviewing local or generated diff output. The first release
optimizes for a single user at one terminal, fast startup, predictable keyboard interaction,
and faithful rendering of the supplied diff.

## Engineering standard

Every change must make the system more correct, more elegant, and easier to understand. The
best abstraction is also the fast path: clear ownership, direct data flow, idiomatic Rust, and
no accidental work. A benchmark, milestone, or target never justifies a shortcut, a speculative
micro-optimization, or a design that a future reader cannot understand in one pass.

The package requires Rust 1.98.0 and uses the Rust 2024 edition.

Shell script filenames use dashes between words.

## First-release boundary

### In scope

- Read unified diff text from standard input or one patch-file operand.
- Compare POSIX-style file and directory operands, including recursive `-r` mode.
- Parse the stream into files, hunks, and line records while preserving original text.
- Show a navigable file list and a focused diff view.
- Navigate files and linearly move the diff viewport with a small discoverable keymap.
- Scroll vertically without losing the current location.
- Resize cleanly and redraw from retained state.
- Cycle unified, vertical-split, and stacked-split views without silently changing the selected
  mode.
- Provide a pager-first TUI with a muted `? help` hint and a popup listing every shortcut.
- Make user-facing settings adjustable through shortcuts and re-render the affected view
  immediately.
- Exit cleanly on EOF, quit input, and terminal errors.
- Provide useful non-interactive behavior when output is not a terminal.

### Explicitly out of scope

- Editing files or applying patches.
- Git repository operations, staging, commits, or network access, except `--install` writing
  the global setting that registers hdiff as the `git diff` pager.
- A persistent database, daemon, or background service.
- A plugin system or user scripting API.
- Reproducing every feature of an existing pager before the core navigation is reliable.

## Program structure

The program is a pipeline with one directional dependency flow:

```text
input bytes
  -> diff parser
  -> immutable diff document
  -> prepared document
  -> visible layout from interaction state and terminal dimensions
  -> terminal renderer
```

The parser owns interpretation of unified diff syntax. The document owns the loss-preserving
representation of what was received. Preparation owns retained display rows, split pairs,
and style partitions. Interaction state owns selection, scroll position, and view mode.
Visible layout derives pane geometry and visibility from prepared content, interaction
state, and terminal size. The terminal adapter owns raw-mode setup, input events,
drawing, resize events, and restoration on exit. Finite output renders the document
directly without entering the interactive preparation and terminal path.

### Interactive loop

`Tui` is the interactive loop. It holds interaction state and the current prepared
document; it owns neither the terminal nor the input source.

```text
tui.load(document)            replaces the current document
tui.run(&mut terminal, events) draws, then steps each event until Exit
tui.step(&mut terminal, event) event -> Event -> Interaction transition -> draw -> Continue | Exit
```

Output is Ratatui's `Terminal<B: Backend>`. Input is any
`Iterator<Item = io::Result<Event>>`.

| Caller     | Terminal                                    | Events                         | Drives       |
|------------|---------------------------------------------|--------------------------------|--------------|
| `main`     | `CrosstermBackend<Stdout>`, `ratatui::init` | `TtyEvents` (coalesces resize) | `tui.run`    |
| live tests | `TestBackend`                               | one key per call               | `tui.step`   |
| `movement` | `CrosstermBackend<io::Sink>`                | fixed key sequence             | timed `step` |

`main` restores the terminal with `ratatui::restore` after `run` returns.

No layer should inspect another layer’s internal representation. In particular, rendering must
not parse diff text, and input handling must not mutate parsed records in place.

## Core model

The document is the source of truth and retains enough original content to render unknown but
valid diff lines without data loss.

- `DiffDocument`: ordered text sections and file-diff sections.
- `FileView`: a borrowed navigation projection of a file and its surrounding text.
  Leading text appears above the following file; final trailing text appears after
  the last file. Repeated paths remain separate file occurrences.
  Each file occurrence is one navigation page; a document that holds no file is one page of
  its surrounding text, with an empty file list.
- `DiffFile`: file identity, headers, and ordered hunks. Identity (state and path) and
  decoded header paths are derived when the file is parsed; the file tree and syntax
  highlighting read them rather than raw header text.
- File-list paths follow the [file tree](#file-tree) path naming, while rendered diff
  headers preserve their original protocol text.
- `Hunk`: header text containing the old/new ranges, and ordered records.
- `Record`: context, addition, deletion, or unclassified/raw line, with original payload.
- `PageIndex`: the selected page in file-list order; the viewport offset is the reader's
  current cursor location and moves without a separate editing cursor.
- `ViewPreferences`: layout, granularity, semantic strategy, wrapping, context-line count,
  and other session-local display choices. Character presentation follows the
  [view model](#view-model) and [prepared style partition](#lossless-and-safe-rendering).
- `Viewport`: the terminal screen size and the scroll position: the vertical line offset
  and the synchronized horizontal offset.
- `Layout`: retained rendered rows, split pairs, and source addresses.
- Visible layout: derived panes, effective visibility, the help hint, and the keymap popup.

The interaction state is a value that transitions in response to an event: a bound command,
or a session event (interrupt or resize) that applies in every mode, so no mode can
intercept one. A transition returns a new state and a rendering request; it does not reach
into the terminal or document. The selected page, viewport position, and preferences are
authoritative interaction state. Wrapping, row numbers, pane geometry, and rendered rows
are derived. Navigation, which finds the display an event leads to and its bounds, is
independent of the terminal. This makes navigation testable without a real terminal and
keeps invalid positions out of the renderer.

## Input contract

The intended command follows the POSIX `diff` operand model where practical. Two file
operands compare files; two directory operands compare corresponding entries; `-r` enables
recursive directory comparison; and `-` denotes standard input. hdiff adds viewer modes
for one existing patch or diff file and for a unified diff supplied on standard input.
It accepts unified diffs from any producer. `--install` is the sole Git integration: it
backs up the exact global config file Git will write to a sibling file named by appending
`.bak` to its name, then overwrites `pager.diff` through `git config --global`; hdiff never
parses or rewrites Git config text. Every path and command is validated before the first
write.

Zero-byte viewer input exits silently with status 0 before parsing, preparation,
or terminal acquisition. This applies to stdin, an empty patch file, and identical
file operands. Measurement modes retain their
[zero-count reports](performance.md#benchmark-mode).

Preserve text outside file diffs independently of its producer. Retain that text once,
render it in both split panes and once in unified view, and use the same parsing and
rendering path for all producers. Do not introduce `git show` exceptions. Recognized
diff sections remain strictly validated.

Support diffs and their surrounding text through explicit `git show <commit> | hdiff`
invocations. Do not provide general text-only paging; reject nonempty stdin or patch-file
input containing no diff. Comparison output is exempt: it is shown even when it holds only
surrounding text, such as `Binary files … differ`. Automatic `pager.show` registration is
outside this scope.

Surrounding text has document-level ownership. Prepared layouts render each text line
once and share its row index across split panes. Text receives no source syntax or
character-detail spans.

An immediate record-looking continuation after a hunk's declared counts is an error.
Surrounding text does not provide recovery from malformed recognized diff sections.

The input selector reads stdin with zero operands or a lone `-`, reads a patch file with
one other operand, and invokes `diff -u -N [-r] -- <before> <after>` with two, so added
and removed files render as diffs; `-N` is not POSIX, and GNU and Apple `diff` both
support it. More than two operands produce `expected zero, one, or two operands`. `-r`
is recognized anywhere in the input arguments and requires two operands; otherwise it
produces `-r requires two comparison operands`. A `-` comparison operand is passed to `diff`,
which reads hdiff's standard input. Other dash-leading arguments are paths.

Structural parsing is strict. Valid unknown lines are preserved where they can be attached
unambiguously, but malformed or truncated input is rejected with a contextual non-zero error
before the interactive TUI starts. The first release does not model partial documents in the
TUI.

Git extended headers beginning with `diff --git` are ordered per-file metadata and render before
that file's unified headers. ANSI control sequences are ignored for structural recognition while
the original source bytes remain available for safe rendering.

Metadata-only Git sections, including binary changes, pure renames, and mode-only changes, are
valid review content and render without requiring unified headers or hunks.

## Initial CLI surface

The first command should be intentionally small:

```text
hdiff [OPTIONS] [FILE ...]
```

Options describe representations or input policy, not interactive procedures. Runtime display
changes are session-local. An rc file may provide defaults, but hdiff never writes those
changes back during a session.

`hdiff --install` registers the current executable as the user-level pager for `git diff`. Git
passes its complete unified diff to hdiff on standard input, preserving the entire file list in
one interactive session. `core.pager` remains unchanged. After successful writes, `--install`
prints a shell-safe `cp` command when it created a backup and the `git config --global pager.diff`
command it applied.

`hdiff --help` prints the command usage, operands, and options without reading diff input.
`hdiff --version` prints the Cargo package version followed by the short source commit
hash in parentheses. Both commands exit successfully after printing their output.

### Current argument selection

`--help` and `--version` select their commands only when supplied alone. `--bench <driver>`
selects measurement only as the first argument; the remaining zero, one, or two arguments
select input as described above. Their output contracts are in
[`performance.md`](performance.md).

`--install` is recognized anywhere in the argument list and must be the sole argument.
Any accompanying argument, including another option, produces
`--install cannot be combined with diff input operands` before input is read.

Other argument combinations fall through to operand selection, where `-r` is the only
option. hdiff has no general unknown-option rejection or `--` option terminator;
unrecognized arguments are treated as paths, subject to the operand-count limit.

### Error output

Errors write their message to standard error with every line prefixed `hdiff: ` and exit
with status 2, `diff`'s status for trouble. Unreadable files are named in the message;
a failed comparison reports `diff`'s own standard error.

## Interaction model

The primary interaction is pager-like rather than a dashboard: the diff occupies the main
viewport, and a muted `? help` hint points to the keymap popup. The hint and popup are a
navigational aid, not a second control plane; every setting has one keyboard path, and changing it produces a
new interaction state and immediate redraw.

The compact surface has a file list on the left, a diff view on the right, and one
vertical separator padded by one space on each side, with no pane borders or labels.

The left file list is the primary navigation pane. `e` shows and hides it; it starts shown.
As width decreases it collapses before the diff view. The help hint sits on the bottom row of
the file list; while the file list is hidden, it floats in a small bordered box over the
bottom-left corner of the diff.
If the terminal is too narrow or too short for the minimum layout, the only rendered content
is `screen too narrow` or `screen too short`. The minimum layout shows one content line in
each diff pane.
The selected layout is never silently replaced by another layout.

`?` opens a centered, bordered popup over the diff that lists every shortcut, grouped under
headings. While it is open, `?`, `Esc`, and `q` close it, `q` without exiting, `Ctrl-C`
exits, and every other key does nothing; closing it restores the view. The pager-first model
and live shortcut-driven settings are settled product requirements.

Unselected file names use a medium-muted neutral tone, one level brighter than the
separator and the help hint; their markers take their state's color, as the
[file tree](#file-tree) defines. The selected file row applies one shared muted
foreground/background style to its whole row and has no selection glyph.

### File tree

Delivery is recorded in the [roadmap](roadmap.md#completed-delivery).

The file list shows files grouped under directory headings, derived from file paths only;
it has no file-system navigation and no collapse or expansion.

- Top-level files come first, then every other file; each part keeps input order. File
  rotation and the file list follow this order.
- Directory rows are headings only. Selection and `Tab` / `Shift-Tab` rotation move
  between files and never stop on a heading.
- Rows have no indentation. A directory heading shows the directory's full path with a
  trailing `/`, such as `src/terminal/`. A directory whose only content is one
  subdirectory shares that subdirectory's heading. A heading appears wherever a file's
  directory differs from the previous file's.
- Directory headings are bold in the separator tone.
- Each file row is one state marker, one space, and the file's basename:

  | Marker | State                                      |
  |--------|--------------------------------------------|
  | `~`    | modified, including mode-only changes      |
  | `+`    | added or copied; a copy shows the new name |
  | `-`    | deleted                                    |
  | `*`    | renamed; the row shows the new name        |
  | `?`    | ambiguous or erroneous entry               |

- Each marker takes a light per-state color: modified amber, added green, deleted red,
  renamed blue, and `?` in the name's neutral tone. File names stay neutral.
- A `rename to` or `copy to` line names a file. Otherwise the name is the longest run of
  trailing `/`-separated components both header paths share, or the whole new path
  when they share none. In a Git section with a `/dev/null` header, both paths come
  from the `diff --git` line, split at the one space whose halves share such a run;
  outside Git, the other header's path is shown as written.
- Git metadata decides a file's state. Without it, a `/dev/null` old header means added
  and a `/dev/null` new header means deleted; differing header paths never mean a rename.
- A `?` entry has no directory; it is a top-level file labelled with its raw header
  line.
- Nerd Font glyphs are not used.

## View model

View choices are orthogonal rather than one growing mode enum:

- `DiffLayout`: unified, vertical split, or stacked split;
- `DiffGranularity`: line-based or character-based;
- `DiffSemantics`: textual or Tree-sitter semantic strategy;
- navigation-pane state: file list visibility and active pane;
- display preferences: wrapping, context-line count, highlighting, and color capability.

The intended `EffectiveView` derives the renderable combination from these preferences, document
capabilities, language support, and terminal dimensions. Tree-sitter semantic diff is used when
the language and parser are available, with textual fallback otherwise. The user-selected
layout is preserved through resizes; hdiff does not automatically switch between layouts. `v`
cycles unified, vertical split, and stacked split in that order, preserving the
selected file and viewport.

Semantic diff strategies remain pending delivery. The current viewer uses textual
comparisons and the `low` palette.

Vertical split renders each changed block as aligned before/after pairs, adding a markerless
alignment peer with the corresponding dim addition or deletion background on the shorter side.
Each alignment peer uses the background and dim modifier of its own pane: before peers use
deletion styling and after peers use addition styling, regardless of the counterpart row.
Alignment peers are layout-only counterparts for unmatched change rows, not source context lines.
Shared context and structural rows render in both panes. A more dimmed separator divides the before
and after panes; each pane retains the marker column and payload spacer used by unified rendering.
Stacked split derives the same pairs and renders the same alignment peers, so both panes retain
equal content height while shared hunk headers and context retain orientation.

Wrapping is session-local and toggled independently from layout. Wrapping starts disabled.
When wrapping is disabled, horizontal movement uses one synchronized offset for
side-by-side panes. When wrapping is enabled:

- payloads break at grapheme-cluster boundaries; a cluster that does not fit the remaining
  cells starts the next line;
- continuation lines keep the row's background and leave the marker column and payload
  spacer blank;
- both side-by-side panes wrap at the narrower pane's payload width; a side-by-side row
  takes the height of its taller side, and the shorter side pads with blank lines in its
  own row style;
- the maximum horizontal offset is zero;
- toggling wrapping keeps the row on the diff pane's middle line on that line.

Showing or hiding the file list changes the diff pane width and keeps the row on the diff
pane's middle line on that line.

### Continuity markers

Each diff pane is framed by one reserved row above and below its content and one reserved
column on its left and right. The frame shows where content continues past the visible
content area and never holds diff content:

- the top row shows `▴` at its center while lines exist above the first visible line;
- the bottom row shows `▾` at its center while lines exist below the last visible line;
- the left column shows one `◂` at its middle while any visible row has cells left of the
  horizontal offset;
- the right column shows one `▸` at its middle while any visible row continues past the
  content area's right edge;
- markers use the dimmed separator tone of the before/after divider on the default
  background; row backgrounds stop at the frame;
- the frame is reserved even when it shows no marker, and stacked panes each have their
  own frame.

The file list has the same reserved row above and below its rows, with no side columns:
`▴` shows while tree rows exist above the first listed row and `▾` while tree rows exist
below the last.

Resizing keeps the record on the diff pane's top line at the top, on the same line within
that record where it still exists. Context visibility is
session-local: hdiff initially retains three context records on either side of each changed
block within a hunk, `+` increases that bound, and `-` decreases it to zero. Context changes
recompute the visible layout without mutating the document or selected file; the row on
the diff pane's middle line stays on that line.

The `low` palette is the only palette and the locked preferred display: additions are strongest; deletion text and
syntax spans are muted by a row-wide dim modifier over a dim red background; context is quieter
neutral text. Every rendered row reserves a marker
column and one payload spacer: addition and deletion markers occupy that column, while metadata,
hunks, and context leave it blank so all payloads align. The separator remains neutral, and each
change background begins after its right-hand gutter. The palette applies consistently in
unified and side-by-side layouts. `DiffGranularity` controls whether differences are
represented as lines or characters.

Character detail does not use reverse video. It first preserves matching identifier/word and
delimiter tokens, then uses grapheme spans only for similar replacement words; unrelated words
remain whole-token changes rather than matching coincidental letters. It coalesces an equal island
of at most three graphemes bracketed by changed spans, so incidental short matches do not fragment
a change. Its changed spans use undimmed green on addition rows and undimmed red on deletion
rows, reusing the line-change marker colors; unchanged text on rows with detail spans is one
step dimmer than the row's line-mode text.
Rows without detail spans retain syntax styling.

In character mode, unmatched additions and deletions use their green or red
detail foreground across the entire payload in every layout, overriding syntax colors.
The payload excludes its diff marker and line ending; empty changed lines retain their
marker and row background, and alignment peers retain their markerless pane styling.
Paired lines retain token/grapheme comparison, and returning to line mode restores syntax
presentation from the same prepared data.

Hunk headers beginning with `@@` are muted cyan-blue structural lines. Git metadata and `---`/`+++`
file headers retain neutral metadata styling.

Addition and deletion backgrounds extend through the right edge of the diff pane's content area, including
cells after the final source character.

### Keyboard bindings

The current keymap is pager-like:

| Keys                      | Action                                        |
| ------------------------- | --------------------------------------------- |
| `j` / Down, `k` / Up      | Move the viewport down or up                  |
| `h`, `l`                  | Move left or right by one terminal cell       |
| `0`, `^`                  | Return to the left edge                       |
| `$`                       | Move to the furthest useful horizontal offset |
| `Ctrl-D`, `Ctrl-U`        | Move down or up by half a page                |
| `g`, `G`                  | Move to the top or bottom                     |
| `Tab`, `Shift-Tab`        | Rotate to the next or previous file           |
| `v`                       | Cycle unified, vertical, and stacked layouts  |
| `c`                       | Toggle line and character detail              |
| `w`                       | Toggle wrapping                               |
| `+`, `-`                  | Increase or decrease visible context          |
| `e`                       | Show or hide the file list                    |
| `?`                       | Open the keymap popup                         |
| `?`, `Esc`, `q` (popup)   | Close the keymap popup                        |
| `q`, `Ctrl-C`             | Exit                                          |

Horizontal movement uses one bounded offset across content panes in every layout;
the file list stays fixed. File rotation wraps around and moves the viewport to the
selected file's top. There is no separate diff editing cursor; movement changes the
viewport. Shifted shortcuts perform meaningful inverse actions; ambiguous keys follow
Vim semantics.
Hunk jumps remain absent until a visible hunk focus makes their destination legible.
Half-page movement is smooth.

The keymap popup is drawn from the key bindings themselves and lists every shortcut in the
table above under four headings: Scroll, Files, View, and Keymap and exit. Scroll shows
`h`, `j`, `k`, and `l` as a diamond. It never advertises a deferred shortcut. The groups
sit in two columns when the screen is wide enough and in one column otherwise. Headings
take the hunk-header tone in bold, keys the syntax function color, actions the file-name
tone, and the border the separator tone.

## Technology direction

Rust is the implementation language. `ratatui` owns layout and rendering; CROSSTERM 0.29 with
`use-dev-tty` owns terminal I/O, events, resize, raw mode, alternate-screen handling, and
lifecycle operations.
The backend must support event reads from an explicit controlling-terminal handle when stdin
contains diff data. Syntax highlighting is assigned to Tree-sitter through an hdiff-owned
boundary; the parser set and performance rationale are recorded in
`docs/vendor/syntax-highlighting.md`.
Syntax highlighting projects each hunk's old and new
records into separate bounded virtual source buffers, maps returned spans back to sanitized
record payloads, and leaves unsupported languages and highlighting failures as plain text.
Projection records are ordered by virtual source range, so span mapping visits only records that
overlap the returned source span.
Safe rendering is the single source of display rows: each rendered record row retains its kind,
hunk and record address, and payload range. Layout derives geometry and navigation offsets from
those rows without syntax state. Preparation computes syntax and character-detail spans before
the terminal loop begins, then the terminal renders only the prepared active viewport. Additions
use the new path's language and deletions use the old path's language; context records remain
textual when the paths select different languages. Headers, metadata, raw records, and diff
markers remain textual.

When both paths select the same language, only the new projection supplies context
styling. Both projections retain context text for parsing. Overlapping syntax captures
use the shortest covering capture; later capture encounter order breaks equal-length
ties. Capture length is measured before row clipping.

The first test boundary is terminal-independent: parser fixtures, state-transition tests, layout
snapshots, and renderer output tests. Live interaction tests run the `Tui` in-process in a
`live` test target under plain `cargo test`: each test loads a fixture document into a `Tui`,
drives it with `tui.step` one key at a time against a 120-by-36 `Terminal<TestBackend>`, and
asserts on the backend buffer after each step. A `macro_rules!` matrix runs scenarios across
layouts and detail modes. Tests never assert timing and never start the `hdiff` executable.

## Input and terminal boundary

The first release reads the complete input before entering raw mode. Input selection distinguishes
standard input, one existing patch file, and two comparison operands; combining piped standard
input with file arguments must produce an explicit usage error. Standard input is piped
when it is a pipe or socket; file arguments are accepted with piped standard input only
when a `-` operand reads it. Interactive mode requires a controlling
terminal independent of the data source, so `stdin` supplies diff bytes while the controlling
terminal supplies events and output. If no controlling terminal is available, hdiff produces
finite non-interactive output. hdiff is the pager and must not launch or depend on an external
pager. The output contract decides whether interactive mode is attempted before a controlling
terminal is opened. The terminal backend must read events from an explicit controlling-terminal
handle rather than rebinding standard input.

Interactive mode is attempted when stdout is a terminal. hdiff then opens `/dev/tty`
and draws there; when it cannot be opened, hdiff writes finite output to stdout instead.
Setup errors after the controlling terminal opens propagate.

## Interaction and layout state

Interaction state is split into selection, viewport, preferences, and derived layout.
Resize is an event; visible layout is recomputed from retained content and the latest
dimensions, following the [interaction model](#interaction-model). Zero-sized layouts
remain valid values rather than arithmetic errors.

### Viewport coordinates

The viewport's vertical offset indexes the screen lines of the visible rows of the active
layout at the current context count; a row is one line unless wrapping breaks it into
several. The frame shows the lines from that offset, and scroll limits count lines. The
visible rows, their lines, and their widest row are derived once per selected file,
layout, context count, and wrap width, and not on movement. Unified rows are lines and
side-by-side rows are pairs, each indexed in its own space; an anchored row is named by
the line it starts with. One pane geometry, derived from the screen size and view
preferences, serves both navigation bounds and drawing, and a scroll extent carries the
wrap it was built with.

### Eager prepared rendering

The preparation boundary, ownership, observation policy, and performance requirements
are defined in [performance.md](performance.md#preparation-boundary).

## Lossless and safe rendering

Layout or span construction must make rendering duplicate source text impossible
by construction.

Preparation partitions each retained row's payload into adjacent styled portions.
The partition and its source row are accessed together through the prepared file;
rendering consumes the source once, and annotations select styles only. Syntax and
character-detail presentation share this boundary. Rows with character-detail spans
use detail styling with plain gaps; rows without them retain syntax styling.

Each record retains exact source bytes, sanitized text, and classification. One scanner
recognizes control sequences for both sanitizing and locating a record's marker.
Prepared partitions own display styling. Parsing uses ANSI-free text; rendering strips
input control sequences, including SGR colors, and applies hdiff's own styling.
It never replays arbitrary source CSI, OSC, DCS, or control bytes into the terminal.
Interactive rows expand each tab to the next 2-column tab stop, counted from the start
of the payload.
Interactive rows are segmented into grapheme clusters once per row, across style spans;
a cluster takes the style of its first byte. Style boundaries never split or drop a
cluster. Grapheme clusters come from `unicode-segmentation`; every cell width, including
tab stops and row widths, comes from Ratatui's `CellWidth`.
A wide grapheme cut by the left pane edge renders its visible cells blank.
Text renders in logical order; hdiff does not reorder bidirectional text.
Ratatui's `layout-cache` feature is enabled.
The authoritative document is never truncated; only decoding, highlighting, wrapping,
and visible-row work may be bounded.

The terminal-independent test boundary must cover invalid UTF-8, CRLF, embedded SGR/OSC/CSI,
combining marks, double-width graphemes, tabs, zero- and one-row terminals, long lines,
resizing while focused on a wrapped record, resize storms, read errors after partial input,
draw errors, Ctrl-C, and broken pipes.

## Failure and safety posture

- Never modify the input files.
- Preserve unknown diff lines rather than silently dropping them.
- Reject malformed or truncated input with context before entering the interactive TUI.
- Provide a rich built-in semantic palette with 256-color support and truecolor/RGB when the
  terminal supports it, falling back by capability. The color depth is decided once per
  session, and one `Theme` per depth owns every syntax, row, and chrome color, so
  user-configurable themes can replace that built-in palette later.

## Terminal lifecycle

Interactive mode is decided from the output contract before a controlling terminal is opened.
Terminal setup is staged and idempotent, with ownership unwound in reverse order. Quit and
Ctrl-C restore terminal state cleanly. Every return path after acquisition restores raw
mode, the alternate screen, cursor state, and controlling-terminal attributes.
Resize events coalesce to the latest dimensions and
trigger a redraw. Draw and read failures report diagnostics after cleanup. Broken pipes are
quiet success only for finite non-interactive output. Suspend/resume is supported only through
a safe public backend transaction; otherwise it remains deferred. A cleanup guard or panic hook
is best effort; SIGKILL and abort-style termination cannot be restored.

## Open decisions for review

- What exact option names and rc-file location should the input and display contract use?
- Which Tree-sitter semantic-diff algorithm should implement the semantic strategy?

Prior-art review of Delta, Tig, and Difftastic is recorded in
`docs/vendor/terminal-diff-viewers.md`.
