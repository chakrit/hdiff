---
status: accepted
---

# hdiff initial architecture

This document records the accepted first-release architecture for `hdiff`. Statements under
“Open decisions” remain intentionally unresolved.

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

Work toward a long-term result through successive proper refactors. Each change begins by
understanding the actual problem, then selects the cleanest structural solution; it does not
force a local patch merely to advance a number. If a clean improvement cannot be demonstrated,
leave the code alone.

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
- Provide a pager-first TUI with muted, context-sensitive shortcut hints at the bottom of the
  file-list column.
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

## Proposed shape

The program is a pipeline with one directional dependency flow:

```text
input bytes
  -> diff parser
  -> immutable diff document
  -> interaction state
  -> layout and view model
  -> terminal renderer
```

The parser owns interpretation of unified diff syntax. The document owns the loss-preserving
representation of what was received. Interaction state owns selection, scroll position, and
view mode. Layout converts document plus state plus terminal size into renderable rows. The
terminal adapter owns raw-mode setup, input events, drawing, resize events, and restoration on
exit.

No layer should inspect another layer’s internal representation. In particular, rendering must
not parse diff text, and input handling must not mutate parsed records in place.

## Core model

The document is the source of truth and retains enough original content to render unknown but
valid diff lines without data loss.

- `DiffDocument`: ordered files and document-level metadata.
- `DiffFile`: file identity, headers, and ordered hunks.
- File-list labels use the new header path when the old header path is `/dev/null`; labels
  omit Git's `a/` and `b/` comparison-root prefixes, while rendered diff headers preserve
  their original protocol text.
- `Hunk`: old/new ranges, header text, and ordered records.
- `Record`: context, addition, deletion, or unclassified/raw line, with original payload.
- `Selection`: the active file-list selection; the viewport offset is the reader's current
  cursor location and moves without a separate editing cursor.
- `ViewPreferences`: layout, granularity, semantic strategy, wrapping, context-line count,
  and other session-local display choices. Character granularity compares corresponding
  changed-line pairs as identifier/word and delimiter tokens, then compares only sufficiently
  similar replacement words by grapheme cluster; unrelated replacements remain whole-token spans.
  Unchanged text retains line-level presentation.
- `Viewport`: terminal width, height, vertical position, and synchronized horizontal offset.
- `Layout`: derived panes, rows, effective visibility, and footer hints.

The interaction state is a value that transitions in response to an input event. A transition
returns a new state and a rendering request; it does not reach into the terminal or document.
Selection, viewport position, and preferences are authoritative interaction state. Wrapping,
row numbers, pane geometry, and rendered rows are derived. This makes navigation testable
without a real terminal and keeps invalid positions out of the renderer.

## User-facing feature sequence

The implementation should land in slices that each leave a usable surface:

1. Parse valid unified diffs and render a static, faithful unified diff.
2. Add terminal lifecycle handling, quit, vertical scrolling, and resize redraw.
3. Add Tree-sitter syntax highlighting with textual fallback.
4. Add asymmetric low-contrast addition and deletion styling before the compact layout.
5. Add a compact two-column surface: a Tab-switchable file list with muted bottom hints on the
   left, a diff view on the right, and one vertical separator padded by one space on each side,
   with no pane borders or labels.
6. Add file navigation and the interactive file list with synchronized selection.
7. Add vertical and stacked split rendering, character detail, and synchronized horizontal
   scrolling.
8. Add implicit Tree-sitter semantic strategies with textual fallback.
9. Refine highlighting coverage and additional navigation after the core interaction is stable.

## Input contract

The command follows the POSIX `diff` operand model where practical. Two file operands compare
files; two directory operands compare corresponding entries; `-r` enables recursive directory
comparison; and `-` denotes standard input. hdiff adds viewer modes for one existing patch or
diff file and for a unified diff supplied on standard input. It accepts unified diffs from any
producer. `--install` is the sole Git integration: it backs up the exact global config file Git
will write to a sibling `.bak` file, then overwrites `pager.diff` through `git config --global`;
hdiff never parses or rewrites Git config text.

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

