---
status: accepted
---

# Terminal control-sequence parser

Source: Paul Williams, [A parser for DEC's ANSI-compatible video
terminals](https://vt100.net/emu/dec_ansi_parser), the VT500-series state machine that
follows ECMA-48. Read 2026-10-19.

hdiff's own rules for what it shows are in `docs/spec/architecture.md` under "Lossless
and safe rendering". This note keeps the reference.

## States and actions

States: ground, escape, escape_intermediate, csi_entry, csi_param, csi_intermediate,
csi_ignore, dcs_entry, dcs_param, dcs_intermediate, dcs_passthrough, dcs_ignore,
osc_string and sos_pm_apc_string.

Actions: ignore, print, execute, clear, collect, param, esc_dispatch, csi_dispatch, hook,
put, unhook, osc_start, osc_put and osc_end. Print displays a character, in ground only;
execute runs a C0 or C1 control function. The others act on a sequence's content or its
handler.

## Transitions

Anywhere, in every state: CAN (0x18) and SUB (0x1A) go to ground; ESC goes to escape;
CSI (0x9B) goes to csi_entry; DCS (0x90) to dcs_entry; SOS (0x98), PM (0x9E) and APC
(0x9F) to sos_pm_apc_string; OSC (0x9D) to osc_string; ST (0x9C) and every other C1
control go to ground.

- **Ground:** C0 controls execute; 0x20–0x7E print. DEL is ignored when a 94-character
  set is mapped into GL, and may print with a 96-character set.
- **Escape and escape intermediate:** C0 controls execute in place, and DEL is ignored.
  0x20–0x2F go to (or stay in) escape intermediate. From escape, `[` opens csi_entry,
  `]` osc_string, `P` dcs_entry, and `X`, `^`, `_` sos_pm_apc_string; any other
  0x30–0x7E is a final byte that returns to ground.
- **CSI states:** C0 controls execute in place, DEL is ignored, 0x20–0x3F stay in the
  sequence, and 0x40–0x7E is the final byte that returns to ground.
- **DCS and SOS/PM/APC strings:** everything stays inside the string until an anywhere
  transition; DEL is ignored.
- **OSC string:** as the strings above; the reference ignores C0 controls other than CAN,
  SUB and ESC inside it, BEL included.

xterm, unlike the reference, also ends an OSC string at BEL: its control-sequence
documentation spells an operating system command `OSC Ps ; Pt BEL` as well as
`OSC Ps ; Pt ST` (Thomas E. Dickey, [XTerm Control
Sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html), "Operating System
Commands").

The reference reads GR codes (0xA0–0xFF) like GL codes; it predates UTF-8 input.
