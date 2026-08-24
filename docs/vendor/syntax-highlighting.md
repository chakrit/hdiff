# Tree-sitter syntax highlighting

## Tree-sitter decision

Decision date: 2026-08-20. Remove syntect completely. Use `tree-sitter` and
`tree-sitter-highlight` with statically linked grammar crates for Rust, JavaScript, Python, Go,
and C. Keep the grammar set deliberately small until usage justifies another parser.

The highlighter boundary accepts a language and a borrowed source slice and returns terminal
spans. It owns one parser and highlight configuration per language, initializes each lazily on
first use, reuses parser/query state across lines, and keeps a bounded cache of parsed files.
Diff prefixes are stripped into a borrowed payload view; returned byte ranges are translated
back to display columns without allocating or copying the source line. Highlighting runs only
for visible rows, with plain-text fallback for unknown languages or parser errors.

This is the performance shape: no runtime grammar discovery, no filesystem lookup in the hot
path, no per-line parser construction, no whole-file string copies, and no syntax-tree work
for rows outside the viewport. The document model remains independent of Tree-sitter node and
query types so the renderer can stay terminal-focused.

Tree-sitter queries assign semantic capture names such as `keyword`, `function`, `type`, and
`string`; hdiff maps those captures to a compact terminal style table. Grammar crates and
queries are version-pinned together. Adding a language is an explicit dependency and registry
entry, not a dynamic plugin scan.

The initial dependency set is `tree-sitter 0.26.13`, `tree-sitter-highlight 0.26.13`,
`tree-sitter-rust 0.24.2`, `tree-sitter-javascript 0.25.0`, `tree-sitter-python 0.25.0`,
`tree-sitter-go 0.25.0`, and `tree-sitter-c 0.24.2`.

## Tree-sitter safety audit

Audit date: 2026-08-20. The 0.26.12 `tree-sitter` and `tree-sitter-highlight` releases and the
committed 33-package closure were audited. The 0.26.13 patch releases were adopted without a
repeat audit at the user's direction. The default closure has no optional WASM or Wasmtime feature
enabled.

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
`tree-sitter-highlight`, `tree-sitter-c`, `tree-sitter-go`, `tree-sitter-javascript`,
`tree-sitter-python`, and `tree-sitter-rust`. Differences were limited to Cargo-generated
manifests/metadata, generated Rust bindings, and packaged C/query files expected by the crate
publish configuration; no unexplained executable payload was found.

OSV querybatch over all 33 locked crates returned zero advisories. The direct repositories are
the official Tree-sitter organization projects, with stable tagged releases matching the audited
versions. Standard-audit verdict: **GO** for the 0.26.12 releases. Continue to pin the lockfile
and keep the WASM/Wasmtime feature disabled unless it is separately audited.

## Sources

- [tree-sitter-highlight crates.io metadata](https://crates.io/crates/tree-sitter-highlight/0.26.13)
- [Tree-sitter Rust bindings](https://github.com/tree-sitter/tree-sitter/tree/master/lib/binding_rust)
