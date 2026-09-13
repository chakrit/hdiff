# Terminal diff viewers

Third-party prior art for hdiff's terminal lifecycle, navigation, layout, and failure
handling. Source snapshots were reviewed in August 2026; their original source links
are retained in `.ace/spec-cleanup-vendor-before.md`. Links below open the current
source files rather than the reviewed snapshots.

## Delta

Delta is a formatter that launches an external `less` pager. It transforms a stream once,
captures terminal width during option construction, and delegates keyboard input, scrolling,
search, and resize behavior to `less`.

- [Pager setup](https://github.com/dandavison/delta/blob/HEAD/src/utils/bat/output.rs)
- [Width snapshot](https://github.com/dandavison/delta/blob/HEAD/src/options/set.rs)
- [Navigation](https://github.com/dandavison/delta/blob/HEAD/src/features/navigate.rs)
- [ANSI ingestion and fallback](https://github.com/dandavison/delta/blob/HEAD/src/delta.rs)

Delta's useful lessons are to separate ANSI-free parsing from display styling, preserve
unknown lines, and treat broken pipes as normal termination only for finite output. Its
external-pager boundary is not suitable for hdiff.

## Tig

Tig is the primary interactive-terminal reference. It distinguishes diff input from the
controlling terminal, opens `/dev/tty` for interaction when input is piped, handles resize as
an input event, keeps semantic position separate from viewport offsets, and performs explicit
terminal cleanup.

- [Startup and pager mode](https://github.com/jonas/tig/blob/HEAD/src/tig.c)
- [Controlling-terminal setup](https://github.com/jonas/tig/blob/HEAD/src/display.c)
- [Position model](https://github.com/jonas/tig/blob/HEAD/include/tig/view.h)
- [Resize handling](https://github.com/jonas/tig/blob/HEAD/src/display.c)
- [Terminal cleanup](https://github.com/jonas/tig/blob/HEAD/src/display.c)

Tig also pre-wraps pager text at ingestion width, so hdiff should retain semantic anchors and
derive wrapped rows again after every resize rather than treating generated rows as state.

For hdiff, the Tig study adds four constraints: decide whether the output contract is
interactive before opening a controlling terminal; make terminal setup and cleanup staged and
idempotent; normalize the semantic selection anchor into the new viewport after every resize;
and require a backend API that reads events from an explicit controlling-terminal handle
instead of rebinding standard input. Tig's signal-handler cleanup is evidence for the scope of
cleanup, not a Rust implementation pattern to copy.

## Difftastic

Difftastic's changelog records failures involving missing terminal dimensions, non-ASCII long
lines, runaway memory, and closed pipes. These cases support bounding visible-row work while
keeping the authoritative diff document lossless.

- [Terminal and long-line fixes](https://github.com/Wilfred/difftastic/blob/HEAD/CHANGELOG.md)