## Interaction model

The primary interaction is pager-like rather than a dashboard: the diff occupies the main
viewport, and the file-list column ends with muted shortcut hints. The hints are a navigational
aid, not a second control plane; every setting has one keyboard path, and changing it produces a
new interaction state and immediate redraw.

The left file list is the primary navigation pane. As width decreases it collapses before the
diff view; a compact footer hint remains when possible. If the terminal is too narrow or too
short for the minimum layout, the only rendered content is `screen too narrow` or
`screen too short`. The selected layout is never silently replaced by another layout.

The file-list hints expose shortcuts relevant to the current context. A help view can expose the
complete keymap when the compact hints cannot fit. The pager-first model and live
shortcut-driven settings are settled product requirements.

Unselected file-list labels use a medium-muted neutral tone, one level brighter than the separator
and footer hints. The selected file remains visually distinct through the reversed selection style.

## View model

View choices are orthogonal rather than one growing mode enum:

- `DiffLayout`: unified, vertical split, or stacked split;
- `DiffGranularity`: line-based or character-based;
- `DiffSemantics`: textual or Tree-sitter semantic strategy;
- navigation-pane state: file list visibility and active pane;
- display preferences: wrapping, context-line count, diff contrast, highlighting, and color
  capability.

`EffectiveView` derives the renderable combination from these preferences, document
capabilities, language support, and terminal dimensions. Tree-sitter semantic diff is used when
the language and parser are available, with textual fallback otherwise. The user-selected
layout is preserved through resizes; hdiff does not automatically switch between layouts. `v`
cycles unified, vertical split, and stacked split in that order.

Vertical split renders each changed block as aligned before/after pairs, adding a markerless
alignment peer with the corresponding dim addition or deletion background on the shorter side.
Each alignment peer uses the background and dim modifier of its own pane: before peers use
deletion styling and after peers use addition styling, regardless of the counterpart row.
Alignment peers are layout-only counterparts for unmatched change rows, not source context lines.
Shared context and structural rows render in both panes. A more dimmed separator divides the before
and after panes; each pane retains the marker column and payload spacer used by unified rendering.
Stacked split derives the same pairs and renders the same alignment peers, so both panes retain
equal content height while shared hunk headers and context retain orientation.

Wrapping is session-local and toggled independently from layout. When wrapping is disabled,
horizontal movement uses one synchronized offset for side-by-side panes. Context visibility is
session-local: hdiff initially retains three context records on either side of each changed
block within a hunk, `+` increases that bound, and `-` decreases it to zero. Context changes
recompute the visible layout without mutating the document, selected file, or viewport.

Diff contrast is a session-local preference with four intensity levels, independent of
line-versus-character diff granularity:

1. `none`: no diff-specific contrast; render additions, deletions, and context with the base
   palette;
2. `low`: the preferred default, using dim red deletion background and muted deletion text,
   quiet neutral context, and a restrained green addition background with a brighter plus marker;
3. `high`: stronger red/green emphasis while retaining readable surrounding context;
4. `max`: the strongest red/green highlighting, including full-line emphasis where the active
   layout supports it.

The `low` palette is the locked preferred display: additions are strongest; deletion text and
syntax spans are muted by a row-wide dim modifier over a dim red background; context is quieter
neutral text. Every rendered row reserves a marker
column and one payload spacer: addition and deletion markers occupy that column, while metadata,
hunks, and context leave it blank so all payloads align. The separator remains neutral, and each
change background begins after its right-hand gutter. Contrast changes never alter the parsed
document and apply consistently in unified and side-by-side layouts. `DiffGranularity` controls
whether differences are represented as lines or characters; `DiffContrast` controls only their
visual intensity and never changes that representation.

