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

## Completed: profiling and benchmarking

- [x] Account for input acquisition, diff parsing, and input-to-ready latency.
- [x] Add separate preparation-stage and stack profiles.
- [x] Preserve reproducible provenance and validate benchmark comparison behavior.
- [x] Measure instrumentation overhead on the pinned Kubernetes corpus.

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

- [ ] Combined merge diffs: assess Git's multi-parent patch format and an appropriate
  display model in a later task. Deferred by the user; support is not an acceptance
  requirement or prerequisite for the empty-input and Git-paging QoL work.

## Later slices

### Support diff-producing git show invocations

Support diffs and surrounding commit text from explicit `git show <commit> | hdiff`
invocations. General text-only paging and automatic `pager.show` registration are
outside this scope.

- [ ] Preserve and display commit headers and messages with their associated diffs;
  the current parser rejects the preamble from ordinary `git show` output.
- [ ] Verify producer-independent surrounding-text handling for multiple commits
  containing diffs and rejection of nonempty text-only input; combined merge-diff
  support remains deferred.
- [x] Define behavior for zero-byte diff input in interactive and finite-output modes,
  including Git commands with no changes to display.
- [ ] Assess which additional Git subcommands should use hdiff, including `git log -p`;
  distinguish patch-producing invocations from ordinary non-diff output before
  recommending additional pager registrations.
- [ ] Add isolated invocation tests for explicit `git show <commit> | hdiff`, including
  commit metadata and empty input, plus a terminal check of the actual invocation.
- [x] Record the settled Git integration contract in `docs/spec/architecture.md`.

Relevant boundaries: `src/parser.rs`, `src/document.rs`,
`src/terminal/`, `tests/git_difftool.rs`, and `docs/spec/architecture.md`.

### QoL delivery plan: empty input and Git paging

The empty-input and producer-independent surrounding-text contracts are settled in
`architecture.md`. This plan covers their implementation, explicit `git show` piping,
and a broader Git-subcommand compatibility assessment. Combined merge-diff support
is deferred. Changes to the user's Git configuration require separate authorization.

#### First: empty input

Completed: normal viewer dispatch returns successfully on zero bytes before parsing,
preparation, output selection, or terminal setup.

Required behavior: zero-byte input exits successfully without output, preparation,
terminal acquisition, raw mode, or alternate-screen entry. Apply the same behavior
to stdin, an empty patch file, and identical file operands. Preserve measurement
reports for `--bench` and `--profile`. Existing Git metadata-only sections remain content;
malformed input remains an error rather than being treated as empty.

Implement the empty-input outcome at normal viewer dispatch, before terminal setup,
using the existing input and output boundaries in `src/main.rs`; introduce no new
module for this branch. Check the existing CLI tests before extending their coverage.

Acceptance checks: CLI exit status and empty stdout/stderr for each empty source;
a pseudo-terminal invocation that exits without a keypress or terminal-control output;
existing Git metadata-only sections still rendered; malformed nonempty input still rejected; empty
measurement input still emits valid zero-count reports.

Verification passed: 75 unit tests and 15 integration tests, formatting, Clippy, and
pseudo-terminal invocations for empty stdin, an empty patch file, and identical operands.
Each empty pseudo-terminal invocation exited with status 0 and emitted no bytes.

#### Second: generic surrounding text with diffs

Reuse the existing safe source-line representation and finite/interactive rendering
boundaries. The existing document owns only files; commit text must have an explicit
document-level owner rather than being attached to an arbitrary file or discarded.

Represent the input as ordered text sections and file-diff sections, preserving
source order without interpreting commit headers or identifying the producing command.
Collect surrounding text at the document boundary; recognized file headers enter
the existing strict diff parser. A failure inside a recognized diff remains an error.
Do not fall back to text after a malformed hunk or add a Git-show input mode.
Reject nonempty input containing no diff; metadata-only Git diff sections remain valid.

Prepare each text section once using the existing sanitization and row-rendering
capabilities. In unified view render each section once; in vertical and stacked
layouts project the same section into both panes. Preserve eager, single-threaded
preparation. Proposed navigation placement: text preceding a file group appears above
its first file and trailing text remains after the last file. This placement remains
proposed. Keep repeated paths in separate source positions so multiple commits cannot
be collapsed into one file entry.

