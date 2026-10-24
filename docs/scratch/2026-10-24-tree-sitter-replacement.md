<!-- not spec because: research into replacing parts of Tree-sitter; nothing is decided. -->

# Replacing parts of Tree-sitter for faster preparation

Question: can hdiff run its own faster parsing or highlighting while still taking its
rules from Tree-sitter, either by building its own runtime or by reusing pieces of the
Tree-sitter library?

## Where syntax time goes

Kubernetes 7/8 at `f65b056` (`.ace/bench/profile-full/f65b056-7-8.stages.txt`):

| Stage             | Time   | Work                                                 |
|-------------------|--------|------------------------------------------------------|
| Tree-sitter parse | 16.3 s | 96,373 parse calls over 167 MB of projected source   |
| Highlight query   | 7.7 s  | 14.0 M captures, about 550 ns each                   |
| Rest of syntax    | 1.1 s  | projection, span mapping                             |
| Preparation total | 28.8 s | syntax is 25.1 s of it                               |

Parsing runs at about 10 MB/s. The full-run stack sample at `957c715`
(`.ace/bench/profile-full/957c715-7-8.stacks.txt`, about 28,000 samples) splits
Tree-sitter's share by top of stack:

| Part of Tree-sitter                                        | Samples | Share |
|------------------------------------------------------------|---------|-------|
| LR driver, GLR stack, subtree building and release         | ~11,100 | ~40%  |
| Query cursor walking the tree and matching patterns        | ~5,800  | ~21%  |
| Generated lexer (`ts_lex`, lexer advance, keyword lexing)  | ~2,100  | ~8%   |
| `malloc`, `free`, `memmove`, much of it called from above  | ~4,000  | ~14%  |

Error recovery (`ts_parser__recover` and its helpers) is under 1%, even though every hunk
is a code fragment. Lexing, the part a tree-less design must keep, is small. Most of the
cost is in building trees and querying them.

## What Tree-sitter's pieces allow

- A grammar is plain data behind a C ABI: the `TSLanguage` struct holds the parse tables,
  lex modes, the generated `lex_fn` and `keyword_lex_fn`, and the external scanner. Go
  and C have no external scanner; Rust, Python and JavaScript do.
- The tables follow `LANGUAGE_VERSION` (14/15). That layout is internal to Tree-sitter and
  changes between releases; only `parser.h` documents it.
- The bundled highlight queries are small and shallow. Go's has 16 capture patterns, none
  deeper than parent plus field, and one `#match?`. Rust's has 103 patterns and 6
  predicates. C, Python and JavaScript fall between.
- `tree_sitter::set_allocator` (`ts_set_allocator`) replaces Tree-sitter's allocator
  process-wide.
- Prior art: Helix's `tree-house` rewrote the highlighter in Rust over the stock C parser.
  Its stated goals were simplicity and incremental layers, not raw throughput.
  `tree-sitter-c2rust` is a mechanical C-to-Rust port of the runtime, built for targets
  without libc. Neither replaces the parser for speed.

## Options

### 1. Compiled highlight matcher

Keep Tree-sitter's parser and tree. Compile each bundled `highlights.scm`, once at
startup, into a table from node kind ID to class, with the parent-kind, field and
predicate checks the patterns need. Walk the tree once and look up each node, replacing
`QueryCursor`.

- Tree-sitter still supplies the rules: the grammar, its node kinds and its query file.
- Expected gain: most of the 7.7 s query stage. The query cursor's per-node status work
  and pattern-map searches go away; a plain walk over this tree should take around 1 s.
  That is roughly a quarter of preparation.
- Risk: matching the query's precedence and capture order exactly. A test can use the
  real `Query` as the oracle over the fixtures and the Kubernetes corpus.
- Scope: one module and no new dependency. A query feature the compiler does not
  understand fails the bundled-grammar test, rather than highlighting silently wrong.

### 2. hdiff-owned allocator for Tree-sitter

Install a size-class free-list allocator through `set_allocator`. Tree-sitter allocates
and frees many small subtree and stack nodes per parse.

- Expected gain: part of the 14% spent in `malloc` and `free`, realistically 5–8%.
- Small and isolated, and cheap to measure.

### 3. hdiff-owned tree-less parser over Tree-sitter's tables

Write an LR driver in Rust that reads `TSLanguage`'s tables and calls its generated lexer
and external scanner, emits highlight events as it reduces, and never builds a tree.

- Expected floor: lexing plus table lookups, about 3–4 s instead of 16 s.
- It must reimplement GLR for each grammar's declared conflicts, and error recovery that
  agrees with Tree-sitter's on fragments. These are the hardest parts of the runtime.
- It ties hdiff to an internal ABI that changes between Tree-sitter versions.
- The roadmap's Tree-sitter semantic diff needs real trees, so the tree-building parser
  would still be needed.
- Effort: months, with output that drifts from Tree-sitter's. Not recommended.

### 4. Grammar-derived lexer with local heuristics

Generate a fast lexer per language from the grammar's tokens, and guess functions,
properties and types from the neighbouring tokens: an identifier followed by `(`, or
after `.`. This could run at hundreds of MB/s, but its output is no longer Tree-sitter's,
which conflicts with the spec's assignment of highlighting to Tree-sitter. It fits only
as a fast first pass under on-demand Tree-sitter highlighting.

## Conclusion

- Options 1 and 2 together should take syntax from about 25 s to about 17 s, and
  preparation to about 21 s.
- Option 3 might add another ~10 s at a cost and risk out of proportion.
- No single-threaded Tree-sitter-based design reaches the 1 s target. That still needs
  on-demand or parallel highlighting (`.ace/perf-preparation-options.md`).
- Option 1 is the clean version of "Tree-sitter supplies the rules" and is worth a spike.
  The spike would compile Go's query first, check it against `Query` on the corpus, and
  measure retired instructions.
