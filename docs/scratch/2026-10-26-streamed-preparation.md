<!-- not spec because: the walked plan for streamed preparation; its decisions are written into docs/spec/ in the first slice of the next run. -->

# Streamed preparation: plan

This is the plan for roadmap product slice 2 (`docs/spec/roadmap.md`), settled with the
user on 2026-10-26. The companion design for the highlighter is
`2026-10-26-generic-highlighter.md`.

## Goals

1. hdiff starts fast, which includes keeping the binary small.
2. Diffing and highlighting feel instant. A 10 ms difference is noticeable, and speed
   means what the user sees, not how soon a batch of work finishes.
3. Highlighting is extensible through Tree-sitter's existing grammars, with no user
   authoring rules. It works fully out of the box, with no downloads.

## Design

- **hdiff's own highlighter:** one fast scanner, with each language described by a data
  table: comment markers, string forms, word lists, and rules about neighbouring tokens.
  The same scanner handles code, markup and prose, and covers the 55 mainstream languages
  listed in `docs/spec/architecture.md#technology-direction`. It colors each line as it
  arrives.
- **Tree-sitter second pass:** after hdiff's own highlighter, hdiff highlights again with
  the `tree-sitter` binary when it is in `PATH`, and its colors replace hdiff's. The pass
  runs as a separate process, in batches, so a slow grammar never delays the screen or
  keys. hdiff links no Tree-sitter itself.
- **Streamed loading:** three threads share nothing and send messages over one
  standard-library channel:
  - **Worker:** reads stdin, parses, and prepares.
  - **Key reader:** reads keys and resizes.
  - **Main:** owns the file list and pages, and draws.

  Keys never wait on loading.
- **Screen:** it opens at the first file header. Content fills in one change block at a
  time, and main redraws once per batch of messages.
- **Rewritten chain:** the whole highlighting path is rewritten rather than patched. The
  projection buffer, the tree, the query and the resolution of overlapping spans all go;
  the scanner writes each row's segments directly.

## Input cases

| Input                                                     | Behavior                                                                 |
|-----------------------------------------------------------|--------------------------------------------------------------------------|
| Zero bytes                                                | exits silently, status 0; the screen never opens                         |
| Not empty, no diff                                        | the screen never opens; the error is reported as today                   |
| A valid diff                                              | shown as it streams                                                      |
| A broken file in the middle                               | that file shows as `X name`; parsing resumes at the next file header     |
| Input that ends part-way through a file                   | that last file shows as `X name`                                         |
| A first file whose content is broken                      | `X first_file`, selected, with its error in the diff pane                |
| A broken file whose header gives no name                  | a name recovered from the readable part of the header, else `X (unnamed)` |
| Comparison output with only surrounding text              | shown when the input ends                                                |

An `X` file shows a best-guess display:

1. Everything parsed before the error, displayed normally.
2. An error row at the failure point, giving the reason.
3. The rest of the broken section, up to the next file header, as raw text. It renders
   like surrounding text: in both split panes, and once in unified view.

hdiff never exits because a file is broken.

## File list during loading

- Files join the list as their headers arrive. They keep their normal `~ + - *` state
  markers; there are no parsing icons.
- While more input is coming, the list ends with a `(parsing...)` line. It is never
  selected, and it disappears when the input ends.
- Arriving files insert above `(parsing...)` without moving the selection. The selection
  follows its file, not a row number, and keeps its screen line when rows are inserted
  above it.
- **`Tab` while loading:** on the last file, `Tab` stops. It does not wrap, does not wait
  for a next file, and nothing switches by itself when a file arrives. You can never
  `Tab` onto the `(parsing...)` line. Once input ends, `Tab` wraps as usual.

## Measurement targets

| Target                                            | Budget                          |
|---------------------------------------------------|---------------------------------|
| Launch to the first frame written                 | 4 ms, one 240 Hz frame          |
| Key press to redraw while the worker is busy      | 1 ms                            |
| A change block's last byte to its frame written   | 4 ms                            |
| Total preparation time                            | no target; a diagnostic only    |

Each clock stops when hdiff has written the frame; the terminal emulator's own latency is
outside it. Retired instructions along the same path remain the primary comparison
metric, per `AGENTS.md`. These targets replace the one-second Kubernetes preparation
target.

## Build order

1. Write the decisions above into `docs/spec/`:
   - `docs/spec/architecture.md`: file tree, `X` marker, `Tab`, strict-parsing and
     failure rules, and the screen opening at the first file header.
   - `docs/spec/performance.md`: preparation boundary and targets.
   - `docs/spec/roadmap.md`.
2. Build the scanner and Go's table, replacing Tree-sitter in the styling chain. Measure
   on the Kubernetes benchmark.
3. Add the Rust, C, JavaScript and Python tables; Tree-sitter leaves the binary.
4. Streamed loading:
   - the threads and channel;
   - a diff parser that accepts input in pieces and resumes after errors;
   - pages that grow one change block at a time;
   - the screen opening at the first file header;
   - the dynamic file list and `Tab` behavior.

   Measure the fixed start-up costs early against the 4 ms budget.
5. Measurement drivers for the three targets.
6. The Tree-sitter second pass. First verify that the `tree-sitter` binary emits
   machine-readable captures. If it does not, bring the alternatives to the user before
   building.
7. The remaining mainstream language tables, in batches.

Dropped: loading grammars from Neovim or Helix installs. The `tree-sitter` second pass
covers it.

Backlog, separate from this plan: Unicode, CJK and Asian script support
(`docs/spec/roadmap.md`, Later work).
