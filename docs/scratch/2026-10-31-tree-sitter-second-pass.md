<!-- not spec because: the build plan for roadmap product slice 2 step 6; its decisions are written into docs/spec/. -->

# Tree-sitter second pass

Plan for roadmap product slice 2, step 6. hdiff's scanner keeps coloring every line as it
arrives; a second pass then re-colors each hunk with a Tree-sitter grammar compiled into
the hdiff binary, and the next frame shows the better colors. Grammar research is in
`docs/vendor/tree-sitter-grammar-sources.md`.

## Settled inputs

- nvim-treesitter is the source of the language list; hdiff works around its drawbacks.
- A maintainer-run generator, `nvim --headless -l <script>`, writes the language data
  into the repository; building and running hdiff need no Neovim.
- hdiff compiles a starter set of grammars into its binary.
- Built-in defaults need no configuration; advanced users can configure afterwards.
- hdiff maintains its own Tree-sitter config and loads grammars in process.

## Design

### Generator

One Lua script run by Neovim on a maintainer's machine. It reads nvim-treesitter from the
`vendor/nvim-treesitter/` submodule and the filetype tables from the running Neovim, and
writes committed, generated output that hdiff's build reads; the build never runs it.

- **File types:** `vim.filetype.inspect()` gives the extension, filename and pattern
  tables. An entry that detects by content is resolved with `vim.filetype.match` on the
  file name alone, which returns Neovim's own default for that name (`.h` gives `c`).
  Pattern entries are kept only where they are plain globs; Lua-pattern entries are
  dropped and listed in the generator's report.
- **Languages:** nvim-treesitter's `parsers.lua` gives each grammar's repository,
  revision and subdirectory; its filetype-to-parser aliases give the join with Neovim's
  filetypes.
- **Queries:** each starter language's `highlights.scm` with `; inherits:` parents
  concatenated, `#lua-match?` translated to `#match?` regex, and Neovim-only directives
  (`#set! priority`, `#set! conceal`) and captures (`@spell`, `@nospell`, `@conceal`)
  removed. A pattern whose translation fails is dropped and reported.
- **Grammar sources:** each starter grammar is a Git submodule directly under `vendor/`,
  named by its repository (`vendor/tree-sitter-go/`), shallow, at nvim-treesitter's
  pinned revision. The generator moves these pins to match the nvim-treesitter
  submodule. Grammars that commit no generated C (`generate = true`) stay scanner-only
  and are reported.
- **Output:** under `src/languages/`, marked generated: the Rust table (extension, file
  name and glob to filetype; filetype to grammar, its `vendor/` directory and source
  subdirectory), the translated `<lang>.scm` queries, and a header recording the Neovim
  version, the nvim-treesitter commit and each grammar's license.

### Language selection

The generated table becomes hdiff's only language selection: a path maps to a filetype
name, and the filetype names both the scanner table and the compiled grammar. The
scanner's hand-written extension table in `src/scanner/languages.rs` goes, so the
scanner and the second pass can never disagree about a file's language.

### Compiled grammars

`build.rs` compiles each grammar's `vendor/` C sources with the `cc` crate and links them
statically, failing with a `git submodule update --init` hint when a directory is empty;
`tree-sitter` returns to `Cargo.toml` (the 0.26.12 safety audit and 0.27.0 triage in
`docs/vendor/syntax-highlighting.md` apply; re-audit the version adopted). Each grammar is
reached through its generated `LANGUAGE` handle, as before. Starter set: the scanner's
55 mainstream languages, less any nvim-treesitter does not carry (the generator lists
them).

### The pass at runtime

- **Thread:** a fourth thread, the highlighter, shares nothing with the others. The
  worker sends it each finished hunk's two sides as owned text with the page and line
  addresses; it sends `Restyled { page, lines, syntax }` back over the existing channel.
  Keys and the first frame never wait on it; a slow grammar only delays the upgrade.
- **Per hunk:** one parser, query and cursor per language, created on first use; each
  side's records of a hunk joined into one buffer, as the old projection did, with the
  new side parsed as an edit of the old side's tree.
- **Captures to colors:** captures fold by prefix onto the existing `SyntaxClass`
  (`keyword.*` to Keyword, `string.*` to String, `function.*` to Function and so on);
  unknown and punctuation captures stay plain. Overlaps resolve innermost first, then
  the later pattern, which is nvim-treesitter's precedence; the result is the same
  ordered, non-overlapping spans the scanner produces.
- **Main:** `PreparedFiles::receive` replaces those lines' syntax partitions in place
  (a new `LineMap` replacement); character-detail partitions are untouched. A page's
  rows and geometry never change, so scroll position is unaffected.
- **Failure:** a grammar that errors on a hunk leaves the scanner's colors. Languages
  with no compiled grammar keep the scanner's colors.

### Later slice: advanced configuration

A TOML file at `$XDG_CONFIG_HOME/hdiff/config.toml` that adds file-type mappings and
extra grammars (a compiled library plus its query directory). Loading a library at
runtime needs `unsafe` or `tree-sitter-loader`, so it gets its own design walk. This
plan only keeps language selection keyed by filetype name so the config can extend it.

## Spec amendments (first commit)

- `architecture.md` Technology direction: replace "highlights again with Tree-sitter
  whenever the `tree-sitter` binary is available in the `PATH`" with the compiled-in
  second pass, nvim-treesitter as the language source and the generator.
- `performance.md` Preparation boundary: replace "runs the `tree-sitter` binary as a
  separate process ... hdiff links no Tree-sitter" with the highlighter thread; fill-in
  "one hunk at a time" (already ruled).
- `roadmap.md` slice 2 step 6: rewritten to this build order.

## Build order

1. Spec amendments and the vendor research doc.
2. `vendor/nvim-treesitter/` submodule, the generator, and its first output for one
   language (Go) with the `vendor/tree-sitter-go/` submodule; review the report.
3. Language selection on the generated table; existing scanner tests stay green
   unchanged.
4. `build.rs` compiles Go; measure clean build time and binary size before going on.
5. Highlighter thread, `Restyled`, in-place replacement, capture fold; Go end to end.
6. Remaining starter languages in batches, measuring build time and size per batch.
7. Measurements: `first-frame` and `loading` stay within targets with the pass on; a
   new diagnostic reports hunk-to-restyle latency; Kubernetes retired instructions for
   the first pass unchanged.

## Validation

- Unit: generator output parses; filetype selection (extension, file name, glob,
  content-detected defaults); capture fold; overlap resolution; in-place replacement.
- Live: a fixture whose lines change color once the pass lands, in every layout, with
  scroll position unchanged; keys answered while the pass runs.
- Measurements per `docs/spec/performance.md`; build time and binary size reported per
  batch (`code · fast-builds`).
