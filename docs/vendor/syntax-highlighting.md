# Rust syntax-highlighting libraries

Status: researched 2026-08-20. Version and feature details are observations from the
listed sources and must be rechecked when the dependency is added.

## Recommendation

Use `syntect` for the first implementation, initially with its `default-fancy` feature set.
Keep the dependency behind a small `syntax_highlighting` boundary that accepts source lines
and a detected language and returns styled spans. The diff parser and renderer should not
depend on `syntect` types.

## Why syntect fits hdiff

- It supports Sublime Text syntax definitions and themes for Rust applications.
- It includes built-in syntax definitions and themes, avoiding a runtime grammar installation
  contract for the first release.
- Its documented API supports ANSI terminal output directly.
- `delta` and `bat` both use it, relevant prior art for a terminal diff viewer.
- Its lower-level highlighting API can preserve diff prefixes while styling source tokens.
- Current crates.io metadata is `syntect` 5.3.0, MIT licensed, with feature flags for trimming
  loaders and engines.

## Important trade-offs

The default `syntect` configuration uses Oniguruma through a native dependency. That adds a C
build and platform toolchain surface. The `default-fancy` feature selects the pure-Rust
`fancy-regex` path; it is documented as slower, especially in debug builds, but removes that
native build requirement. The first implementation should benchmark real diff fixtures before
reconsidering the engine; it should not silently switch engines at runtime.

The bundled grammar set is broad but not universal, and syntax detection still belongs to
hdiff based on file paths and diff headers. Unknown languages must render as plain text without
losing input.

## Rejected for the first boundary

`tree-sitter-highlight` is a credible alternative with incremental parsing and structural
highlighting. It is a poorer first boundary for hdiff because language grammars are separate
packages, grammar/version management becomes part of the product surface, and the viewer does
not currently need syntax trees or incremental editing. Reconsider it if hdiff needs structural
navigation, semantic highlighting, or editor-like incremental updates.

## Dependency criteria

- Public, documented Rust API; no internal module coupling.
- MIT licensing compatible with hdiff.
- No runtime network or filesystem requirement for the built-in baseline.
- No mandatory native toolchain in the default hdiff build path.
- Highlighting failure degrades to plain text rather than dropping or rewriting diff content.
- The dependency remains isolated so replacing the engine does not reshape the document model.

## Sources

- [syntect repository](https://github.com/trishume/syntect/)
- [syntect crates.io metadata](https://crates.io/crates/syntect/5.3.0)
- [tree-sitter-highlight crates.io metadata](https://crates.io/crates/tree-sitter-highlight/0.26.12)
- [Tree-sitter Rust bindings](https://github.com/tree-sitter/tree-sitter/tree/master/lib/binding_rust)
- [bat README](https://github.com/sharkdp/bat/blob/master/README.md)

## Quick safety audit

Audit depth: triage. The published `syntect` 5.3.0 crate was downloaded as source without
executing build hooks. The manifest declares `build = false`, contains no `build.rs`, and the
crate contains no precompiled native object files. The default feature set would enable the
optional `onig` dependency; the recommended `default-fancy` feature avoids it.

The shipped library source has no networking, process-spawning, or environment/secret-reading
capabilities. The only `unsafe` API found is a narrowly scoped unchecked bit conversion in
`src/highlighting/style.rs`; it is not required by the recommended integration boundary. The
embedded `.packdump` assets are syntax/theme data, not executable blobs. OSV returned no advisory
for `syntect` 5.3.0 at audit time.

Triage verdict: **GO**, with the exact version pinned in `Cargo.lock` when adopted. This is not a
full transitive-closure or artifact-vs-repository audit; the dependency should be rechecked when
the lockfile and feature set are introduced.

## Standard audit follow-up

Audit date: 2026-08-20. The exact crates.io artifact for `syntect` 5.3.0 was downloaded and
unpacked without executing code. The manifest sets `build = false`, has no `build.rs`, and
ships no native object or shared-library files. The recommended `default-fancy` feature path
uses `fancy-regex` and does not enable `onig`/`onig_sys`; the lockfile records the optional
native path but it is not in the recommended feature closure.

The published artifact was compared with the Git tag `v5.3.0` at commit
`e4670846ecf16d8832db6c43d531bec466214e27`. No shipped source file differed from the tag. The
artifact-only files were Cargo-generated metadata (`.cargo_vcs_info.json`, `Cargo.toml.orig`);
the repository-only files were maintainer tooling, test fixtures, and CI material. No
unmapped executable blob was found; the `.packdump` files are embedded syntax/theme data.

Capability review of the shipped `src/` found no networking, process spawning, credential or
environment-secret reads, persistence writes, dynamic loading, or obfuscation. `std::env` and
`std::process::exit` hits occur only in examples. The sole `unsafe` block is the documented
unchecked style-bit conversion in `src/highlighting/style.rs`.

The source-package lockfile contains the default-fancy dependency closure at 5.3.0,
including `fancy-regex`, `regex`, `regex-automata`, `regex-syntax`, `bincode`, `flate2`,
`fnv`, `once_cell`, `plist`, `serde`, `serde_derive`, `thiserror`, `walkdir`, and `yaml-rust`
plus their supporting crates. It also contains dev-only and optional `onig` packages; those
are outside the recommended default-fancy runtime path.

OSV and GitHub Advisory searches found no advisory naming `syntect` 5.3.0. Advisory databases
can lag, and this static review does not prove that every transitive crate's published bytes
match its VCS source. Re-run the audit against hdiff's committed `Cargo.lock` immediately
before installation, including advisory checks for every resolved package.

Standard-audit verdict: **GO WITH CAVEAT**. The direct package and artifact integrity checks
are clean; adoption remains contingent on pinning the resolved lockfile and repeating the
transitive advisory/artifact review when the dependency is actually added.

## hdiff lockfile audit

hdiff was initialized as a Rust binary crate and added `syntect = 5.3.0` with
`default-features = false` and `features = ["default-fancy"]`. `Cargo.lock` resolves 48
packages. `cargo test --locked` passed (10.77s compile/test profile) and
`cargo clippy --locked --all-targets --all-features` passed (3.58s check profile).

The resolved default-fancy closure contains no networking, process execution, credential or
secret reads, dynamic loading, or persistence markers in the downloaded Rust sources. No
resolved package contains a `build.rs` hook or shipped native object/shared-library blob.

OSV querybatch over all 48 locked crates returned two informational advisories:

- `bincode 1.3.3` — `RUSTSEC-2025-0141`, unmaintained after the project ceased development.
- `yaml-rust 0.4.5` — `RUSTSEC-2024-0320`, unmaintained and recommends the `yaml-rust2` fork.

These are transitive dependencies enabled by syntect's `dump-*` and `yaml-load` features;
they are not known exploit advisories, but they reduce maintenance confidence. Replacing them
is not an hdiff-local change because syntect owns those feature edges. Final verdict for the
current lockfile: **NEEDS-HUMAN REVIEW / CONDITIONAL GO**. The dependency is technically
clean and tests pass, but adoption should be accepted only with explicit approval of these two
unmaintained transitive crates or an upstream syntect feature/dependency change.