Character detail does not use reverse video. It first preserves matching identifier/word and
delimiter tokens, then uses grapheme spans only for similar replacement words; unrelated words
remain whole-token changes rather than matching coincidental letters. It coalesces an equal island
of at most three graphemes bracketed by changed spans, so incidental short matches do not fragment
a change. Its changed spans use dim green on addition rows and dim red on deletion rows, reusing
the line-change palette while leaving unchanged text plain. The selected file row applies one
shared muted foreground/background style to both its compact `·` marker and file label.

Hunk headers beginning with `@@` are muted cyan-blue structural lines. Git metadata and `---`/`+++`
file headers retain neutral metadata styling.

Addition and deletion backgrounds extend through the right edge of the active diff pane, including
cells after the final source character.

The initial keymap is pager-like: `j/k` vertical movement, `h/l` horizontal movement,
`Ctrl-D/Ctrl-U` smooth half-page movement, `g/G` top/bottom, `Tab` file rotation with wraparound,
`w` wrapping, `c` line/character detail, `v` cycles unified/vertical split/stacked split,
`+/-` context lines, `q` quit, and `:` command entry. Help/footer text exposes active
bindings. File rotation selects the next file and moves the viewport to its top. There is no
separate diff editing cursor; movement changes the viewport.

A dedicated shortcut-audit slice makes shifted shortcuts perform meaningful inverse actions.
`Shift-Tab` rotates to the previous file, the inverse of `Tab`; when a key's meaning is
ambiguous, follow Vim semantics.

The left file-list pane will show shortcut hints at its bottom.

The file-list footer is a compact multi-row reference for every currently implemented key: vertical
movement, page movement, top/bottom, layout cycling, file rotation, and exit. It uses only the
available footer rows and never advertises a deferred shortcut.

The footer places `h`, `j`, `k`, and `l` in a centered diamond followed by `movement`. Page-up
and page-down remain separate rows. The remaining current shortcuts use compact paired keys and
middots: `g·G` top/bottom, `v` cycle layout, `(⇧)Tab` next/previous file, and `q` exit.

## Technology direction

Rust is the implementation language. `ratatui` owns layout and rendering; CROSSTERM 0.29 with
`use-dev-tty` owns terminal I/O, events, resize, raw mode, alternate-screen handling, and
lifecycle operations.
The backend must support event reads from an explicit controlling-terminal handle when stdin
contains diff data. Syntax highlighting is assigned to Tree-sitter through an hdiff-owned
boundary; the parser set and performance rationale are recorded in
`docs/vendor/syntax-highlighting.md`.
Syntax highlighting precedes further navigation work. It projects each hunk's old and new
records into separate bounded virtual source buffers, maps returned spans back to sanitized
record payloads, and leaves unsupported languages and highlighting failures as plain text.
Projection records are ordered by virtual source range, so span mapping visits only records that
overlap the returned source span.
Safe rendering is the single source of display rows: each rendered record row retains its hunk
and record address plus its payload range. Layout derives geometry and navigation offsets from
those rows without syntax state. Preparation computes syntax and character-detail spans before
the terminal loop begins, then the terminal renders only the prepared active viewport. Additions
use the new path's language and deletions use the old path's language; context records remain
textual when the paths select different languages. Headers, metadata, raw records, and diff
markers remain textual.

The first test boundary is terminal-independent: parser fixtures, state-transition tests, layout
snapshots, and renderer output tests. End-to-end terminal checks run hdiff in fixed-size tmux
sessions, drive pager inputs with `send-keys`, capture ANSI-styled panes, and lock those captures
with SMOKE. The tmux script owns and removes its uniquely named session on every exit path.

## Input and terminal boundary

The first release reads the complete input before entering raw mode. Input selection distinguishes
standard input, one existing patch file, and two comparison operands; combining piped standard
input with file arguments is an explicit usage error. Interactive mode requires a controlling
terminal independent of the data source, so `stdin` supplies diff bytes while the controlling
terminal supplies events and output. If no controlling terminal is available, hdiff produces
finite non-interactive output. hdiff is the pager and must not launch or depend on an external
pager. The output contract decides whether interactive mode is attempted before a controlling
terminal is opened. The terminal backend must read events from an explicit controlling-terminal
handle rather than rebinding standard input.

