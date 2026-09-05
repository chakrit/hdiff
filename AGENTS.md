# 

This project's AI coding environment is managed by
[ACE](https://github.com/ace-rs/ace). Run `ace` to start a coding session.
Run `ace setup` if not yet configured.

Skills and conventions are provided by the PRODIGY9 school and
are symlinked into `.agents/skills/`. Skill edits go through
symlinks into the school clone — propose changes back to the school repo
when ready. Run `ace config` or `ace paths` to debug configuration issues.

## ACE workflow

Track tasks only in repository Markdown at `docs/spec/roadmap.md`.

Continue automatically through ACE workflow steps unless the workflow names an explicit confirm
gate. Create ACE save points at useful recovery boundaries, especially before context becomes
large enough to make the work less effective; do not stop merely because an intermediate step
completed.

Report the measured preparation-time saving for every optimization slice.
Run the Kubernetes preparation benchmark at ordinary scheduler priority; never
de-prioritize it.

## Durable artifacts

`docs/` holds this project's durable record.

**`docs/spec/` is authoritative — read the relevant spec before working, and comply.**
It owes no justification: a rule that departs from mainstream practice, from what you
would have picked, or from what you expected is not grounds to escalate, annotate, or
re-open it. If a spec is wrong, raise it with the user and amend the spec.

**Everything settled is a `docs/spec/` amendment** — an instruction you were given, an
approach that was agreed, a library that was picked, a convention or preference that was
fixed. Write it there as it was given: the rule, at the length it was given, with no reason
supplied and no note of what it was chosen over. A one-sentence rule — "use RESTful routes"
— is a complete entry. There is no decisions log.

File new material by the gate, first match wins: third-party lookup → `vendor/`; a how-to →
`guides/`; our own design or surface → `spec/`; unsettled exploration → `scratch/` (last
resort, opened with a "not spec because ___" line). Nothing defaults to `scratch/`.

**Before writing into a `docs/` folder, read that folder's `README.md` first.** It holds the
folder's filing test, filename format, and lifecycle rules, and they are binding. Nothing
else surfaces them.

**`docs/spec/README.md` indexes every spec file — keep it current.** Read the index before
adding a spec file, so you amend the existing doc on a subject instead of writing a second
one. Adding, renaming, or retiring a file updates its row in the same change.
