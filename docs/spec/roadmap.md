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
interaction slice adds vertical viewport movement, file rotation, and resize redraw; its automated
and human checks passed. The viewport is the reader's cursor, and hunk jumps are absent until a
visible hunk focus makes their destination legible.

Low-contrast styling reserves aligned marker and payload columns on every row. Context is quiet
neutral text; deletions use muted text over a dim red background; additions retain the strongest
green emphasis and syntax colors. Its automated and mixed-language terminal checks pass.

The compact two-column layout has no pane borders, labels, or titles; it renders one separator
with one space on each side between the file list and diff, keeps muted file-navigation hints at
the bottom of the list, and collapses the list before the diff. Its automated and multi-file
terminal checks pass.

Addition and deletion rows extend their backgrounds through the right edge of the active diff
pane while retaining marker and syntax colors. Its automated and mixed-language terminal checks
pass.

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
7. Add Tree-sitter syntax highlighting with textual fallback.
8. Add asymmetric low-contrast addition and deletion styling with aligned marker and payload
   columns.
9. Add the compact two-column layout, including the multi-row file-list footer and hunk-header
   styling.
10. Extend addition and deletion backgrounds through the active diff pane's right edge.
11. Connect Ratatui rendering to authoritative interaction state for viewport movement, file
    rotation, and resize.

## Remaining interactive delivery sequence

Each slice is incomplete until its automated checks and its human check both pass.

1. Add side-by-side line rendering as an explicit Ratatui layout, preserving the selected file,
   viewport meaning, and file-list pane.

   Human check: press `v` on a changed file. Before and after lines occupy visibly separate,
   aligned panes; switching back to unified view retains the selected file and approximate
   location. Resize both views without overlap or a displaced column separator.

2. Add character-level detail within changed line pairs and session-local context controls.

   Human check: press `c` on a changed line and see only changed character spans gain detail;
   press `c` again to return to line detail. Use `+` and `-` to change visible context and verify
   that the selected file and current hunk remain understandable.

3. Add wrapping and synchronized horizontal scrolling for side-by-side panes.

   Human check: open a diff with long changed lines, disable wrapping, then use `h` and `l`.
   Both before and after panes move by the same horizontal offset and their aligned content stays
   aligned. Re-enable wrapping and verify no content is lost or rendered over another pane.

4. Display continuity markers on each content-pane edge when content extends beyond the visible
   frame: downward for content below, upward for content above, and leftward or rightward for
   horizontally clipped content.

   Human check: open a diff that exceeds the pane vertically and horizontally. Verify that each
   marker appears only while content continues past its corresponding edge, disappears at that
   edge's boundary, and does not obscure diff content or pane layout.

## Deferred work

Local Git integration remains deferred.

## Later slices

1. Add implicit Tree-sitter semantic strategies with textual fallback.

   Human check: open supported and unsupported source files in the same diff. Supported files
   gain semantic detail when available; unsupported files remain readable with textual diffing,
   and neither case changes the selected layout unexpectedly.

2. Refine highlighting coverage and additional navigation after the core interaction is stable.

   Human check: exercise every displayed shortcut on a mixed-language diff and verify that the
   footer/help text matches the key behavior, additions and deletions remain distinguishable, and
   low contrast remains readable without saturating the whole terminal.

3. Display the existing file list as a folder tree derived from its file names, without adding
   file-system navigation or changing file-selection behavior.

   Human check: open a diff containing files at multiple directory depths and verify that the
   left pane shows their shared directories as a readable tree while selecting and rotating files
   works exactly as it does for the flat list.

## Working rule

get docs uptodate before implement always

## Explicit non-goals for this phase

- No remote publication.
