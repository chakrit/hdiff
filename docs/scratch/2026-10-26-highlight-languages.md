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
| [x]  | Go                 | .go                                | Low    | Kubernetes benchmark corpus; build first       |
| [x]  | Rust               | .rs                                | Medium | raw strings, lifetimes vs chars, macros        |
| [x]  | C                  | .c .h                              | Medium | preprocessor lines; .h is shared with C++      |
| [x]  | JavaScript         | .js .jsx .mjs .cjs                 | High   | regex vs division, template strings, JSX       |
| [x]  | Python             | .py .pyi                           | Medium | triple-quoted strings, f-strings, decorators   |

## Mainstream

| Pick | Language           | Files                              | Effort | Notes                                          |
| ---- | ------------------ | ---------------------------------- | ------ | ---------------------------------------------- |
| [x]  | Java               | .java                              | Low    | annotations, text blocks                       |
| [x]  | C++                | .cc .cpp .cxx .hh .hpp .hxx        | High   | templates, raw strings; shares .h with C       |
| [x]  | C#                 | .cs                                | Medium | verbatim and interpolated strings              |
| [x]  | Kotlin             | .kt .kts                           | Medium | string templates                               |
| [x]  | Swift              | .swift                             | Medium | string interpolation, multi-line strings       |
| [x]  | PHP                | .php                               | High   | embedded HTML, heredocs                        |
| [x]  | Ruby               | .rb .rake .gemspec, Gemfile (name) | High   | heredocs, %-literals, regex literals           |
| [x]  | Shell              | .sh .bash .zsh                     | High   | quoting and expansion rules                    |
| [x]  | SQL                | .sql                               | Low    | many dialects; keyword set is the work         |
| [x]  | HTML               | .html .htm                         | Medium | embedded script and style blocks               |
| [x]  | CSS                | .css                               | Low    |                                                |
| [x]  | SCSS / Less        | .scss .less                        | Medium | nesting, variables, mixins                     |

## Data, configuration and markup

| Pick | Language           | Files                              | Effort | Notes                                          |
| ---- | ------------------ | ---------------------------------- | ------ | ---------------------------------------------- |
| [x]  | JSON               | .json .jsonc                       | Low    | roadmap slice 4 (Tree-sitter today)            |
| [x]  | YAML               | .yml .yaml                         | Medium | indentation, block scalars, anchors            |
| [x]  | TOML               | .toml                              | Low    | roadmap slice 4 (Tree-sitter today)            |
| [x]  | Markdown           | .md .markdown                      | Medium | roadmap slice 4; fenced code blocks            |
| [x]  | XML                | .xml .svg .plist                   | Low    |                                                |
| [x]  | INI / config       | .ini .cfg .conf                    | Low    |                                                |
| [x]  | Dockerfile         | Dockerfile (name), .dockerfile     | Low    | embedded shell in RUN lines                    |
| [x]  | Makefile           | Makefile (name), .mk               | Medium | embedded shell in recipes                      |
| [x]  | CMake              | CMakeLists.txt (name), .cmake      | Low    |                                                |
| [x]  | Terraform / HCL    | .tf .tfvars .hcl                   | Low    |                                                |
| [ ]  | Nix                | .nix                               | Medium | indented strings, interpolation                |
| [x]  | Protocol Buffers   | .proto                             | Low    |                                                |
| [x]  | GraphQL            | .graphql .gql                      | Low    |                                                |
| [ ]  | CUE                | .cue                               | Low    |                                                |
| [ ]  | Typst              | .typ                               | Medium | markup and code modes mixed                    |
| [ ]  | LaTeX              | .tex .sty                          | Medium |                                                |
| [ ]  | reStructuredText   | .rst                               | Medium |                                                |
| [ ]  | Jsonnet            | .jsonnet .libsonnet                | Low    |                                                |
| [ ]  | Starlark / Bazel   | .bzl, BUILD (name), WORKSPACE      | Low    | Python-like subset                             |

## Moderate user base

| Pick | Language           | Files                              | Effort | Notes                                          |
| ---- | ------------------ | ---------------------------------- | ------ | ---------------------------------------------- |
| [x]  | Dart               | .dart                              | Medium | string interpolation                           |
| [x]  | Scala              | .scala .sc                         | Medium | string interpolators, XML literals (Scala 2)   |
| [x]  | Lua                | .lua                               | Low    | long brackets                                  |
| [x]  | Perl               | .pl .pm                            | High   | regex and quote-like operators                 |
| [x]  | R                  | .r .R                              | Low    |                                                |
| [x]  | Julia              | .jl                                | Medium | string macros, interpolation                   |
| [x]  | Haskell            | .hs                                | Medium | nested comments, operators, layout             |
| [x]  | OCaml              | .ml .mli                           | Medium | nested comments                                |
| [x]  | F#                 | .fs .fsi .fsx                      | Medium |                                                |
| [x]  | Elixir             | .ex .exs                           | Medium | sigils, heredocs                               |
| [x]  | Erlang             | .erl .hrl                          | Low    |                                                |
| [x]  | Clojure            | .clj .cljs .cljc .edn              | Low    | Lisp syntax                                    |
| [x]  | Zig                | .zig                               | Low    |                                                |
| [x]  | Nim                | .nim                               | Medium | indentation                                    |
| [x]  | Objective-C        | .m .mm                             | Medium | .m is shared with MATLAB                       |
| [x]  | Groovy             | .groovy .gradle                    | Medium | Gradle build files                             |
| [x]  | PowerShell         | .ps1 .psm1                         | Medium | here-strings                                   |
| [x]  | Visual Basic       | .vb                                | Low    |                                                |
| [ ]  | Fortran            | .f .f90 .f95                       | Low    | fixed vs free form                             |
| [ ]  | MATLAB             | .m                                 | Low    | .m is shared with Objective-C                  |
| [ ]  | Solidity           | .sol                               | Low    |                                                |
| [x]  | Svelte             | .svelte                            | High   | embedded script, style and markup              |
| [x]  | Vue                | .vue                               | High   | embedded script, style and template            |
| [x]  | Crystal            | .cr                                | Medium | Ruby-like                                      |
| [x]  | Gleam              | .gleam                             | Low    |                                                |
| [x]  | Assembly           | .s .S .asm                         | Low    | many dialects                                  |
| [x]  | CUDA               | .cu .cuh                           | High   | C++ superset                                   |
| [ ]  | GLSL / HLSL        | .glsl .vert .frag .hlsl            | Low    | C-like                                         |
