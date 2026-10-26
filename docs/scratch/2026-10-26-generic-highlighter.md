<!-- not spec because: a proposed design awaiting the user's approval. -->

# One data-driven highlighter for many languages

The user ticked 55 languages in `2026-10-26-highlight-languages.md`. Writing a lexer for
each one would be 55 pieces of hand-written code to maintain. This design uses one
scanner instead, and describes each language as data. Rougher output is accepted in
return. Code, markup (tag-based text such as HTML and XML) and prose (Markdown) all go
through the same rules; no language gets special-case code.

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
constant, operator, property, variable) come from those pieces, plus rules about
neighbouring tokens that are the same in nearly every language.

## The scanner

One single-pass byte scanner, driven by one `Spec` per language:

```rust
static GO: Spec = Spec {
    line_markers: &[Marker::anywhere("//", Class::Comment)],
    delimited: &[
        Pair::flat("/*", "*/", Class::Comment),
        Pair::escaped("\"", Class::String),
        Pair::escaped("'", Class::String),
        Pair::multiline("`", Class::String),
    ],
    identifier: Identifier::C,
    words: &[
        (&["break", "case", "chan", /* ... */ "var"], Class::Keyword),
        (&["bool", "byte", "error", "int", "string", /* ... */], Class::Type),
        (&["true", "false", "nil", "iota"], Class::Constant),
    ],
    after: &[
        ("func", Class::Function),
        ("type", Class::Type),
        (".", Class::Property),
    ],
    capitalized: None,
    other_identifiers: Some(Class::Variable),
};
```

Every rule is a spec field, and every language uses the same fields:

- **Line markers** color from the marker to the end of the line. They match either
  anywhere in the line or only at its start.
- **Delimited regions** are comments and strings. They can be flat or nested, use an
  escape character, and may span lines.
- **Word lists** give keywords, builtin types and constants.
- **`after`** gives a class to the identifier that follows a given token.
- An identifier followed by `(` is a function.
- **`capitalized`** optionally gives capitalized identifiers a class, for languages where
  that convention marks types. ALL_CAPS identifiers are constants.
- **`other_identifiers`** colors remaining words as variables, or leaves them plain.

The same fields cover prose and markup:

| Construct                   | Markdown                                      | HTML / XML                                 |
|-----------------------------|-----------------------------------------------|--------------------------------------------|
| Headings, comments          | `#` line marker at line start                 | `<!-- -->` delimited comment               |
| Code spans and fences       | `` ` `` and ```` ``` ```` delimited strings   |                                            |
| Emphasis                    | `**` and `*` delimited regions                |                                            |
| List and quote markers      | `-`, `*`, `>`, `1.` as operators and numbers  |                                            |
| Tag names, attributes       |                                               | `after` `<` and `</`, `after` space in tag |
| Attribute values            |                                               | `"` delimited strings                      |
| Running text                | `other_identifiers: None`, so it stays plain  | the same                                   |

Vue and Svelte use the HTML spec. PHP uses its own code spec.

## How state crosses lines

The scanner carries one small state from record to record on each side of a hunk:
ground, or inside a delimited region (with its nesting depth). It scans each record's
payload directly, with no projection buffer, no tree and no allocation per token. Only
the spans are allocated.

Expected speed is hundreds of MB/s on ASCII code. That would take the syntax stage on
the 167 MB Kubernetes projection from 25 s to under 1 s.

## What it gives up

- **Types:** a type is recognized only by word lists, `after` rules and capitalization.
  In Go, `*T`, `T{` and generics are often shown as variables.
- **Hunks that start inside a construct:** a hunk that begins inside a block comment or
  a multi-line string is colored as code until the closing delimiter.
- **Interpolation:** `${...}`, `#{...}` and `\(...)` inside strings stay string colored.
- **Regex literals and heredocs:** these are not recognized, so JavaScript, Ruby, Perl
  and shell show them as code.
- **Embedded languages:** these are not recognized. `<script>` and `<style>` bodies,
  PHP's HTML parts, and the bodies of Markdown fences are colored by the outer spec.
- **Markdown text:** apostrophes are not string delimiters, and emphasis that spans lines
  is not tracked.

## Effort and order

1. Build the scanner with Go's spec, then measure the Kubernetes benchmark.
2. Move the other four bundled languages (Rust, C, JavaScript, Python) to it.
3. Add the remaining specs in batches, each with a sample-file test.

The scanner should be about 700 lines. The specs should be about 1,500 lines, mostly
word lists.

## The chain, today and after

### Today

Each step walks the text again, and the syntax step copies and converts it twice:

1. **Decode** (`src/source_text.rs`): each input line is read once through the
   escape-sequence scanner into sanitized text.
2. **Records** (`src/parser/hunk.rs`): the diff parser splits each line into its marker
   and its payload.
3. **Layout** (`src/page_rows.rs`): each file's rows are built in display order, and the
   cells each payload covers are measured.
4. **Syntax** (`src/syntax.rs`):
   1. Each side of a hunk is copied into a joined source buffer (the projection).
   2. Tree-sitter parses that buffer into a tree, and the new side is reparsed as an
      edit of the old side.
   3. The highlight query walks the tree and returns captures. Captures can overlap.
   4. Each capture is mapped back onto the records it covers, into a
      `RecordMap<Vec<SyntaxSpan>>` per file.
5. **Detail** (`src/detail.rs`): changed line pairs are diffed by character.
6. **Styling** (`src/styling.rs`): each row's spans are turned into ordered segments.
   Because spans can overlap, `Partition::of` collects every span edge, sorts them, and
   picks the shortest span covering each piece through a heap. Every row with spans
   pays for the sort and the heap.
7. **Drawing** (`src/terminal/row_writer.rs`) writes the segments.

### After

The scanner replaces steps 4 and 6 for syntax with one step:

- **Styling scans:** while styling a file's rows in display order, styling keeps one
  scanner state per side. Each record's payload is scanned in place with the state of
  its side, and the scanner writes that row's segments directly, in order and never
  overlapping.
- **Context lines** belong to both sides. They are scanned with both states, so each
  side's state stays right, and are colored from the new side as today. When the two
  states are equal, which is the usual case, the line is scanned once.
- **A hunk starts each side in the ground state.**

What disappears:

- The projection buffer and its copy.
- Tree-sitter's tree, the reparse, the query, and the capture-to-record mapping.
- The `RecordMap<Vec<SyntaxSpan>>` handed from syntax to styling, and the
  `SyntaxSpan::capture_length` tie-break.
- The edge sort and heap in `Partition::of` for syntax. They remain only if character
  detail still needs them.
- Extension-only language selection. It becomes a lookup from extension or whole file
  name to a `Spec`.

Measurement follows the change:

- The `language`, `projection`, `parse` and `query` stages become one `scan` stage.
- The `parse_calls`, `captures` and `projected_bytes` counters become scanned bytes and
  segments.
- `bench/kubernetes.sh`, its runner tests and `docs/spec/performance.md` follow.

### Fusions considered and not taken

- **Decoding and highlighting in one pass:** the decoder runs before the file's language
  matters and exists for terminal safety. Fusing them would make safe decoding depend on
  the language.
- **Layout measurement and highlighting in one pass:** layout runs once per file
  independently of color. Fusing them would tie wrapping to the language.

Both extra passes walk clean, cache-hot text at memory speed. If a profile shows either
one costing a perceptible amount, revisit it with that profile.

## Consequence for Tree-sitter

Once all five bundled languages move, no language left uses Tree-sitter for
highlighting. The crate and its grammars would then serve only the planned semantic diff
(roadmap slice 3).
