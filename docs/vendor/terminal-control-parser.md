---
status: accepted
---

# Terminal control-sequence parser

Source: Paul Williams, [A parser for DEC's ANSI-compatible video
terminals](https://vt100.net/emu/dec_ansi_parser), the VT500-series state machine that
follows ECMA-48. Read 2026-10-19.

`src/source_text.rs` decides which input characters are visible text with this
machine. hdiff takes none of its actions (collect, param, dispatch, put); it keeps only
whether a character shows and which state follows.

## States

The reference has fourteen states. hdiff keeps five, because the reference's substates
differ only in actions hdiff does not take:

| hdiff state          | Reference states                                         |
|----------------------|----------------------------------------------------------|
| `Ground`             | ground                                                   |
| `Escape`             | escape                                                   |
| `EscapeIntermediate` | escape_intermediate                                      |
| `ControlSequence`    | csi_entry, csi_param, csi_intermediate, csi_ignore       |
| `CommandString`      | dcs_entry, dcs_param, dcs_intermediate, dcs_passthrough, |
|                      | dcs_ignore, osc_string, sos_pm_apc_string                |

Every CSI substate stays inside the sequence on 0x20–0x3F and ends it on 0x40–0x7E.
Every DCS, OSC and SOS/PM/APC state ends only at ST or an anywhere transition.

## Transitions

Anywhere, in every state: CAN (0x18) and SUB (0x1A) go to ground; ESC goes to escape;
CSI (0x9B) goes to the control sequence; DCS (0x90), SOS (0x98), OSC (0x9D), PM (0x9E)
and APC (0x9F) go to the command string; ST (0x9C) and every other C1 control go to
ground.

- **Ground:** tab and printable characters show; other C0 controls, DEL and C1
  controls do not.
- **Escape:** C0 controls and DEL execute in place. 0x20–0x2F go to escape
  intermediate. `[` opens a control sequence; `]`, `P`, `X`, `^` and `_` open a command
  string; any other 0x30–0x7E is a final byte that returns to ground.
- **Escape intermediate:** C0 controls and DEL execute in place, 0x20–0x2F stay, and
  0x30–0x7E is the final byte that returns to ground.
- **Control sequence:** C0 controls and DEL execute in place, 0x20–0x3F stay, and
  0x40–0x7E is the final byte that returns to ground.
- **Command string:** everything stays inside the string until an anywhere transition.

## hdiff departures from the reference

- **BEL ends a command string.** xterm accepts BEL as an OSC terminator, and hdiff ends
  every command string at it.
- **Visible characters break an escape.** ECMA-48 allows only 0x20–0x7E after ESC. A
  tab or a non-ASCII printable character read in escape or escape intermediate ends the
  escape and shows, so no visible source text is lost (`ESC é` shows `é`). The reference
  treats GR codes like GL codes; it predates UTF-8 input.
- **A shown character returns to ground.** Visible text after a shown character is
  decoded from plain text, so a tab inside a control sequence stays part of the sequence
  rather than showing, unlike the reference, which executes it there.

## Open question

What a non-ASCII character inside a control sequence does is undecided. Terminals
disagree on this malformed input; hdiff keeps it part of the sequence until a final
byte.
