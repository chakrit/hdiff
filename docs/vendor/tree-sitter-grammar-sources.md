# Tree-sitter grammar sources

Where a Tree-sitter highlighting pass can get its language list (language, file types,
grammar repository and pinned revision, highlight queries) and how grammars reach the
user's process. Checked 2026-10-31 against GitHub, crates.io and project documentation;
counts change weekly. "Unverified" marks claims taken only from secondary sources.

## Language lists

| Source                    | Format                 | Languages | File types       | Pin             | Queries                         | License           |
|---------------------------|------------------------|-----------|------------------|-----------------|---------------------------------|-------------------|
| tree-sitter-language-pack | one JSON file          | 372       | yes              | commit          | 314, copied from grammar repos  | MIT               |
| Helix                     | one TOML file          | 342       | yes, globs too   | commit          | 341, Helix capture names        | MPL-2.0           |
| nvim-treesitter           | Lua data table         | 323       | no (in Neovim)   | commit or tag   | 323, Neovim-only predicates     | Apache-2.0        |
| arborium                  | YAML per language      | ~112      | aliases only     | commit          | vendored per language           | MIT OR Apache-2.0 |
| syntastica                | one TOML file          | 65        | yes              | commit          | Neovim-derived, rewritten       | MPL-2.0           |
| Difftastic                | Rust code, Cargo deps  | ~60       | hand glob table  | crate version   | crate `HIGHLIGHTS_QUERY`        | MIT               |
| Zed extensions            | TOML per extension     | ~414      | yes              | commit          | Zed capture names               | per extension     |
| Emacs treesit-auto        | Elisp                  | 62        | regex            | partly          | none (Elisp font-lock)          | GPL-3.0           |
| GitHub Linguist           | YAML                   | 836       | yes              | n/a             | none (TextMate scopes only)     | MIT               |

Tree-sitter has no official registry; its documentation points at the nvim-treesitter
wiki. Each grammar repository's `tree-sitter.json` declares its own file types, scope and
query paths, and `tree-sitter-loader` reads it.

- **tree-sitter-language-pack** —
  <https://github.com/xberg-io/tree-sitter-language-pack>,
  `sources/language_definitions.json`. Entry: `"rust": {"abi_version": 14, "extensions":
  ["rs"], "repo": "...", "rev": "<sha>"}`; optional `directory`, `generate`, `license`.
  Queries keep whichever upstream's capture names they were copied from (preference
  emacs/zed/lapce > helix > nvim). Releases about weekly; the organisation moved once
  (kreuzberg-dev to xberg-io).
- **Helix** — <https://github.com/helix-editor/helix>, `languages.toml` and
  `runtime/queries/<lang>/`. `[[language]]` carries `file-types` (extensions or `{ glob =
  ... }`), `shebangs`, `injection-regex`; `[[grammar]]` carries `source = { git, rev }`.
  Format stable since 23.10. No release since 25.07.1 (2025-07); `master` active but
  slowing. MPL-2.0 is file-level: modified copies of its files stay MPL-2.0.
- **nvim-treesitter** — <https://github.com/nvim-treesitter/nvim-treesitter>,
  `lua/nvim-treesitter/parsers.lua` on `main`; `master` frozen since 2025-05. Archived
  roughly 2026-04 to 2026-07 (unverified dates); Neovim issue
  <https://github.com/neovim/neovim/issues/39006> plans a data-only replacement.
  Extension mapping lives in Neovim's `runtime/lua/vim/filetype.lua`, which is Lua code.
  Maintenance tiers 1–3 per parser; 8 parsers need `tree-sitter generate`.
- **Difftastic** — <https://github.com/Wilfred/difftastic>. The closest diff-tool
  precedent hand-maintains about 56 grammar crates and an extension glob table
  (`src/parse/guess_language.rs`), and folds captures by prefix into a few classes.
- **arborium** — <https://github.com/bearcove/arborium>; one crates.io crate per grammar,
  selected by feature flag. **syntastica** — <https://github.com/RubixDev/syntastica>; last
  release 2025. Both use Grammar Orchard forks (<https://codeberg.org/grammar-orchard>)
  where they exist.

## Getting grammars into the process

| Route                      | User needs            | Coverage         | Trust                                   |
|----------------------------|-----------------------|------------------|-----------------------------------------|
| Build at runtime           | C compiler            | any listed       | runs fetched C with full privileges     |
| language-pack prebuilt     | network on first use  | 372, 6 platforms | SHA-256 from the same release; unsigned |
| WebAssembly grammars       | network or bundled    | partial `.wasm`  | sandboxed in wasmtime                   |
| Compiled into hdiff        | nothing               | what hdiff ships | reviewed at hdiff's build               |

- **Build at runtime** — `tree-sitter-loader` (<https://docs.rs/tree-sitter-loader>)
  compiles with the `cc` crate into `$TREE_SITTER_LIBDIR` or the platform cache. Helix
  (`hx --grammar fetch/build`) and nvim-treesitter `main` (also needs the `tree-sitter`
  CLI, `curl`, `tar`) do the same. On macOS without Command Line Tools, `/usr/bin/cc`
  opens Apple's installer dialog instead of failing. The loader's library API is about
  2% documented and tracks the CLI.
- **language-pack prebuilt** — Rust crate `tree-sitter-language-pack` (1.22.1) downloads
  per-platform bundles from GitHub Releases into
  `~/.cache/tree-sitter-language-pack/v<ver>/`; URL and cache overridable by environment;
  permissive grammar licenses only; no compile-in feature.
- **WebAssembly** — `tree-sitter` crate feature `wasm` (wasmtime 48). Neovim's build grew
  6.7 MB to 12 MB, startup +0.5 ms (<https://github.com/neovim/neovim/pull/28415>). Only
  the lexer runs in wasm. Grammars publish `.wasm` only if they use the shared
  tree-sitter release workflow. No published lexing-speed or per-module load numbers.
- **Compiled into hdiff** — per-grammar crates or arborium / syntastica-parsers bundles.
  Every grammar update needs an hdiff release; clean builds add minutes of C compilation;
  binary growth estimated (unverified) at 20–50 MB for ~50 grammars, ~100 MB for ~200.

A downloaded native grammar runs its constructors at `dlopen`, with hdiff's privileges.
A notarized macOS build with the hardened runtime cannot load unsigned grammar libraries
without the discouraged `disable-library-validation` entitlement.

## Gotchas

- The `tree-sitter` crate evaluates only `eq?`, `match?`, `any-of?` and their `not-` /
  `any-` forms. Any other predicate (`#lua-match?`, `#has-ancestor?`, `#not-kind-eq?`) is
  stored but never checked, so its pattern matches unconditionally. nvim-treesitter uses
  `#lua-match?` in 134 query files; Helix highlights use almost only standard predicates.
- `; inherits: <lang>` is an editor convention; the consumer concatenates parent query
  files itself (185 files in Helix, 141 in nvim-treesitter).
- A language's grammar revision and its queries must come from the same commit of the
  list; queries are validated only against the pinned grammar revision.
- No shared capture-name standard exists (Helix `@variable.other.member`, Neovim
  `@variable.member`, Zed and upstream sets differ). Every multi-source consumer folds
  captures by prefix to a small set of classes, as Difftastic does.
- hdiff's `Cargo.toml` declares no license, which matters before embedding MPL or GPL
  query files.
