<!-- not spec because: a proposed design awaiting the user's approval. -->

# One data-driven highlighter for many languages

The user ticked 55 languages in `2026-10-26-highlight-languages.md`. Writing a lexer
for each one would be 55 pieces of hand-written code to maintain. This design instead
uses three small engines and describes each language as data, accepting rougher output
in return.

## Observation

Almost every ticked language is made of the same few lexical pieces:

- **Comments** begin with a line marker (`//`, `#`, `--`, `;`, `%`, `'`) or sit between
  a pair of block delimiters (`/* */`, `{- -}`, `(* *)`, `<!-- -->`, `--[[ ]]`).
  Haskell and OCaml block comments can nest.
- **Strings** have a delimiter, an optional escape character, and an optional raw or
  multi-line form (`` ` ``, `"""`, `r"..."`, `@"..."`, `[[ ]]`).
- **Numbers** are digits, radix prefixes, `_` separators, a decimal point, an exponent,
  and an alphanumeric suffix. One scanner covers every language.
- **Identifiers** are C-style. A few languages also allow `-` (CSS, Clojure), `$` (PHP,
  shell, JavaScript), `@` or a trailing `?`/`!` (Ruby, Elixir, Crystal).
- **Keywords, builtin types and constants** are word lists.
- **Operators** are runs of punctuation.

The highlight classes hdiff colors (keyword, string, comment, number, function, type,
constant, operator, property, variable) come from those pieces plus a few
neighbour-token rules that are the same in nearly every language.

## The three engines

### 1. Code engine (about 45 languages)

A single-pass byte scanner, driven by one `CodeSpec` per language:

```rust
static GO: CodeSpec = CodeSpec {
    line_comments: &["//"],
    block_comments: &[Block::flat("/*", "*/")],
    strings: &[Quote::escaped("\""), Quote::escaped("'"), Quote::raw_multiline("`")],
    identifier: Identifier::C,
    keywords: &["break", "case", "chan", /* ... */ "var"],
    types: &["bool", "byte", "error", "int", "string", /* ... */],
    constants: &["true", "false", "nil", "iota"],
    function_keywords: &["func"],
    type_keywords: &["type"],
    capitalized_is_type: false,
};
```

These rules are shared by every language and switched by spec fields:

- An identifier followed by `(` is a function.
- An identifier after `.` is a property, or a function when `(` follows it.
- An identifier after a function keyword (`func`, `fn`, `def`, `function`, `fun`,
  `sub`) is a function. One after a type keyword (`type`, `class`, `struct`, `enum`,
  `trait`, `interface`) is a type.
- A capitalized identifier is a type, in the languages where that convention holds
  (Java, C#, Kotlin, Swift, Rust, Haskell, Elixir and others).
- An ALL_CAPS identifier is a constant.
- For configuration-style languages, the key at the start of a line before `=` or `:`
  is a property.
- Any other identifier is a variable.

It covers:

- **C-style:** Go, Rust, C, C++, CUDA, Objective-C, Java, C#, Kotlin, Swift, Dart,
  Scala, Groovy, JavaScript, Zig, Gleam, Crystal, Protocol Buffers, GraphQL, HCL.
- **Script-style:** Python, Ruby, Perl, Shell, PowerShell, R, Julia, Lua, Elixir,
  Erlang, Nim, SQL, Visual Basic.
- **Functional:** Haskell, OCaml, F#, Clojure.
- **Styling:** CSS, SCSS and Less.
- **Data and build files:** JSON, TOML, INI, YAML, Dockerfile, Makefile, CMake and
  Assembly.

Most languages are 20–40 lines of word lists.

### 2. Markup engine

The markup engine scans tags, attribute names, attribute values, entities and
`<!-- -->` comments. A spec maps an embedded region to another language's spec:
`<script>` to JavaScript, `<style>` to CSS, and `<?php ... ?>` to PHP. It covers HTML,
XML, Vue, Svelte and PHP.

### 3. Markdown engine

The Markdown engine applies rules at the start of each line: headings, list markers,
block quotes and fences. Inside a line it handles code spans, emphasis and links. A
fenced block's info string selects a code spec, such as ` ```go `.

## How state crosses lines

The scanner carries one small state from record to record on each side of a hunk:
ground, inside a block comment (with its nesting depth), inside a multi-line string, or
inside an embedded language. It scans each record's payload directly, with no projection
buffer, no tree and no allocation per token. Only the spans are allocated.

Expected speed is hundreds of MB/s on ASCII code. That would take the syntax stage on
the 167 MB Kubernetes projection from 25 s to under 1 s.

## What it gives up

- **Types:** a type is recognized only by builtin names, declaration keywords and
  capitalization. In Go, `*T`, `T{` and generics are often shown as variables.
- **Hunks that start inside a construct:** a hunk that begins inside a block comment or
  a multi-line string is colored as code until the closing delimiter. A cheap fix is to
  recolor the earlier lines as a comment when a closing `*/` appears with no opener.
- **Interpolation:** `${...}`, `#{...}` and `\(...)` inside strings stay string colored.
- **Regex literals:** JavaScript, Ruby and Perl treat `/` as a regex only after an
  operator, `(`, `,` or a line start. Perl's quote-like operators (`qw`, `s///`) are
  rough.
- **Heredocs:** one generic rule colors from `<<WORD` (and `<<-`, `<<~`) up to the line
  holding `WORD`. Other forms render as code.
- **Language-specific constructs:** C++ templates, Rust macros and C preprocessor logic
  get no special handling. Preprocessor lines are colored as keywords.
- **Embedding depth:** embedded regions go one level deep only.

## Effort and order

1. The code engine with Go, then measure the Kubernetes benchmark.
2. Move the other four bundled languages (Rust, C, JavaScript, Python) to it.
3. Add the rest of the code-engine languages in batches, each with a sample-file test.
4. Build the markup engine (HTML, XML, Vue, Svelte, PHP).
5. Build the Markdown engine.

Estimated size: the code engine is 700–900 lines, the markup engine about 300, the
Markdown engine about 200, and the language specs about 1,500 lines, mostly word lists.

## Consequence for Tree-sitter

Once all five bundled languages move, no language left uses Tree-sitter for
highlighting. The crate and its grammars would then serve only the planned semantic diff
(roadmap slice 3).
