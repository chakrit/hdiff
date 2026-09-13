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
