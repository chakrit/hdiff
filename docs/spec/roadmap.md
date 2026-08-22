---
status: accepted
---

# hdiff roadmap

## Current position

The strict, loss-preserving unified-diff parser and input-selection boundary are complete.
The static renderer is also complete: it produces finite non-interactive output, preserves
source bytes in the document, neutralizes terminal controls, and treats broken pipes as quiet
success for finite output.

## Completed slices

1. Establish the Rust CLI direction, architecture, testing boundary, and durable project records.
2. Record terminal-diff-viewer prior art and the Tree-sitter dependency decision and audit.
3. Implement strict parsing, loss-preserving document types, patch/stdin input selection, and
   malformed-input rejection.
4. Implement safe finite unified rendering and quiet broken-pipe handling.
5. Implement terminal lifecycle handling with explicit controlling-TTY events, staged cleanup,
   pager navigation, and resize redraw.

## Next slice

Implement file/hunk navigation and the interactive file list with synchronized selection.

## Later slices

1. File/hunk navigation and the interactive file list with synchronized selection.
2. Side-by-side rendering, character detail, and synchronized horizontal scrolling.
3. Tree-sitter semantic strategies with textual fallback.
4. Highlighting coverage and additional navigation after the core interaction is stable.

## Working rule

get docs uptodate before implement always

## Explicit non-goals for this phase

- No remote publication.
