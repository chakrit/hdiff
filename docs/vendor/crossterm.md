---
status: accepted
---

# CROSSTERM 0.29

Sources: [crates.io](https://crates.io/crates/crossterm/0.29.0),
[release](https://github.com/crossterm-rs/crossterm/releases/tag/0.29), and
[OSV](https://osv.dev/).

## Standard safety audit

Audit date: 2026-08-22.

The audited package is `crossterm` 0.29.0 with its default features plus
`use-dev-tty`. Its resolved normal dependency closure contains 40 packages.

The registry package records commit `36d95b26a26e64b0f8c12edfe11f410a6d56a812`, matching
the signed 0.29 release. Package-source differences are packaging metadata and line endings.
The downloaded crate has SHA-256
`d8b9f2e4c67f833b660cdb0a3523065869fb35570177239812ed4c905aeff87b`.

No shipped precompiled binary artifacts were found. CROSSTERM has no build script. The closure's
build scripts run compiler and platform probes; no download, credential search, or network action
was found. CROSSTERM itself performs terminal, signal, and platform calls by design. Its `tput`
fallback is a local process invocation. Mio exposes general socket types, but the selected
CROSSTERM event path does not create network connections. No telemetry or secret-file search was
found.

An OSV batch query for every resolved package returned no advisories. This static-source audit
does not prove future releases or runtime behavior outside the audited source.

Verdict: GO for the pinned 0.29.0 release with `use-dev-tty`.
