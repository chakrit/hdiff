<!-- not spec because: a proposed design for the spec's streamed preparation boundary, awaiting the user's review. -->

# Streamed preparation: how it works

This design implements the preparation boundary in `docs/spec/performance.md`. Its goal
is the shortest time until the user sees the first screen and the shortest delay after a
key press. How long the whole diff takes to finish does not matter, because that is not
what the user perceives.

## Threads

There are three threads, and they share nothing. Each one only sends messages to the
main thread over one channel from the standard library.

| Thread  | Does                                                                         | Never does           |
|---------|------------------------------------------------------------------------------|----------------------|
| Worker  | reads stdin, parses the diff, and prepares each file (layout, scanner colors, character detail) | touch the terminal |
| Keys    | reads key and resize events from the controlling terminal                    | prepare anything     |
| Main    | owns the file list and pages, handles each message, draws                    | parse or prepare     |

The worker sends these messages:

- `FileStarted(header)` when a file's header has been read.
- `FileReady(page)` when the file's last hunk has been read and the file is prepared.
- `FileFailed(error)` when the file's input fails to parse.
- `InputEnded` at the end of input.

The keys thread sends `Key` and `Resize`. The main thread blocks on the channel, so it
wakes as soon as either a key or a file arrives. It never polls on a timer and never
waits on loading.

A prepared page owns its file's data. It moves from the worker to the main thread whole,
with no locks and no shared references.

## Timeline

```text
stdin  ──bytes──▶ worker: parse ─▶ file 1 done ─▶ prepare 1 ─▶ FileReady(1) ─▶ …next file
                                                                    │
main:  wait ─────────────────────────────────────────────────────── ▼ open terminal, draw file 1
                                                                       │
keys:  ───────────────────────────────── Key ──────────────────────▶ handle, redraw
```

1. **Before the first file:** hdiff does not open the terminal UI until the first file
   is ready. Empty input still exits silently, and a document with no files is shown as
   text when its input ends, as today.
2. **First screen:** the moment the first file is ready, the main thread opens the
   terminal and draws it. Text before the first file, such as a `git show` commit
   message, belongs to that page as today.
3. **While loading:** the worker keeps reading and preparing in arrival order. Each
   `FileStarted` adds a row with the "parsing" icon. Each `FileReady` removes the icon,
   and `FileFailed` replaces it with the "error" icon.
4. **End of input:** the list is complete. Nothing else changes.

Pressing `q` while files are loading exits at once. The worker dies with the process,
and the producer (for example, Git) sees a closed pipe, which hdiff already handles
quietly.

## File list

The ordering rule stays as it is: top-level files first, then every other file, each in
input order, with directory headings. Git sorts paths, so top-level files such as
`go.mod` often arrive after nested ones and are inserted above them.

The user's focus never shifts:

- The selection follows the selected file, not a row number.
- When rows are inserted above the selection, the list window moves with them. The
  selected row stays on the same screen line, and the diff pane does not change.
- The first file shown is the first file to arrive. After loading, a top-level file may
  sit above it in the list.

Directory headings are recomputed as files arrive. One subtlety: a directory whose only
content is one subdirectory shares that subdirectory's heading. A later file can split
such a merged heading back into two. The split happens around the selection without
moving it.

## `Tab` during the load

- `Tab` and `Shift-Tab` move through the list as it stands at that moment.
- **Onto a file still parsing:** `Tab` selects it, and the diff pane shows it when it
  becomes ready. With the scanner that is milliseconds after its last hunk arrives.
- **At the last row, while more input is still coming:** `Tab` wraps to the first file,
  the same as after loading. Waiting at the last row for the next file to arrive is the
  alternative, but then a key does nothing visible.

## Errors

The diff parser stays strict. When a file's input fails to parse, that file shows the
"error" icon, and selecting it shows the error message in the diff pane. Files that are
already ready stay usable. Parsing stops at the first error, as it does today. Whether
hdiff should exit with an error status after the user quits is still to decide.

## Where the scanner and Tree-sitter fit

- **The scanner** runs inside the worker's prepare step. It is fast enough that a file
  is ready almost as soon as its last byte arrives.
- **Tree-sitter plugins**, if added later, would run apart from the worker, so a slow
  grammar never delays later files. The page first appears with plain or scanner
  colors, and the plugin's colors replace them when they arrive. This would need one
  more message, `Recolored(file, styles)`.

## Measurement

The Kubernetes target stops being total preparation time. The new targets are:

- **First screen:** time from launch to the first frame drawn.
- **Key latency while loading:** the delay between a key press and its redraw, while the
  worker is busy.

`--bench` grows drivers for both, and the existing stage profile remains a diagnostic.

## Open points for the user

- At the last row during load, should `Tab` wrap to the first file or wait for the next
  one to arrive?
- Should an "error" file make hdiff exit with an error status?
