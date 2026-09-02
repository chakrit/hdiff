# Tree-sitter syntax highlighting

## Tree-sitter decision

Decision date: 2026-08-20. Remove syntect completely. Use `tree-sitter`'s public parser and query
APIs with statically linked grammar crates for Rust, JavaScript, Python, Go, and C. Keep the
grammar set deliberately small until usage justifies another parser.

The syntax boundary accepts a language and a borrowed source slice and returns terminal spans. It
owns one parser, query, and query cursor per language, initializes each lazily on first use, and
uses one old-to-new projection edit to incrementally parse each same-language hunk pair. Diff
prefixes are stripped into a borrowed payload view; returned byte ranges are translated back to
display columns without allocating or copying the source line. Unknown languages and parser
errors fall back to plain text.

This is the performance shape: no runtime grammar discovery, no filesystem lookup in the hot
path, no per-line parser construction, no whole-file string copies, and no syntax-tree work
outside preparation. The document model remains independent of Tree-sitter node and query types
so the renderer can stay terminal-focused.

Tree-sitter queries assign semantic capture names such as `keyword`, `function`, `type`, and
`string`; hdiff maps those captures through `SyntaxTheme`. Grammar crates and queries are
version-pinned together. Adding a language is an explicit dependency and registry entry, not a
dynamic plugin scan.

The current dependency set is `tree-sitter 0.27.0`, `tree-sitter-rust 0.24.2`,
`tree-sitter-javascript 0.25.0`, `tree-sitter-python 0.25.0`, `tree-sitter-go 0.25.0`, and
`tree-sitter-c 0.24.2`.

## Tree-sitter safety audit

Audit date: 2026-08-20. The 0.26.12 `tree-sitter` release and the committed 33-package closure
were audited. The 0.26.13 patch release was adopted without a repeat audit at the user's direction.
The default closure has no optional WASM or Wasmtime feature enabled.

The direct parser crates contain legitimate build scripts that compile their bundled parser C
source into the build output using `cc`; they do not download files, invoke shells, read
credentials, or write outside `OUT_DIR`. `tree-sitter-language` only publishes WASM metadata
when a WASM target is selected. No precompiled object, shared-library, DLL, or WASM blob is
shipped in the audited packages.

The runtime source closure has no networking, process execution, environment-secret reads,
persistence writes, dynamic loading, or obfuscation markers. The C sources contain no socket,
process, dynamic-loader, or memory-protection calls. FFI and `unsafe` code are limited to the
documented Rust bindings to the bundled Tree-sitter C parser library; grammar crates expose
their generated `LANGUAGE` handles through that binding.

The published artifacts were compared with the matching GitHub tags for `tree-sitter`,
`tree-sitter-c`, `tree-sitter-go`, `tree-sitter-javascript`, `tree-sitter-python`, and
`tree-sitter-rust`. Differences were limited to Cargo-generated manifests/metadata, generated Rust
bindings, and packaged C/query files expected by the crate publish configuration; no unexplained
executable payload was found.

OSV querybatch over all 33 locked crates returned zero advisories. The direct repositories are
the official Tree-sitter organization projects, with stable tagged releases matching the audited
versions. Standard-audit verdict: **GO** for the 0.26.12 releases. Continue to pin the lockfile
and keep the WASM/Wasmtime feature disabled unless it is separately audited.

### Tree-sitter 0.27.0 triage

Audit date: 2026-09-02. Triage covered the published `tree-sitter 0.27.0` crate, not its
default-install closure or artifact-to-tag and version-to-version integrity. The downloaded crate
matched the crates.io sparse-index SHA-256 checksum
`2038684e0058edba0d17302619f62eabce4a8e11c6ac59506996a8d79848851d`.

The published crate contains no precompiled object, shared-library, DLL, WASM, or binary blob.
Its build script reads Cargo build variables, copies its WASM symbol list into `OUT_DIR`, and
compiles the bundled Tree-sitter C source there through `cc`; it contains no download, shell,
credential, or persistence path. A capability scan found no networking, process execution,
secret-file access, dynamic loading, executable-memory allocation, or obfuscation path in the
published Rust, C, or header sources. OSV returned zero advisories for `tree-sitter 0.27.0` at the
audit date.

Triage verdict: **GO** for `tree-sitter 0.27.0` with the default `std` feature and WASM disabled.
The unchanged grammar crates retain the previous standard-audit evidence; this triage does not
extend that evidence to newly resolved transitive versions.

## Sources

- [Tree-sitter Rust bindings](https://github.com/tree-sitter/tree-sitter/tree/master/lib/binding_rust)
- [Tree-sitter query API](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/4-api.html)
- [Tree-sitter 0.27.0 release](https://github.com/tree-sitter/tree-sitter/releases/tag/v0.27.0)
- [tree-sitter 0.27.0 on crates.io](https://crates.io/crates/tree-sitter/0.27.0)
