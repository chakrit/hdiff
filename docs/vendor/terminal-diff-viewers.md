# Terminal diff viewers

Third-party prior art for hdiff's terminal lifecycle, navigation, layout, and failure
handling. Source snapshots were reviewed in August 2026.

## Delta

Delta is a formatter that launches an external `less` pager. It transforms a stream once,
captures terminal width during option construction, and delegates keyboard input, scrolling,
search, and resize behavior to `less`.

- [Pager setup](https://github.com/dandavison/delta/blob/95a0e224f55ccfdf3a7d1278fdea98a3edb9fbf4/src/utils/bat/output.rs#L79-L169)
- [Width snapshot](https://github.com/dandavison/delta/blob/95a0e224f55ccfdf3a7d1278fdea98a3edb9fbf4/src/options/set.rs#L605-L619)
- [Navigation](https://github.com/dandavison/delta/blob/95a0e224f55ccfdf3a7d1278fdea98a3edb9fbf4/src/features/navigate.rs#L32-L90)
- [ANSI ingestion and fallback](https://github.com/dandavison/delta/blob/95a0e224f55ccfdf3a7d1278fdea98a3edb9fbf4/src/delta.rs#L192-L253)

Delta's useful lessons are to separate ANSI-free parsing from display styling, preserve
unknown lines, and treat broken pipes as normal termination only for finite output. Its
external-pager boundary is not suitable for hdiff.

## Tig

Tig is the primary interactive-terminal reference. It distinguishes diff input from the
controlling terminal, opens `/dev/tty` for interaction when input is piped, handles resize as
an input event, keeps semantic position separate from viewport offsets, and performs explicit
terminal cleanup.

- [Startup and pager mode](https://github.com/jonas/tig/blob/1b86f070a1f6d4c686a09b997fd4249d52a2a272/src/tig.c#L813-L849)
- [Controlling-terminal setup](https://github.com/jonas/tig/blob/1b86f070a1f6d4c686a09b997fd4249d52a2a272/src/display.c#L630-L657)
- [Position model](https://github.com/jonas/tig/blob/1b86f070a1f6d4c686a09b997fd4249d52a2a272/include/tig/view.h#L83-L121)
- [Resize handling](https://github.com/jonas/tig/blob/1b86f070a1f6d4c686a09b997fd4249d52a2a272/src/display.c#L820-L863)
- [Terminal cleanup](https://github.com/jonas/tig/blob/1b86f070a1f6d4c686a09b997fd4249d52a2a272/src/display.c#L595-L618)

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

- [Terminal and long-line fixes](https://github.com/Wilfred/difftastic/blob/9e2d60c882b523e75a1d2fae8d5134ff506743d6/CHANGELOG.md#L324-L334)
- [Long-line cases](https://github.com/Wilfred/difftastic/blob/9e2d60c882b523e75a1d2fae8d5134ff506743d6/CHANGELOG.md#L382-L383)
