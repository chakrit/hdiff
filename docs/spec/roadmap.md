---
status: accepted
---

# hdiff roadmap

## Current position

The strict, loss-preserving Git/unified-diff parser and finite-output boundary are complete.
They accept multi-line hunks, extended Git metadata, ANSI-colored Git output, and metadata-only
Git sections. Finite rendering preserves review content while neutralizing terminal controls and
treats broken pipes as quiet success.

The first interactive Ratatui rendering boundary is implemented: the unified view has distinct
file-list and diff panes, while CROSSTERM retains terminal lifecycle and event ownership. The
slice passed its automated and human checks. Do not describe file navigation, hunk navigation,
resize handling, or later interactive slices as delivered until they pass their own human checks.

Low-contrast styling reserves aligned marker and payload columns on every row. Context is quiet
neutral text; deletions use muted text over a dim red background; additions retain the strongest
green emphasis and syntax colors. Its automated checks pass; the mixed-language terminal human
check remains required to complete the slice.

The compact two-column layout has no pane borders, labels, or titles; it renders one separator
with one space on each side between the file list and diff, keeps muted file-navigation hints at
the bottom of the list, and collapses the list before the diff. Its automated checks pass; the
multi-file terminal human check remains required to complete the slice.

Addition and deletion rows extend their backgrounds through the right edge of the active diff
pane while retaining marker and syntax colors. Its automated checks pass; the mixed-language
terminal human check remains required to complete the slice.

The terminal smoke suite uses one fixed-size tmux session and a multi-file Rust, JavaScript,
Python, Go, and C fixture. It captures ANSI-styled panes after launch and each Tab-selected file,
then exits with `q`; SMOKE locks this observable terminal surface for drift review.

## Completed slices

1. Establish the Rust CLI direction, architecture, testing boundary, and durable project records.
2. Record terminal-diff-viewer prior art and the Tree-sitter dependency decision and audit.
3. Implement strict parsing, loss-preserving document types, patch/stdin input selection, and
   malformed-input rejection.
4. Implement safe finite unified rendering and quiet broken-pipe handling, including Git
   metadata, multi-line hunks, colored input, and metadata-only Git sections.
5. Establish the Ratatui application boundary and render a unified view with distinct file-list
   and diff panes.
6. Audit the shortcut map so `Shift-Tab` rotates to the previous file, the inverse of `Tab`.

## Remaining interactive delivery sequence

Each slice is incomplete until its automated checks and its human check both pass.

1. Add Tree-sitter syntax highlighting with textual fallback before further navigation work.

   Human check: open supported and unsupported source files in the same diff. Supported payload
   lines gain syntax detail; unsupported files remain readable as plain text, and neither case
   changes the selected layout unexpectedly.

   The safe renderer is the only producer of display rows. Tree-sitter receives bounded virtual
   old/new hunk sources and returns semantic tokens mapped to those row payloads. The terminal
   applies tokens only to visible payloads; errors, unsupported paths, oversized hunks, and
   context records whose old and new paths select different languages retain textual rendering.

2. Add asymmetric low-contrast addition and deletion styling before the compact layout. Additions
   are strongest; deletion content is muted over a dim red background; context is quiet neutral
   text; every rendered row has aligned marker and payload columns.

   Human check: open a mixed-language diff and verify additions and deletions are visibly
   distinguishable without obscuring syntax colors or saturating whole lines.

3. The compact two-column layout is implemented before local Git integration. The left column
   contains the Tab-switchable file list and muted shortcut hints at its bottom; the right column
   contains the diff. One vertical separator with one space on each side divides the columns. Pane
   borders, labels, and titles are absent so content uses the available terminal cells.

   Human check: open a multi-file diff and verify the file list and muted hints occupy the left
   column, the diff occupies the right column, exactly one vertical separator is visible, and no
   pane border, label, or title consumes space.

4. Render hunk headers beginning with `@@` as muted cyan-blue structural lines while retaining
   neutral styling for Git metadata and file headers.

   Human check: open a multi-hunk diff and verify every `@@` header is visually distinct from
   source context without competing with additions or deletions.

5. Expand the file-list footer into a compact multi-row reference for every implemented movement,
   file-rotation, and exit shortcut.

   Human check: open a multi-file diff and verify the footer identifies vertical, page, top/bottom,
   hunk, file-rotation, and exit controls without reducing the file list below usable height.

6. Addition and deletion backgrounds extend through the right edge of the active diff pane.

   Human check: open a mixed-language diff and verify every addition and deletion background
   continues through the remaining visible cells without changing the marker or syntax colors.

7. Connect Ratatui rendering to the authoritative interaction state for vertical movement,
   file rotation, hunk movement, and resize.

   Human check: with a multi-file, multi-hunk diff open, verify `j`/`k`, `Ctrl-D`/`Ctrl-U`, and
   `g`/`G` move only the diff viewport; `Tab` changes the selected file and returns that file to
   its top; `{` and `}` move between that file's hunks; resizing redraws the same selected file
   and does not leave terminal artifacts; `q` restores the terminal.

8. Add side-by-side line rendering as an explicit Ratatui layout, preserving the selected file,
   viewport meaning, and file-list pane.

   Human check: press `v` on a changed file. Before and after lines occupy visibly separate,
   aligned panes; switching back to unified view retains the selected file and approximate
   location. Resize both views without overlap or a displaced column separator.

9. Add character-level detail within changed line pairs and session-local context controls.

   Human check: press `c` on a changed line and see only changed character spans gain detail;
   press `c` again to return to line detail. Use `+` and `-` to change visible context and verify
   that the selected file and current hunk remain understandable.

10. Add wrapping and synchronized horizontal scrolling for side-by-side panes.

   Human check: open a diff with long changed lines, disable wrapping, then use `h` and `l`.
   Both before and after panes move by the same horizontal offset and their aligned content stays
   aligned. Re-enable wrapping and verify no content is lost or rendered over another pane.

## Deferred work

Local Git integration remains deferred until the syntax-highlighting, contrast, and compact-layout
slices are usable in a real terminal.

## Later slices

1. Add implicit Tree-sitter semantic strategies with textual fallback.

   Human check: open supported and unsupported source files in the same diff. Supported files
   gain semantic detail when available; unsupported files remain readable with textual diffing,
   and neither case changes the selected layout unexpectedly.

2. Refine highlighting coverage and additional navigation after the core interaction is stable.

   Human check: exercise every displayed shortcut on a mixed-language diff and verify that the
   footer/help text matches the key behavior, additions and deletions remain distinguishable, and
   low contrast remains readable without saturating the whole terminal.

## Working rule

get docs uptodate before implement always

## Explicit non-goals for this phase

- No remote publication.
