<!-- not spec because: a candidate list for the user to pick from; nothing is chosen yet. -->

# Languages to highlight

Candidate languages for hdiff syntax highlighting. Tick `[x]` for each language to
build. "Effort" is a rough guess at the cost of a hand-rolled lexer/parser good enough
for diff display. "Low" means mostly tokens and keywords. "High" means the language's
quoting, nesting or embedded sublanguages make even rough highlighting tricky.

The five languages bundled today (Rust, JavaScript, Python, Go and C) move to hand-rolled
highlighting under `docs/spec/architecture.md#technology-direction`. Roadmap slice 4
already plans TOML, Markdown and JSON with Tree-sitter.

Languages are chosen by file extension today. Rows marked "name" also need matching by
whole file name, such as `Makefile` or `Dockerfile`.

## Bundled today

| Pick | Language           | Files                              | Effort | Notes                                          |
| ---- | ------------------ | ---------------------------------- | ------ | ---------------------------------------------- |
| [ ]  | Go                 | .go                                | Low    | Kubernetes benchmark corpus; build first       |
| [ ]  | Rust               | .rs                                | Medium | raw strings, lifetimes vs chars, macros        |
| [ ]  | C                  | .c .h                              | Medium | preprocessor lines; .h is shared with C++      |
| [ ]  | JavaScript         | .js .jsx .mjs .cjs                 | High   | regex vs division, template strings, JSX       |
| [ ]  | Python             | .py .pyi                           | Medium | triple-quoted strings, f-strings, decorators   |

## Mainstream

| Pick | Language           | Files                              | Effort | Notes                                          |
| ---- | ------------------ | ---------------------------------- | ------ | ---------------------------------------------- |
| [ ]  | Java               | .java                              | Low    | annotations, text blocks                       |
| [ ]  | C++                | .cc .cpp .cxx .hh .hpp .hxx        | High   | templates, raw strings; shares .h with C       |
| [ ]  | C#                 | .cs                                | Medium | verbatim and interpolated strings              |
| [ ]  | Kotlin             | .kt .kts                           | Medium | string templates                               |
| [ ]  | Swift              | .swift                             | Medium | string interpolation, multi-line strings       |
| [ ]  | PHP                | .php                               | High   | embedded HTML, heredocs                        |
| [ ]  | Ruby               | .rb .rake .gemspec, Gemfile (name) | High   | heredocs, %-literals, regex literals           |
| [ ]  | Shell              | .sh .bash .zsh                     | High   | quoting and expansion rules                    |
| [ ]  | SQL                | .sql                               | Low    | many dialects; keyword set is the work         |
| [ ]  | HTML               | .html .htm                         | Medium | embedded script and style blocks               |
| [ ]  | CSS                | .css                               | Low    |                                                |
| [ ]  | SCSS / Less        | .scss .less                        | Medium | nesting, variables, mixins                     |

## Data, configuration and markup

| Pick | Language           | Files                              | Effort | Notes                                          |
| ---- | ------------------ | ---------------------------------- | ------ | ---------------------------------------------- |
| [ ]  | JSON               | .json .jsonc                       | Low    | roadmap slice 4 (Tree-sitter today)            |
| [ ]  | YAML               | .yml .yaml                         | Medium | indentation, block scalars, anchors            |
| [ ]  | TOML               | .toml                              | Low    | roadmap slice 4 (Tree-sitter today)            |
| [ ]  | Markdown           | .md .markdown                      | Medium | roadmap slice 4; fenced code blocks            |
| [ ]  | XML                | .xml .svg .plist                   | Low    |                                                |
| [ ]  | INI / config       | .ini .cfg .conf                    | Low    |                                                |
| [ ]  | Dockerfile         | Dockerfile (name), .dockerfile     | Low    | embedded shell in RUN lines                    |
| [ ]  | Makefile           | Makefile (name), .mk               | Medium | embedded shell in recipes                      |
| [ ]  | CMake              | CMakeLists.txt (name), .cmake      | Low    |                                                |
| [ ]  | Terraform / HCL    | .tf .tfvars .hcl                   | Low    |                                                |
| [ ]  | Nix                | .nix                               | Medium | indented strings, interpolation                |
| [ ]  | Protocol Buffers   | .proto                             | Low    |                                                |
| [ ]  | GraphQL            | .graphql .gql                      | Low    |                                                |
| [ ]  | CUE                | .cue                               | Low    |                                                |
| [ ]  | Typst              | .typ                               | Medium | markup and code modes mixed                    |
| [ ]  | LaTeX              | .tex .sty                          | Medium |                                                |
| [ ]  | reStructuredText   | .rst                               | Medium |                                                |
| [ ]  | Jsonnet            | .jsonnet .libsonnet                | Low    |                                                |
| [ ]  | Starlark / Bazel   | .bzl, BUILD (name), WORKSPACE      | Low    | Python-like subset                             |

## Moderate user base

| Pick | Language           | Files                              | Effort | Notes                                          |
| ---- | ------------------ | ---------------------------------- | ------ | ---------------------------------------------- |
| [ ]  | Dart               | .dart                              | Medium | string interpolation                           |
| [ ]  | Scala              | .scala .sc                         | Medium | string interpolators, XML literals (Scala 2)   |
| [ ]  | Lua                | .lua                               | Low    | long brackets                                  |
| [ ]  | Perl               | .pl .pm                            | High   | regex and quote-like operators                 |
| [ ]  | R                  | .r .R                              | Low    |                                                |
| [ ]  | Julia              | .jl                                | Medium | string macros, interpolation                   |
| [ ]  | Haskell            | .hs                                | Medium | nested comments, operators, layout             |
| [ ]  | OCaml              | .ml .mli                           | Medium | nested comments                                |
| [ ]  | F#                 | .fs .fsi .fsx                      | Medium |                                                |
| [ ]  | Elixir             | .ex .exs                           | Medium | sigils, heredocs                               |
| [ ]  | Erlang             | .erl .hrl                          | Low    |                                                |
| [ ]  | Clojure            | .clj .cljs .cljc .edn              | Low    | Lisp syntax                                    |
| [ ]  | Zig                | .zig                               | Low    |                                                |
| [ ]  | Nim                | .nim                               | Medium | indentation                                    |
| [ ]  | Objective-C        | .m .mm                             | Medium | .m is shared with MATLAB                       |
| [ ]  | Groovy             | .groovy .gradle                    | Medium | Gradle build files                             |
| [ ]  | PowerShell         | .ps1 .psm1                         | Medium | here-strings                                   |
| [ ]  | Visual Basic       | .vb                                | Low    |                                                |
| [ ]  | Fortran            | .f .f90 .f95                       | Low    | fixed vs free form                             |
| [ ]  | MATLAB             | .m                                 | Low    | .m is shared with Objective-C                  |
| [ ]  | Solidity           | .sol                               | Low    |                                                |
| [ ]  | Svelte             | .svelte                            | High   | embedded script, style and markup              |
| [ ]  | Vue                | .vue                               | High   | embedded script, style and template            |
| [ ]  | Crystal            | .cr                                | Medium | Ruby-like                                      |
| [ ]  | Gleam              | .gleam                             | Low    |                                                |
| [ ]  | Assembly           | .s .S .asm                         | Low    | many dialects                                  |
| [ ]  | CUDA               | .cu .cuh                           | High   | C++ superset                                   |
| [ ]  | GLSL / HLSL        | .glsl .vert .frag .hlsl            | Low    | C-like                                         |
