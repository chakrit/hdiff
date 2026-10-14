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
- Test suite and benchmark drivers: the `Tui` loop behind `src/lib.rs`, the in-process
  `live` test target replacing the tmux SMOKE captures, `--bench <driver>` (`prepare`,
  `stages`, `movement`), and `bench/fixtures.sh`; see
  [interactive loop](architecture.md#interactive-loop),
  [benchmark drivers](performance.md#benchmark-drivers), and the
  [testing guide](../guides/testing.md).
- Responsive movement: the worst `bench/fixtures.sh` movement step fell from 216.9 ms to
  0.19 ms through constant-time context visibility, cached layout splits, direct row
  writing, stored scroll extents with visible-row offsets, and static frame chrome.
  Closed at `0521119` with mean steps of 141, 183, and 129 µs (unified, vertical,
  stacked); the 30–60 µs goal is not pursued. Per-slice results are in
  [performance](performance.md#constant-time-context-visibility).
- Leading indentation: leading tabs rendered without width; 2-column tab expansion
  fixed it, with live regression checks in every layout and detail mode.
- File tree sidebar per the [file tree](architecture.md#file-tree) contract: files named
  by their shared path suffix under directory headings, top-level files first and
  otherwise in input order, one state marker per file (copies `+` at the new path).
  Human check passed through `./git.sh` on 2026-10-04; the Kubernetes `labels` stage
  adds 0.03–0.09% retired instructions per range.
- Session-local row wrapping toggled with `w`, starting disabled, per the
  [view model](architecture.md#view-model); human check passed on 2026-10-12.
- Continuity markers in a one-cell frame around each diff pane (one side marker per
  clipped side, dimmed separator tone) and frame rows on the file list, per the
  [continuity marker](architecture.md#continuity-markers) contract; human check passed on
  2026-10-14.

Automated checks (formatting, Clippy, `cargo test --locked` including `tests/live.rs`)
and human terminal checks passed for every delivered slice. Historical delivery plans
and verification records are preserved in `.ace/spec-cleanup-roadmap-before.md` and in
Git history.

## Remaining interactive delivery sequence

Each slice is incomplete until its automated checks and its human check both pass.

No slice is scheduled.

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
ordering. Each resumed slice requires its specification amendment, tests, verification, audit,
and commit under the [performance contract](performance.md).
Before the first resumed comparison, change `bench/kubernetes.sh` to key its improvement
verdict on retired instructions as the performance contract requires; it still keys on
cycles.

## Deferred work

Git integrations beyond user-level Git diff-pager installation remain deferred.

- [ ] Combined merge diffs: assess Git's multi-parent patch format and an appropriate
  display model in a later task. Deferred by the user; support is not an acceptance
  requirement or prerequisite for the empty-input and Git-paging QoL work.
- [ ] Make the tab-stop width configurable; it is fixed at 2 columns.
- [ ] Design configurable defaults: the user sets view-preference defaults, such as
  wrapping, in a configuration file.

## Later work

### Investigate live reload from git diff

- [ ] Investigate whether hdiff can reliably identify invocation through `git diff`
  and retain enough producer context to rerun the comparison when files change.

Assess what invocation and repository information reaches the pager, whether diff
arguments and revision/index/worktree choices can be recovered, and how stdin-only
input limits a faithful re-diff. Report feasibility and constraints for watching
changes and reloading the diff. This task authorizes investigation only; live-watch
implementation remains unapproved.

### Smooth scrolling

- [ ] Add smooth scrolling. Not yet planned.

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

4. Add search mode entered with `/`, with `n` selecting the next result and `p` selecting the
   previous result.

   Human check: search a multi-file diff, verify every match is reachable in both directions with
   `n` and `p`, and confirm the selected result remains visible in its file and viewport.

## Working rule

Get docs up to date before implementation.

## Explicit non-goals for this phase

- No remote publication.