Reuse existing layout and navigation representations where they support these ordered
sections; adapt their ownership where they assume every visible item is a file.
Do not introduce a parallel renderer for `show`, `log`, or individual text formats.
Combined merge-diff parsing and rendering are outside this slice; record the current
limitation without treating its resolution as a prerequisite for explicit piping.

Acceptance checks: isolated actual `git show <commit> | hdiff` invocations; ordinary
and multiple commits with diffs; rejection of nonempty patchless commit, tag, tree,
and blob output containing no diff; colored metadata and terminal-control sanitization;
finite output and broken pipes; commit association while navigating files in all three
layouts. Review the terminal display manually.
Also test text before, between, and after file diffs, rejection of text-only input, and
malformed recognized patches. No test may modify the user's Git configuration.

#### Third: additional Git subcommands

Produce a broad compatibility assessment before adding registrations. Examine
`git log` and `git log -p`, `git reflog`, `git stash show` and `git stash show -p`,
`git range-diff`, `git format-patch --stdout`, `git blame`, and `git status`, including
existing `git diff` variants such as `--cached` and `--stat`. Record output shape,
applicable pager setting or explicit pipe, parser/display coverage, practical review
value, and whether registration affects non-patch invocations.

Use official Git documentation and isolated invocation checks. Recommend explicit
patch-producing invocations only when hdiff preserves all their review content.
Recommend automatic registration only when hdiff also handles the subcommand's
ordinary non-patch invocations within its diff-viewer scope. The output is a
supported/deferred recommendation with evidence; additional implementation and
registrations require scope approval.

Investigate `git log -p` and `git stash show -p` as likely reusable patch producers,
without limiting the assessment to those commands or preselecting registrations.
Git's [`pager.<cmd>` setting](https://git-scm.com/docs/git-config)
applies to a subcommand, not only its patch-producing invocations, and
[`stash show`](https://git-scm.com/docs/git-stash) defaults to a diffstat.
[`range-diff`](https://git-scm.com/docs/git-range-diff) uses a distinct, unstable
human-readable output format and needs a separate compatibility decision.
For `log -p`, check repeated paths across commits, graph prefixes, custom formatting,
and entries without patches before claiming compatibility.

Deliver a compatibility table with verified invocation examples and a recommendation
for each candidate: ready for installation, usable by explicit pipe, or deferred with
its concrete limitation. Keep raw findings in the repository's vendor-documentation
convention and settled support contracts in `architecture.md`; the roadmap owns tasks.

#### Verification and delivery

Implement approved behavior with meaningful failing behavior tests first, then
formatting, Rust tests, and Clippy using the installed working toolchain. Run terminal
checks for changed output and review any snapshot drift before accepting it; the
recorded JavaScript highlighting drift must not be mistaken for a new regression.
Obtain approval for resource-intensive work; no preparation benchmark is planned
for these QoL slices unless implementation introduces a preparation performance claim.
Audit each complete slice against the approved spec and commit locally after its
checks pass. Installing into the user's environment and pushing are separate actions.
The compatibility research can run independently of the empty-input implementation;
invocation verification depends on the surrounding-text slice. Commit empty-input
behavior and surrounding-text support as coherent verified slices.

### Investigate apparent duplicated source tokens

- [ ] Reproduce and test the reported display of `excluded_mcp` appearing twice in
  a changed Rust line; establish whether duplication occurs in the input or is
  introduced by hdiff before diagnosing a rendering bug.

Reported content, with the unrelated filename column omitted:

```diff
-         let excluded = ace.excluded_mcpexcluded_mcp();
+         let excluded = ace.excluded_mcpexcluded_mcp()?;
```

Use a minimal source pair with a single `excluded_mcp()` call and only an added `?`
to check that displayed payloads preserve the source exactly. Exercise unified,
vertical, and stacked layouts and retain a regression test if the duplication is
reproduced. The report alone does not establish the original source contents.

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

get docs uptodate before implement always

## Explicit non-goals for this phase

- No remote publication.