## Interaction and layout state

Interaction state is split into selection, viewport, preferences, and derived layout. Resize is
an event; layout is recomputed from retained content and the latest dimensions, with resize
bursts coalesced. The selected layout is preserved through resize. The file-list pane collapses
before the diff view, and below minimum width or height the layout renders only the corresponding
screen-size message. Zero-sized layouts remain valid values rather than arithmetic errors.

### Eager prepared rendering

Preparation is one optimized single-threaded pass before the terminal loop begins. It produces
layout-neutral rows, split pairs, syntax spans, and character-detail spans for every file. The
terminal loop owns only interaction, terminal lifecycle, redraw decisions, and rendering of the
prepared active viewport.

The terminal loop has no parser, cache, readiness, loading, or first-file state. It receives fully
prepared data on its first draw and on every redraw. No async runtime, executor, channel, or task
dependency is part of this boundary.

## Lossless and safe rendering

Each record retains exact source bytes, decoded text, classification, and sanitized style
spans. Parsing uses ANSI-free text. Rendering interprets supported SGR styling only and never
replays arbitrary source CSI, OSC, DCS, or control bytes into the terminal. The authoritative
document is never truncated; only decoding, highlighting, wrapping, and visible-row work may
be bounded.

Terminal setup is staged and idempotent. After terminal acquisition, every return path restores
raw mode, the alternate screen, cursor state, and controlling-terminal attributes. Broken pipes
are quiet success only for finite non-interactive output.

The terminal-independent test boundary must cover invalid UTF-8, CRLF, embedded SGR/OSC/CSI,
combining marks, double-width graphemes, tabs, zero- and one-row terminals, long lines,
resizing while focused on a wrapped record, resize storms, read errors after partial input,
draw errors, Ctrl-C, and broken pipes.

## Failure and safety posture

- Never modify the input files.
- Preserve unknown diff lines rather than silently dropping them.
- Restore terminal state on every exit path after raw mode is entered.
- Treat a resize as a new layout calculation over the same document and interaction state.
- Reject malformed or truncated input with context before entering the interactive TUI.
- Make non-terminal output finite and script-friendly rather than entering an interactive loop.
- Interpret only supported SGR styling. Preserve source bytes separately, neutralize OSC/CSI/DCS
  and other unsupported control sequences, and never replay arbitrary input escapes.
- Provide a rich built-in semantic palette with 256-color support and truecolor/RGB when the
  terminal supports it, falling back by capability. User-configurable themes are later scope.

## Terminal lifecycle

Interactive mode is decided from the output contract before a controlling terminal is opened.
Terminal setup is staged and idempotent, with ownership unwound in reverse order. Quit and
Ctrl-C restore terminal state cleanly. Resize events coalesce to the latest dimensions and
trigger a redraw. Draw and read failures report diagnostics after cleanup. Broken pipes are
quiet success only for finite non-interactive output. Suspend/resume is supported only through
a safe public backend transaction; otherwise it remains deferred. A cleanup guard or panic hook
is best effort; SIGKILL and abort-style termination cannot be restored.

## Open decisions for review

- What exact option names and rc-file location should the input and display contract use?
- Which character-diff and Tree-sitter semantic-diff algorithms should implement the strategy
  boundaries?

## Next design work

Prior-art review of Delta, Tig, and Difftastic is recorded in
`docs/vendor/terminal-diff-viewers.md`. Terminal lifecycle is complete. Tree-sitter syntax
highlighting is complete. Low-contrast addition/deletion styling and compact layout are
implemented and await their terminal human checks. Navigation, side-by-side, character, and
semantic strategies follow in the feature sequence above.
