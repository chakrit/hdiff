---
status: accepted
---

# hdiff roadmap

Track project tasks in this repository's Markdown. This roadmap owns the backlog.

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
Python, Go, and C fixture. It captures unified, vertical, and stacked ANSI-styled panes, then
each Tab-selected file, before exiting with `q`; SMOKE locks this observable terminal surface for
drift review.

The `v` layout cycle now preserves the selected file and viewport through unified, vertical, and
stacked displays. Vertical display aligns changed rows with a dimmer inner separator and retains
each pane's marker and spacer columns; both split displays render markerless, dimmed alignment
peers for unmatched additions and deletions. Unit and terminal-smoke checks cover the three
layouts.

Character detail now coalesces small equal islands inside changed spans, and `+` and `-` adjust
session-local context visibility without changing the selected file or current viewport. Unit and
terminal checks cover the completed interaction slice.

Horizontal movement now uses one synchronized viewport offset across content panes. `h` and `l`
move by one column, `0` and `^` move to the left edge, and `$` moves to the furthest useful
offset; movement remains bounded while the file list stays fixed.

`hdiff --install` backs up the global Git configuration file Git will edit, then registers the
current executable as the user-level `git diff` pager. Git passes the complete unified diff to
hdiff on standard input, preserving the entire file list in one interactive session.

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
12. Add unified, vertical, and stacked layout cycling with paired changed-row derivation.
13. Add user-level Git diff-pager installation with global-config backup and complete-diff input.
14. Add synchronized, bounded horizontal movement across unified, vertical, and stacked layouts.
15. Add `--help` and `--version`, including the package version and short source commit hash
    (`e3ba805`).
16. Reduce syntax preparation cost with Tree-sitter 0.27 (`4192938`). The recorded paired
    benchmarks save 2.157 seconds for 7/8, 1.681 seconds for 6/8, and 1.579 seconds for
    4/8; full measurements and provenance are in [performance.md](performance.md).

## Next: restore responsive movement

Movement is noticeably slow after the interactive layout and detail work. Restore
responsive navigation without raising time budgets or weakening rendering behavior.
Horizontal scrolling adds repeated `h` / `l` input and redraw pressure.

- [ ] Measure movement and redraw cost with a representative multi-file diff.
- [ ] Measure `h` / `l` input-to-frame latency and remove avoidable horizontal-bound or
  redraw work.
- [ ] Identify and remove redundant work from the interaction, layout, or terminal
  rendering hot path.
- [ ] Add a focused regression check that protects the resulting performance boundary.

Relevant boundaries: `src/interaction.rs`, `src/layout.rs`, `src/terminal.rs`,
`src/terminal/view.rs`, and `src/terminal/rows.rs`.

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

## Performance delivery plan

Further preparation optimization is parked. Resumption requires considering a more
drastic refactor or a multi-threaded parsing architecture; the accepted architecture
remains single-threaded.

Each phase has its own specification amendment, tests, verification, audit, and commit.

1. Establish the eager-preparation contract and benchmark mode.
2. Build a borrow-first prepared-file representation that preserves renderer and layout output.
   Completed: prepared files retain their layout, syntax, and line and character detail data.
3. Prepare layout, syntax, and character detail for the complete document before terminal entry.
   Completed: the terminal loop receives prepared data and selects only the active viewport.
4. Measure the final eighth, quarter, and half of pinned Kubernetes first-parent history through
   `hdiff --bench`; the reproducible command is in `docs/guides/kubernetes-benchmark.md`.
   Completed: the resolved measurements are recorded in `docs/spec/performance.md`.
5. Remove only measured redundant work from preparation and active rendering, including inactive
   layouts, rows outside the viewport, allocations, cloning, palette construction, and span
   ordering.

## Deferred work

Git integrations beyond user-level Git diff-pager installation remain deferred.

## Later slices

### Install: support git show

`hdiff --install` configures Git diff paging but does not make `git show` open the
same interactive hdiff view. Installation should support both commands.

- [ ] Determine the Git configuration boundary that routes both `git diff` and
  `git show` to hdiff.
- [ ] Add isolated configuration and invocation tests for `hdiff --install` and
  `git show`.
- [ ] Record the settled Git integration contract in `docs/spec/architecture.md`.

Relevant boundaries: `src/actions/install_git_pager.rs`, `tests/git_difftool.rs`, and
`docs/spec/architecture.md`.

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

get docs uptodate before implement always

## Explicit non-goals for this phase

- No remote publication.
