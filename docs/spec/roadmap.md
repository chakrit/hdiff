---
status: accepted
---

# hdiff roadmap

Track project tasks in this repository's Markdown. This roadmap owns the backlog.
[Architecture](architecture.md) owns product and engineering contracts;
[performance](performance.md) owns preparation and measurement contracts.

## Completed delivery

- Strict, loss-preserving unified-diff parsing, safe finite output, quiet broken-pipe
  handling, Git metadata-only sections, and ANSI-colored input.
- Ratatui rendering with CROSSTERM terminal lifecycle, compact file-list and diff panes,
  low-contrast styling, full-width change backgrounds, and muted shortcut hints.
- Vertical movement, file rotation and its shifted inverse, resize redraw, unified /
  vertical / stacked layout cycling, aligned change rows, character detail, adjustable
  context, and synchronized bounded horizontal movement.
- Tree-sitter syntax highlighting with textual fallback and eager preparation for the
  complete document; preparation measurement, profiling, and reproducible comparisons.
  Raw optimization results and provenance are in `.ace/bench/kubernetes/`.
- User-level Git diff-pager installation with configuration backup, `--help`, and
  `--version`.
- Silent zero-byte viewer input and producer-independent surrounding text, including
  explicit `git show` piping; [Git command compatibility](../vendor/git-command-output.md)
  records the completed compatibility assessment and its supported/deferred findings.
  No additional pager registration is recommended.
