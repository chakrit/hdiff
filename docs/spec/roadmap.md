---
status: accepted
---

# hdiff roadmap

## Current position

hdiff has a strict, loss-preserving unified-diff parser and input-source boundary. The
current slice adds terminal-independent safe unified rendering and finite non-interactive
output before terminal lifecycle and navigation work begins.

## Steps

1. Study the intended terminal UI and choose the Rust CLI stack, entry point, test strategy,
   formatting, and input/rendering boundaries.
2. Study `dandavison/delta` as prior art, focusing on pager boundaries, terminal resizing,
   navigation state, view toggles, ANSI handling, and failure cases.
3. Complete ACE Phase 1 read-only onboarding: select active skills and prepare the proposed
   `AGENTS.md` and `ace.toml` changes as one approval-gated batch.
4. Record delta findings in `docs/vendor/` and hdiff’s settled architecture and interaction
   surface in `docs/spec/`.
5. Design and implement resize handling, view-mode shortcuts, file switching, and an
   interactive file list as separate slices with terminal edge-case tests.
6. Validate behavior, commit logical slices locally, and wait for explicit approval before
   any push or other shared-state publication.

## Explicit non-goals for this phase

- No remote publication.
