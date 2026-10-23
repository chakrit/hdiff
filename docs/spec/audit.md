# Audit

How audits of hdiff classify findings and when a repair loop stops. These rules are
hdiff's exception to the school's `ace-audit` skill and `ace` workflow audit step; where
they differ, this file governs.

## Findings

A finding is a Violation only when it is one of:

- a reproduced behaviour defect, with the reproduction recorded;
- a disagreement between the code and a spec sentence, with the sentence quoted;
- a breach of a quoted rule whose text decides the case without analogy to another
  finding.

Every other finding is a Borderline. Borderlines are listed, not repaired, and never keep
an audit loop open.

An earlier pass's Violation is never a rule for a later pass.

## Repair loop

Every defect fix starts with a test that fails on the unfixed code. Style and lint fixes
get no tests. A test's expected values never come from the code under test.

A repair changes only what the cited defect, spec sentence, or rule requires; a finding's
suggested fix is advice.

The first pass audits the requested scope. Every later pass audits only the diff from
the commit the first pass audited to HEAD, and receives this file and every earlier
pass's Violations. Findings outside that diff are Out of scope.

A finding whose fix would restore a shape an earlier pass replaced stops the loop: report
both findings and wait for the user's ruling.

The loop closes when a pass over the repair diff finds zero Violations.

## Rulings

User rulings on what is or is not a Violation are recorded here. An agent may record only
that a shape stands or is out of scope; it never adds a prohibition.

Shapes that stand:

- A module without a `//! … Rules:` header.
- The split direction held in `VisibleRows`.
- Test helpers kept inside their module, not lifted across module boundaries.
- `Parent::label` matching the parent directly.
- `_ => None` arms that reject unlisted variants of a dependency's open-ended enum, such
  as crossterm events.
- `BySide`, `ByStage` and `ByLanguage` as separate types.
- `DiffLayout::ALL` as a hand-written list, `next` as an exhaustive match, and a unit
  test that walking `next` from the first layout visits `ALL` in order and returns to it.
- At 16 colours, a comment on a deleted line sharing the deleted body's `DarkGray`.

Out of scope: `RecordAddress` and `PageIndex` branding.
