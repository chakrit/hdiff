---
status: draft
---

# hdiff initial architecture

This document is the first architectural position for `hdiff`. It is intentionally a review
draft: statements under “Open decisions” are questions, not settled behavior.

## Product intent

`hdiff` is an interactive terminal diff viewer for understanding a change set quickly. It
should make the structure of a diff—files, hunks, additions, deletions, and context—easy to
navigate without requiring the reader to remember a command-line sequence or leave the
terminal.

The primary reader is a developer reviewing local or generated diff output. The first release
optimizes for a single user at one terminal, fast startup, predictable keyboard interaction,
and faithful rendering of the supplied diff.

## First-release boundary

### In scope

- Read unified diff text from standard input.
- Read a diff from one or more file arguments when that input shape is useful to the CLI.
- Parse the stream into files, hunks, and line records while preserving original text.
- Show a navigable file list and a focused diff view.
- Navigate files, hunks, and lines with a small discoverable keymap.
- Scroll vertically without losing the current location.
- Resize cleanly and redraw from retained state.
- Toggle at least a normal diff view and a side-by-side view if the terminal width permits.
- Provide a pager-first TUI with a persistent bottom toolbar that shows the active mode and
  context-sensitive shortcut hints.
- Make user-facing settings adjustable through shortcuts and re-render the affected view
  immediately.
- Exit cleanly on EOF, quit input, and terminal errors.
- Provide useful non-interactive behavior when output is not a terminal.

### Explicitly out of scope

- Editing files or applying patches.
- Git repository operations, staging, commits, or network access.
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

The document is the source of truth and retains enough original content to render unknown or
partially understood diff lines without data loss.

- `DiffDocument`: ordered files and document-level metadata.
- `DiffFile`: file identity, headers, and ordered hunks.
- `Hunk`: old/new ranges, header text, and ordered records.
- `Record`: context, addition, deletion, or unclassified/raw line, with original payload.
- `Cursor`: the currently focused file/hunk/record location.
- `ViewMode`: a tagged choice of unified or side-by-side presentation.
- `Viewport`: terminal width, height, and scroll offsets.

The interaction state is a value that transitions in response to an input event. A transition
returns a new state and a rendering request; it does not reach into the terminal or document.
This makes navigation testable without a real terminal and keeps invalid cursor positions out of
the renderer.

## User-facing feature sequence

The implementation should land in slices that each leave a usable surface:

1. Parse stdin and render a static, faithful unified diff.
2. Add terminal lifecycle handling, quit, vertical scrolling, and resize redraw.
3. Add file/hunk navigation and a visible position indicator.
4. Add the interactive file list and synchronized file selection.
5. Add side-by-side rendering where width allows; defer horizontal scrolling until real usage
   demonstrates that wrapping or a wider layout cannot solve the need.
6. Add robust malformed-input handling and non-terminal output behavior.
7. Refine highlighting coverage and additional navigation after the core interaction is stable;
   highlighting itself remains a first-release capability.

## Initial CLI surface

The first command should be intentionally small:

```text
hdiff [OPTIONS] [FILE ...]
```

The default input is standard input. Options should describe representations or input policy,
not encode interactive procedures. The exact option names and whether file arguments belong in
the first release remain open until the input contract is tested against real usage.

## Interaction model

The primary interaction is pager-like rather than a dashboard: the diff occupies the main
viewport, and a bottom command/status strip provides the familiar `:` entry point for commands
and a compact toolbar for the common actions. The toolbar is a navigational aid, not a second
control plane; every setting has one keyboard path, and changing it produces a new interaction
state and immediate redraw.

The toolbar should expose the current file, view mode, highlighting state, and the shortcuts
relevant to the current context. A help view can expose the complete keymap when the compact
toolbar cannot fit it. The exact visual treatment and command vocabulary remain open, but the
pager-first model and live shortcut-driven settings are settled product requirements.

## Technology direction

Rust is the proposed implementation language because the product is a terminal CLI with strict
control over input, rendering, cleanup, and startup behavior. The terminal library, argument
parser, and diff-parser choices are not yet settled; they should be selected from maintained
public APIs and evaluated against resize events, raw-mode restoration, ANSI handling, and test
seams. Syntax highlighting is assigned to `syntect` through an hdiff-owned boundary; the
dependency rationale and alternative are recorded in
`docs/vendor/syntax-highlighting.md`.

The first test boundary should be terminal-independent: parser fixtures, state-transition tests,
layout snapshots, and renderer output tests. A small number of end-to-end terminal checks can be
added after the lifecycle boundary is known.

## Failure and safety posture

- Never modify the input files.
- Preserve unknown diff lines rather than silently dropping them.
- Restore terminal state on every exit path after raw mode is entered.
- Treat a resize as a new layout calculation over the same document and interaction state.
- Report malformed input with context while still displaying the recoverable portion when safe.
- Make non-terminal output finite and script-friendly rather than entering an interactive loop.

## Open decisions for review

- Is `hdiff` specifically a Git diff viewer, or must unified diffs from any producer be first-class?
- Should file arguments mean “diff these paths” or “read these already-produced diff files”?
- Is side-by-side comparison a first-release requirement or a later view mode?
- What is the minimum keymap: familiar pager keys only, or explicit file-list shortcuts too?
- What should the non-terminal contract be: pass-through, normalized rendering, or an error?
- Which terminal backend and parser crates meet the lifecycle and test requirements?

## Next design work

The next pass should study `dandavison/delta` and comparable terminal viewers, then amend this
document with the chosen input contract, dependency selections, state-transition vocabulary,
and initial keymap. Implementation begins only after those choices are reviewed.