- Source-preserving style partitions and full-payload character detail for unmatched
  additions/deletions in every layout, as specified in
  [lossless rendering](architecture.md#lossless-and-safe-rendering) and the
  [view model](architecture.md#view-model).

Automated and human terminal checks passed for the delivered interaction and rendering
slices. The terminal smoke suite captures all three layouts and Tab-selected files using
one fixed-size tmux session and a mixed-language fixture; SMOKE records observable drift.
The latest unmatched-line fix passed 95 unit and 18 integration tests, formatting,
Clippy across all targets and features, and diff hygiene, with terminal inspection of
all layouts, Unicode, alignment peers, backgrounds, and horizontal clipping.
Its detailed evidence is in `.ace/unmatched-*.txt`.
Historical delivery plans and verification records are preserved in
`.ace/spec-cleanup-roadmap-before.md`.

## Next: test suite and benchmark drivers

Precedes the movement work. Contracts are in
[architecture](architecture.md#interactive-loop) and
[performance](performance.md#benchmark-drivers).

- [x] Extract `Tui` with `load`, `run`, and `step` over Ratatui's `Terminal<B>` and an
  event iterator; move terminal setup to `ratatui::init` / `ratatui::restore`, confirming
  they cover cursor hiding and staged cleanup; add `TtyEvents` with resize coalescing.
- [x] Add `src/lib.rs` so test targets can reach `Tui`; keep `main.rs` a thin binary.
- [x] Add the `live` test target: `TestBackend` scenarios for movement, `Tab` /
  Shift-Tab, `v`, detail, context, resize, quit, the former SMOKE frames, and leading
  tabs and spaces, run across layouts and detail modes by a `macro_rules!` matrix.
- [x] Delete `tests/terminal-ui/`.
- [x] Replace `--bench` / `--profile` with `--bench <driver>` (`prepare`, `stages`,
  `movement`) behind a `bench::Driver` enum; remove `--profile`; update `--help` and
  `tests/benchmark.rs`.
- [x] Delete the ignored `measures_prepared_movement` test; generate its workload as a
  fixture for the `movement` driver.
- [x] Switch `bench/kubernetes.sh` to `--bench stages` and update its runner test.
- [x] Add `bench/fixtures.sh`, writing to `.ace/bench/fixtures/`.
- [x] Replace `docs/guides/testing-terminal-ui.md` with a testing guide; update
  `docs/guides/kubernetes-benchmark.md` and the `AGENTS.md` command list.

## Next: restore responsive movement

Movement is noticeably slow after the interactive layout and detail work. Restore
responsive navigation without raising time budgets or weakening rendering behavior.
Horizontal scrolling adds repeated `h` / `l` input and redraw pressure.

- [ ] Measure movement and redraw cost with a representative multi-file diff.
- [ ] Measure `h` / `l` input-to-frame latency and remove avoidable horizontal-bound or
  redraw work.
- [x] Identify and remove redundant work from the interaction, layout, or terminal
  rendering hot path.
- [x] Meet the [movement target](performance.md#targets) in `bench/fixtures.sh` output.

Relevant boundaries: `src/interaction.rs`, `src/layout.rs`, `src/terminal.rs`,
`src/terminal/view.rs`, and `src/terminal/rows.rs`.

### Movement measurement checkpoint

The synthetic in-process baseline is complete and recorded in
[performance.md](performance.md#initial-movement-baseline). The 1,024-hunk-per-file
workload takes 54–55 ms per movement frame across all layouts and detail modes.
Actual terminal keypress latency and a representative user diff remain unmeasured;
the diagnostic excludes event reads, PTY transport, and terminal-emulator painting.

### Completed first movement refactor

Constant-time context visibility, precomputed row widths, and active-layout,
screen-bounded row construction are complete; see
[performance.md](performance.md#constant-time-context-visibility). The worst
`bench/fixtures.sh` movement step fell from 216.9 ms to 0.44 ms. The earlier proposals
to borrow row text (`DiffRow<'a>`) and to buffer terminal output are superseded by the
frame-cost plan below; output buffering is not measurable with the `movement` driver's
`io::sink()` backend and stays unstarted.

## Next: frame-cost movement plan

Goal: bring a movement step from roughly 300–430 µs to 30–60 µs. The floor is Ratatui's
own frame diff and escape-sequence encoding over the 120-by-36 screen (4,320 cells, about
40 µs); nearly every cell changes on `j` / `k`, and every design pays that cost. The plan
removes the work above it and never bypasses Ratatui's terminal, buffer, or diff path.

### Measured breakdown

A throwaway harness in `tmp/movement-profile/` (not part of the repository) loads the
1,024-hunk workload from `bench/fixtures.sh`, repeats `l`, `j`, `h`, `k` steps against a
120-by-36 `CrosstermBackend<io::Sink>`, and prints the mean step. macOS `sample` on that
harness, unified layout, mean 313 µs per step:

| Share | Work                                                                                                               |
|-------|--------------------------------------------------------------------------------------------------------------------|
| ~32%  | One `Paragraph` per row with `.scroll()`: grapheme reflow of the whole line, including the part left of the offset |
| ~25%  | `Layout::split` running the Cassowary solver every frame; `default-features = false` disables `layout-cache`       |
| ~22%  | Buffer reset, file list, footer, separators, and allocation                                                        |
| ~14%  | Ratatui buffer diff and escape-sequence encoding (the floor)                                                       |
| 3–4%  | Scroll limits rescanning the selected file on every keypress                                                       |
| ~2%   | Styled row construction                                                                                            |

Enabling `layout-cache` in the harness alone measured 204 µs unified, 281 µs vertical,
and 209 µs stacked, from 299, 414, and 338 µs; `Layout::split` then appears in under 0.2%
of samples.

### Slice 1: enable `layout-cache` (complete)

See [performance.md](performance.md#cached-layout-splits). The worst
1,024-hunk movement step fell from 0.44 ms to 0.30 ms.

### Slice 2: row writer with whole-row grapheme segmentation (complete)

See [performance.md](performance.md#direct-row-writing). The worst 1,024-hunk movement
step fell from 0.30 ms to 0.23 ms. The design below is implemented in
`src/terminal/row_writer.rs`.

Replace per-row `Paragraph` rendering in `src/terminal/view.rs` (`render_diff_rows`
and its callers) and the `Line` / `Span` construction in `src/terminal/rows.rs`
(`DiffRow`, `diff_row`, `styled_line`) with a row writer that fills Ratatui's frame
buffer directly through its public API (`Frame::buffer_mut`, `Buffer` indexing,
`Cell::set_symbol`, `Cell::set_style`, `Buffer::set_style`).

- Input: a `StyledRow` (marker, segments with `TextStyle`), the row's `Rect`, the
  horizontal offset, the row style, and the color count.
- Expand tabs with `src/tab_stops.rs` using 2-column stops counted from the payload
  start, as now.
- Segment the whole expanded payload once with `unicode-segmentation` extended grapheme
  clusters; map each cluster to the style of the segment containing its first byte.
  Clusters never split or disappear at style boundaries (the per-span segmentation in
  Ratatui drops a zero-width mark that starts a span).
- Measure each cluster with the same width function Ratatui uses (`unicode-width` via
  `Span::width` on the cluster), skip control-containing clusters as Ratatui does, and
  keep zero-width clusters attached to their base.
- Skip `horizontal_offset` columns. A wide cluster straddling the left edge fills its
  visible cells with blanks in its style, keeping every following column aligned.
  (`Paragraph` currently draws the whole wide character and shifts the row right by one.)
- Write clusters left to right; a wide cluster writes its symbol in the first cell and
  resets the continuation cells, matching `Buffer::set_stringn`. Stop at the pane's
  right edge; a wide cluster that does not fit is not drawn and its cell stays blank.
- Fill the full row width with the row style (change backgrounds stay full width), then
  draw the two-cell marker gutter (`+ `, `- `, or blank) and the payload after it, as
  `styled_line` does now.
- Alignment peers (`aligned_pair`) stay blank rows in their pane's change style.
- Preserve every existing rendering rule in `docs/spec/architecture.md` (Lossless and
  safe rendering, low-contrast palette, deletion dim modifier).
- Text stays in logical order; no bidirectional reordering.

Tests (write first, earn a logic red against the current `Paragraph` path where it
differs):

- A Thai cluster whose tone mark starts a new style segment renders whole in one cell.
- A double-width character straddling the left edge at offset 1 leaves one blank cell
  and keeps the next character in its column.
- A double-width character that does not fit at the right edge leaves a blank cell.
- Existing live tests for tabs, wide text, horizontal movement, and all three layouts
  keep passing unchanged.

### Slice 3: scroll extents computed on change, visible-row offsets (complete)

See [performance.md](performance.md#stored-scroll-extents) and
[architecture.md](architecture.md#viewport-coordinates). The worst 1,024-hunk movement
step fell from 0.23 ms to 0.21 ms, and `G` now reaches the last row past hidden context.
Context changes keep the current top row (`anchored_vertical_offset`); the plan's
"existing clamp" did not exist.

- Add a per-(file, layout, context-count) scroll extent: the visible rows as raw indices
  (`Box<[u32]>`) and the widest visible row. `Tui` keeps the extent for its current key
  and rebuilds it only when the selected file, layout, or context count changes;
  `NextLayout`, file rotation, and context changes compute the extent for the target key.
  Screen size does not affect the extent.
- `navigation_bounds` reads the stored extent instead of calling
  `visible_*_line_count` and `maximum_visible_width`, so `h` / `j` / `k` / `l` do no
  per-file work.
- Fix the viewport coordinate mismatch found by reading the code: `visible_diff_rows`
  and `visible_side_by_side_rows` apply `viewport.vertical_offset` as a raw-row skip
  before filtering, while `maximum_vertical_offset` counts visible rows. When context
  hides rows, `G` and paging stop short of the bottom. Make `vertical_offset` a
  visible-row index; the frame renders `extent.visible[offset..offset + height]`.
  Context changes keep the existing clamp in `transition_interaction`.
- Record the coordinate contract in `docs/spec/architecture.md`.
- Tests first: a live test with hidden context rows where `G` shows the last row; a
  unit test that the extent matches the visibility and width queries it replaces.

### Slice 4: static frame chrome and the file list

- `PreparedDocument::file_rows` clones every file label into a new `Vec` on every frame;
  with the Kubernetes corpus that is 20,017 allocations per keypress. Render the file
  list from borrowed labels and only the rows that fit the file-list pane around the
  selection, keeping `List` scrolling behavior (selection visible, `· ` highlight).
- Build footer hints from `&'static str` without per-frame `Vec<Line>` allocation.
- Draw the vertical `│` separator and the stacked `─` separator by filling buffer cells
  instead of building a `Text` of one `Line` per row or a repeated `String`.
- Tests: existing live tests for the file list, footer, and separators keep passing; add
  a live test that a selection beyond the pane height stays visible.

### Measurement for every slice

- `bench/fixtures.sh` worst `movement` step (target in
  [performance.md](performance.md#targets): 1 ms) and the harness mean per layout.
- Retired instructions are the primary measure; wall-clock numbers come from a shared
  work machine.
- Record each slice's result in
  [performance.md](performance.md#constant-time-context-visibility).
- Preparation is unaffected by slices 1, 2, and 4; slice 3 adds no preparation work.

## Investigate broken leading indentation

Leading tabs and spaces on code lines appear to render incorrectly. Recent changes may
be the cause; this is unconfirmed.

- [x] Reproduce with tab- and space-indented lines in every layout and detail mode.
- [ ] Determine whether a recent change introduced it.
- [x] Fix it and add a regression check.

## Remaining interactive delivery sequence

Each slice is incomplete until its automated checks and its human check both pass.

1. Add wrapping for unified and side-by-side panes.

   Human check: open a diff with long changed lines, disable wrapping, then use `h` and `l`.
   Both before and after panes move by the same horizontal offset and their aligned content stays
   aligned. Re-enable wrapping and verify no content is lost or rendered over another pane.

2. Display continuity markers on each content-pane edge when content extends beyond the visible
   frame: downward for content below, upward for content above, and leftward or rightward for
   horizontally clipped content.

   Human check: open a diff that exceeds the pane vertically and horizontally. Verify that each
   marker appears only while content continues past its corresponding edge, disappears at that
   edge's boundary, and does not obscure diff content or pane layout.

## Accepted requirements awaiting implementation

These existing architecture obligations have no implementation slice yet; listing them
here does not authorize implementation or settle additional behavior.

- [ ] Implement recursive `-r` comparison and stdin `-` operands under the existing
  [input contract](architecture.md#input-contract).
- [ ] Reject piped standard input combined with file arguments under the existing
  [input and terminal boundary](architecture.md#input-and-terminal-boundary).
- [ ] Complete the existing controlling-terminal output and finite-fallback requirements
  in the [input and terminal boundary](architecture.md#input-and-terminal-boundary).
- [ ] Implement the specified session-local contrast levels and `:` command entry under
  the existing [view model](architecture.md#view-model); unresolved interface details
  remain design work.

## Parked preparation optimization

Further preparation optimization is parked. Resumption requires considering a more
drastic refactor or a multi-threaded parsing architecture; the accepted architecture
remains single-threaded.

The eager-preparation contract, benchmark mode, retained prepared-file representation,
whole-document preparation, and pinned-corpus measurements are complete.
Remaining preparation work is to remove only measured redundant work, including inactive
layouts, rows outside the viewport, allocations, cloning, palette construction, and span
ordering. Active-rendering work belongs to the responsive-movement task above.
Each resumed slice requires its specification amendment, tests, verification, audit,
and commit under the [performance contract](performance.md).

## Deferred work

Git integrations beyond user-level Git diff-pager installation remain deferred.

- [ ] Combined merge diffs: assess Git's multi-parent patch format and an appropriate
  display model in a later task. Deferred by the user; support is not an acceptance
  requirement or prerequisite for the empty-input and Git-paging QoL work.
- [ ] Make the tab-stop width configurable; it is fixed at 2 columns.

## Later work

### Investigate live reload from git diff

- [ ] Investigate whether hdiff can reliably identify invocation through `git diff`
  and retain enough producer context to rerun the comparison when files change.

Assess what invocation and repository information reaches the pager, whether diff
arguments and revision/index/worktree choices can be recovered, and how stdin-only
input limits a faithful re-diff. Report feasibility and constraints for watching
changes and reloading the diff. This task authorizes investigation only; live-watch
implementation remains unapproved.

### Product slices

1. Add implicit Tree-sitter semantic strategies with textual fallback.

   Human check: open supported and unsupported source files in the same diff. Supported files
   gain semantic detail when available; unsupported files remain readable with textual diffing,
   and neither case changes the selected layout unexpectedly.

2. Add Tree-sitter syntax highlighting for TOML, Markdown, and JSON.

   Human check: open TOML, Markdown, and JSON changes and verify syntax detail remains readable
   across unified, vertical, and stacked layouts.

3. Refine highlighting coverage and additional navigation after the core interaction is stable.

   Human check: exercise every displayed shortcut on a mixed-language diff and verify that the
   footer/help text matches the key behavior, additions and deletions remain distinguishable, and
   low contrast remains readable without saturating the whole terminal.

4. Display the existing file list as a folder tree derived from its file names, without adding
   file-system navigation or changing file-selection behavior.

   Human check: open a diff containing files at multiple directory depths and verify that the
   left pane shows their shared directories as a readable tree while selecting and rotating files
   works exactly as it does for the flat list.

5. Add search mode entered with `/`, with `n` selecting the next result and `p` selecting the
   previous result.

   Human check: search a multi-file diff, verify every match is reachable in both directions with
   `n` and `p`, and confirm the selected result remains visible in its file and viewport.

## Working rule

Get docs up to date before implementation.

## Explicit non-goals for this phase

- No remote publication.
